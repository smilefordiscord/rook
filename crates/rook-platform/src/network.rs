//! HTTP requests: asset delivery, Roblox's web APIs and scripts' own
//! requests all go out through [`Network`]. Its parts: which hosts a
//! request may reach ([`reach`]), and the desktop's client (`Ureq`,
//! behind `native`).

pub mod reach;
#[cfg(feature = "native")]
mod ureq;

use std::fmt::Debug;
use std::time::Duration;

#[cfg(feature = "native")]
pub use self::ureq::Ureq;

#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// `GET`, `POST`, ...
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    /// Sent when not empty.
    pub body: &'a [u8],
    /// A longer answer fails with [`NetworkError::TooLarge`].
    pub max_bytes: u64,
    /// How long the whole request may take, in place of the network's
    /// own limit.
    pub timeout: Option<Duration>,
    pub reach: Reach,
}

/// How long a request may take unless its [`Request::timeout`] says
/// otherwise.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// Which hosts a request may reach. The network checks every hop,
/// redirects included, and every address a host's name resolves to, and
/// connects only to addresses it checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reach {
    /// Anywhere: Rook's own requests, to the endpoints its host chose.
    #[default]
    Any,
    /// Where a game's script may send one (HttpService): never Roblox's
    /// web domains, and only addresses on the public internet unless
    /// `local` ([`reach::is_public`]).
    Script { local: bool },
}

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
    /// A header or method HTTP doesn't allow, so nothing could be sent.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
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
    /// The answer's body is longer than [`Request::max_bytes`], this many.
    #[error("the answer is over {0} bytes")]
    TooLarge(u64),
    /// A hop's host is one of Roblox's web domains, which
    /// [`Reach::Script`] never reaches.
    #[error("{0} is a Roblox web domain")]
    RobloxDomain(String),
    /// A hop's host has no address on the public internet, which
    /// [`Reach::Script`] reaches only with `local`.
    #[error("{0} has no public address")]
    NotPublic(String),
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
    /// at all (no connection, a timeout, a host [`Request::reach`]
    /// refuses).
    fn send(&self, request: &Request<'_>) -> Result<Response, NetworkError>;
}
