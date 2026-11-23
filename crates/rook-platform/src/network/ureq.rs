//! The desktop's [`Network`]: HTTPS through ureq (rustls, gzip),
//! [`TIMEOUT`] per request unless it sets its own, following up to 10 redirects. Each
//! [`Reach`] has an agent of its own, as ureq resolves names per agent:
//! the scripts' agents resolve through [`Checked`], which every hop's
//! connection goes through, so a redirect is checked like the first
//! request and a connection is made only to an address that was.

use std::io::Read;

use ::ureq::config::Config;
use ::ureq::http::Uri;
use ::ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ::ureq::unversioned::transport::{DefaultConnector, NextTimeout};
use ::ureq::{Agent, Error};

use super::{Network, NetworkError, Reach, Request, Response, TIMEOUT, reach};

#[derive(Debug)]
pub struct Ureq {
    any: Agent,
    /// [`Reach::Script`] without and with `local`.
    script: Agent,
    script_local: Agent,
}

impl Ureq {
    pub fn new() -> Self {
        let config = || {
            Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .http_status_as_error(false)
                .user_agent(concat!(
                    "Rook/",
                    env!("CARGO_PKG_VERSION"),
                    " (+https://github.com/AsyncFinn/rook)"
                ))
        };
        let script = |local| {
            // No proxy: one would resolve the host itself, out of
            // `Checked`'s sight.
            let config = config().proxy(None).build();
            let resolver = Checked {
                reach: Reach::Script { local },
                dns: DefaultResolver::default(),
            };
            Agent::with_parts(config, DefaultConnector::new(), resolver)
        };
        Self {
            any: config().build().into(),
            script: script(false),
            script_local: script(true),
        }
    }

    fn agent(&self, reach: Reach) -> &Agent {
        match reach {
            Reach::Any => &self.any,
            Reach::Script { local: false } => &self.script,
            Reach::Script { local: true } => &self.script_local,
        }
    }
}

impl Default for Ureq {
    fn default() -> Self {
        Self::new()
    }
}

impl Network for Ureq {
    fn send(&self, request: &Request<'_>) -> Result<Response, NetworkError> {
        let agent = self.agent(request.reach);
        let mut call = ::ureq::http::Request::builder()
            .method(request.method)
            .uri(request.url);
        for (name, value) in request.headers {
            call = call.header(*name, *value);
        }
        let mut call = call
            .body(request.body.to_vec())
            .map_err(|e| builder_error(&e))?;
        if let Some(timeout) = request.timeout {
            call = agent
                .configure_request(call)
                .timeout_global(Some(timeout))
                .build();
        }
        let mut response = agent.run(call).map_err(network_error)?;
        let headers = response
            .headers()
            .iter()
            .filter_map(|(n, v)| Some((n.as_str().to_owned(), v.to_str().ok()?.to_owned())))
            .collect();
        let mut body = Vec::new();
        response
            .body_mut()
            .with_config()
            .limit(request.max_bytes)
            .reader()
            .read_to_end(&mut body)
            .map_err(|e| network_error(e.into()))?;
        let status = response.status();
        Ok(Response {
            status: status.as_u16(),
            reason: status.canonical_reason().unwrap_or_default().to_owned(),
            headers,
            body,
        })
    }
}

/// Resolves a host's name as ureq does, keeping only the addresses
/// `reach` allows; a host it refuses, or one with none left, fails with
/// [`NetworkError`] inside [`Error::Other`].
#[derive(Debug)]
struct Checked {
    reach: Reach,
    dns: DefaultResolver,
}

impl Resolver for Checked {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, Error> {
        let host = uri.host().unwrap_or_default();
        if let Some(refused) = reach::refuse_host(self.reach, host) {
            return Err(Error::Other(Box::new(refused)));
        }
        let found = self.dns.resolve(uri, config, timeout)?;
        let mut kept = self.empty();
        for addr in found.iter().filter(|a| reach::allows(self.reach, a.ip())) {
            kept.push(*addr);
        }
        if kept.is_empty() {
            return Err(Error::Other(Box::new(NetworkError::NotPublic(
                host.to_owned(),
            ))));
        }
        Ok(kept)
    }
}

/// What kind of failure building a request is: its URL, or else a header
/// or method HTTP doesn't allow.
fn builder_error(e: &::ureq::http::Error) -> NetworkError {
    use ::ureq::http::uri::{InvalidUri, InvalidUriParts};
    if e.is::<InvalidUri>() || e.is::<InvalidUriParts>() {
        NetworkError::InvalidUrl(e.to_string())
    } else {
        NetworkError::InvalidRequest(e.to_string())
    }
}

/// What kind of failure a ureq error is.
fn network_error(e: Error) -> NetworkError {
    use std::io::ErrorKind;
    let text = e.to_string();
    match e {
        Error::BadUri(_) | Error::RequireHttpsOnly(_) => NetworkError::InvalidUrl(text),
        Error::Http(e) => builder_error(&e),
        Error::HostNotFound => NetworkError::DnsResolve(text),
        Error::ConnectionFailed | Error::ConnectProxyFailed(_) => NetworkError::ConnectFail(text),
        Error::Timeout(_) => NetworkError::TimedOut(text),
        Error::TooManyRedirects => NetworkError::TooManyRedirects,
        Error::RedirectFailed => NetworkError::InvalidRedirect(text),
        Error::Tls(_) | Error::Pem(_) | Error::Rustls(_) | Error::TlsRequired => {
            NetworkError::Tls(text)
        }
        Error::BodyExceedsLimit(limit) => NetworkError::TooLarge(limit),
        // What `Checked` refused.
        Error::Other(other) => match other.downcast::<NetworkError>() {
            Ok(refused) => *refused,
            Err(_) => NetworkError::Other(text),
        },
        Error::Io(io) => match io.kind() {
            ErrorKind::ConnectionRefused
            | ErrorKind::HostUnreachable
            | ErrorKind::NetworkUnreachable => NetworkError::ConnectFail(text),
            ErrorKind::TimedOut => NetworkError::TimedOut(text),
            ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::UnexpectedEof => NetworkError::ConnectionClosed(text),
            _ => NetworkError::Other(text),
        },
        _ => NetworkError::Other(text),
    }
}
