//! HTTP requests: asset delivery, Roblox's web APIs and scripts' own
//! requests all go out through [`Network`].

use std::fmt::Debug;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// `GET`, `POST`, ...
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    /// Sent when not empty.
    pub body: &'a [u8],
    /// The answer's body is cut off past this.
    pub max_bytes: u64,
    /// How long the whole request may take, when shorter than the
    /// network's own limit.
    pub timeout: Option<Duration>,
}

/// How long a request may take at most; [`Request::timeout`] may only
/// shorten it.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// An answer, whatever its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// The standard reason phrase for the status (`Not Found`), whatever
    /// the server wrote (HTTP/2 sends none); empty for a code with none.
    pub reason: String,
    /// Names in lowercase, as HTTP/2 sends them.
    pub headers: Vec<(String, String)>,
    /// Decompressed.
    pub body: Vec<u8>,
}

/// Why no answer came. The kinds follow what an HTTP client can tell
/// apart (and Roblox's `Enum.HttpError` names); each keeps the client's
/// description.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("couldn't resolve the host: {0}")]
    DnsResolve(String),
    #[error("couldn't connect: {0}")]
    ConnectFail(String),
    #[error("timed out: {0}")]
    TimedOut(String),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("invalid redirect: {0}")]
    InvalidRedirect(String),
    #[error("TLS failed: {0}")]
    Tls(String),
    #[error("connection closed: {0}")]
    ConnectionClosed(String),
    #[error("{0}")]
    Other(String),
}

impl Response {
    /// A header's value; names are compared ignoring case, as HTTP's are.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub trait Network: Send + Sync + Debug {
    /// Blocks until the answer has arrived. `Err` is a failure to get one
    /// at all (no connection, a timeout).
    fn send(&self, request: &Request<'_>) -> Result<Response, NetworkError>;
}

/// HTTPS through ureq (rustls, gzip), [`TIMEOUT`] per request at most,
/// following up to 10 redirects.
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct Ureq {
    agent: ureq::Agent,
}

#[cfg(feature = "native")]
impl Ureq {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .user_agent(concat!(
                "Rook/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/AsyncFinn/rook)"
            ))
            .build()
            .into();
        Self { agent }
    }
}

#[cfg(feature = "native")]
impl Default for Ureq {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
impl Network for Ureq {
    fn send(&self, request: &Request<'_>) -> Result<Response, NetworkError> {
        use std::io::Read;
        let mut call = ureq::http::Request::builder()
            .method(request.method)
            .uri(request.url);
        for (name, value) in request.headers {
            call = call.header(*name, *value);
        }
        let mut call = call
            .body(request.body.to_vec())
            .map_err(|e| NetworkError::InvalidUrl(e.to_string()))?;
        if let Some(timeout) = request.timeout {
            call = self
                .agent
                .configure_request(call)
                .timeout_global(Some(timeout))
                .build();
        }
        let mut response = self.agent.run(call).map_err(network_error)?;
        let headers = response
            .headers()
            .iter()
            .filter_map(|(n, v)| Some((n.as_str().to_owned(), v.to_str().ok()?.to_owned())))
            .collect();
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(request.max_bytes)
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

/// What kind of failure a ureq error is.
#[cfg(feature = "native")]
fn network_error(e: ureq::Error) -> NetworkError {
    use std::io::ErrorKind;
    let text = e.to_string();
    match e {
        ureq::Error::BadUri(_) | ureq::Error::Http(_) | ureq::Error::RequireHttpsOnly(_) => {
            NetworkError::InvalidUrl(text)
        }
        ureq::Error::HostNotFound => NetworkError::DnsResolve(text),
        ureq::Error::ConnectionFailed | ureq::Error::ConnectProxyFailed(_) => {
            NetworkError::ConnectFail(text)
        }
        ureq::Error::Timeout(_) => NetworkError::TimedOut(text),
        ureq::Error::TooManyRedirects => NetworkError::TooManyRedirects,
        ureq::Error::RedirectFailed => NetworkError::InvalidRedirect(text),
        ureq::Error::Tls(_)
        | ureq::Error::Pem(_)
        | ureq::Error::Rustls(_)
        | ureq::Error::TlsRequired => NetworkError::Tls(text),
        ureq::Error::Io(io) => match io.kind() {
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
