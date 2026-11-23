//! Scripts' web requests (`rook_lua::WebRequest`), each sent on a thread
//! of its own through the host's network, their answers delivered on the
//! engine thread. Members that use them are rare and wait on a person or
//! a server, so no pool is kept. The frame never waits for an answer, on
//! any host: one arrives on the first frame after it came back, as on
//! Roblox, so a slow or silent server holds up only the script that
//! asked, never the DataModel.

use std::sync::mpsc::{Receiver, Sender, channel};

use rook_lua::{WebAnswer, WebRequest};
use rook_platform::network::{NetworkError, Request};

/// A longer answer fails (Roblox's web APIs answer in kilobytes;
/// HttpService's limit is in `docs/behavior/scripting/http-requests.md`).
const MAX_BYTES: u64 = 16 << 20;

#[derive(Debug)]
pub(crate) struct Web {
    sender: Sender<(u64, WebAnswer)>,
    answers: Receiver<(u64, WebAnswer)>,
}

impl Default for Web {
    fn default() -> Self {
        let (sender, answers) = channel();
        Self { sender, answers }
    }
}

impl Web {
    pub(crate) fn send(&self, id: u64, request: WebRequest) {
        let sender = self.sender.clone();
        let body = Box::new(move || {
            let headers: Vec<(&str, &str)> = request
                .headers
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .collect();
            let answer = rook_platform::get().network.send(&Request {
                method: &request.method,
                url: &request.url,
                headers: &headers,
                body: &request.body,
                max_bytes: MAX_BYTES,
                timeout: request.timeout,
                reach: request.reach,
            });
            if let Err(e) = &answer {
                // An invalid URL's description holds the URL itself.
                let why = match e {
                    NetworkError::InvalidUrl(_) => "invalid URL".to_owned(),
                    e => e.to_string(),
                };
                tracing::warn!(target: "web", "{} {}: {why}", request.method, loggable(&request.url));
            }
            // The DataModel may be gone; its answer is then for no one.
            let _ = sender.send((id, answer));
        });
        if let Err(e) = rook_platform::get().threads.spawn("rook-web", body) {
            let _ = self.sender.send((
                id,
                Err(NetworkError::Other(format!("couldn't start a thread: {e}"))),
            ));
        }
    }

    /// The answers that came back since the last call; never waits.
    pub(crate) fn finished(&self) -> Vec<(u64, WebAnswer)> {
        self.answers.try_iter().collect()
    }
}

/// `url` as the log may show it: its scheme, host, port and path, without
/// the login, query or fragment, which can hold a secret (a webhook's
/// token, an API key).
fn loggable(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return "<not a URL>".to_owned();
    };
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let rest = &rest[..end];
    let (authority, path) = rest
        .find(['/', '\\'])
        .map_or((rest, ""), |at| rest.split_at(at));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_urls_keep_no_login_query_or_fragment() {
        for (url, logged) in [
            (
                "https://example.com/hooks/1?key=k&wait=true",
                "https://example.com/hooks/1",
            ),
            (
                "https://user:secret@example.com:8080/a/b?key=k#frag",
                "https://example.com:8080/a/b",
            ),
            ("http://example.com?token=t", "http://example.com"),
            ("http://example.com#token", "http://example.com"),
            ("http://a@b@example.com/x", "http://example.com/x"),
            ("example.com/x?y=1", "<not a URL>"),
        ] {
            assert_eq!(loggable(url), logged, "{url}");
        }
    }
}
