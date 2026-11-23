//! Web requests scripts wait on: to Roblox's web domains, for the members
//! Roblox answers from its servers (users, filtering, CoreScripts'
//! `HttpRbxApiService`), and HttpService's to any site.
//!
//! Each member on a Roblox domain has two answers: with no endpoint for its domain
//! ([`rook_platform::endpoints`]), a local one; with one, a request in
//! Roblox's shape. Requests wait in the World ([`WebRequests`]) for the
//! engine to send (`Runtime::take_web_requests`), and the calling thread
//! waits for the answer (`Runtime::deliver_web`), so this crate never
//! touches the network. Which member asks which domain for what is in
//! `docs/design/web-endpoints.md`.

use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use mlua::{Function, Lua, MultiValue, Table, Thread, Value as LuaValue};
use rook_platform::endpoints::Endpoints;
use rook_platform::network::{NetworkError, Reach, Response};

use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, api_fn, with_world};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// How long it may take, when shorter than the network's own limit.
    pub timeout: Option<Duration>,
    pub reach: Reach,
}

impl WebRequest {
    pub(crate) fn get(url: String) -> Self {
        Self {
            method: "GET".into(),
            url,
            headers: Vec::new(),
            body: Vec::new(),
            timeout: None,
            reach: Reach::Any,
        }
    }

    /// A POST of a JSON body, as Roblox's web APIs take.
    pub(crate) fn post_json(url: String, body: &serde_json::Value) -> Self {
        Self {
            method: "POST".into(),
            url,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_string().into_bytes(),
            timeout: None,
            reach: Reach::Any,
        }
    }
}

/// What came back, whatever its status, or why nothing did.
pub type WebAnswer = Result<Response, NetworkError>;

/// Turns an answer into what the waiting member returns, or the error
/// it raises.
pub(crate) type Decode = Box<dyn FnOnce(&Lua, WebAnswer) -> Result<MultiValue, String>>;

/// Turns an answer into the next request a member sends, and what
/// decodes that one's answer, or the error it raises.
pub(crate) type Then = Box<dyn FnOnce(&Lua, WebAnswer) -> Result<(WebRequest, Decode), String>>;

/// What is done with a request's answer.
enum Step {
    Decode(Decode),
    Then(Then),
}

/// Web requests scripts are waiting on, and where they go.
#[derive(Default)]
pub struct WebRequests {
    pub endpoints: Arc<Endpoints>,
    /// Requests to send, by request id, since the engine last took them.
    pub(crate) outbox: Vec<(u64, WebRequest)>,
    waiting: FxHashMap<u64, (Option<Thread>, Step)>,
    next: u64,
    /// When each of HttpService's requests in the last minute went out
    /// (scheduler seconds, oldest first), for its per-minute limit.
    pub(crate) script_sent: VecDeque<f64>,
    /// HttpService's requests may reach local addresses (the host's
    /// `httpLocalAddresses` setting, a Rook extension).
    pub local_addresses: bool,
}

impl std::fmt::Debug for WebRequests {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebRequests")
            .field("endpoints", &self.endpoints)
            .field("outbox", &self.outbox)
            .field("waiting", &self.waiting.len())
            .finish_non_exhaustive()
    }
}

/// `path` on `domain`'s endpoint, or `None` when the domain answers
/// locally.
pub(crate) fn url(lua: &Lua, domain: &str, path: &str) -> Option<String> {
    with_world(lua, |w| w.web.endpoints.url(domain, path))
}

/// A Roblox web URL as sent, or `None` when its domain has no endpoint.
pub(crate) fn rewrite(lua: &Lua, url: &str) -> Option<String> {
    with_world(lua, |w| w.web.endpoints.rewrite(url))
}

/// Send `request` and return what `decode` makes of the answer; the
/// calling thread waits meanwhile, and a decode error is raised.
pub(crate) fn call(lua: &Lua, request: WebRequest, decode: Decode) -> ApiResult<Outcome> {
    wait_on(lua, request, Step::Decode(decode))
}

/// [`call`] for a member that needs two requests: `then` makes the second
/// from the first's answer, and the thread waits for both.
pub(crate) fn call_then(lua: &Lua, request: WebRequest, then: Then) -> ApiResult<Outcome> {
    wait_on(lua, request, Step::Then(then))
}

/// Queue `request` for `step`, with `waiter` waiting on it; its id.
fn queue(
    w: &mut crate::runtime::world::World,
    request: WebRequest,
    waiter: Option<Thread>,
    step: Step,
) -> u64 {
    let web = &mut w.web;
    web.next += 1;
    web.outbox.push((web.next, request));
    web.waiting.insert(web.next, (waiter, step));
    web.next
}

fn wait_on(lua: &Lua, request: WebRequest, step: Step) -> ApiResult<Outcome> {
    let id = with_world(lua, |w| queue(w, request, None, step));
    let helper: Function = lua.named_registry_value("rook.web_wait")?;
    Ok(Outcome::Call(
        helper,
        MultiValue::from_vec(vec![LuaValue::Number(id as f64)]),
    ))
}

/// Send `request` for Rook itself: no script waits, and `decode` runs on
/// the answer for what it does to the World.
pub(crate) fn send(lua: &Lua, request: WebRequest, decode: Decode) {
    with_world(lua, |w| queue(w, request, None, Step::Decode(decode)));
}

/// The JSON body of a 2xx answer, or the error a member raises for it.
pub(crate) fn json(answer: WebAnswer) -> Result<serde_json::Value, String> {
    let response = answer.map_err(|e| format!("HttpError: ConnectFail ({e})"))?;
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP {}", response.status));
    }
    serde_json::from_slice(&response.body).map_err(|_| "Can't parse JSON".to_owned())
}

/// Attach the current thread to request `id`.
fn wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "web");
    let id = a.number(0)? as u64;
    let thread = lua.current_thread();
    with_world(lua, |w| {
        if let Some((waiter, _)) = w.web.waiting.get_mut(&id) {
            *waiter = Some(thread);
        }
    });
    Ok(Outcome::Yield)
}

/// Resume the thread waiting on `request` with `(true, results)` or
/// `(false, error)`; a request of Rook's own ([`send`]) is only decoded.
pub(crate) fn deliver(lua: &Lua, request: u64, answer: WebAnswer) {
    let Some((thread, step)) = with_world(lua, |w| w.web.waiting.remove(&request)) else {
        return;
    };
    let decode = match step {
        Step::Decode(decode) => decode,
        Step::Then(then) => {
            match then(lua, answer) {
                Ok((next, decode)) => {
                    with_world(lua, |w| queue(w, next, thread, Step::Decode(decode)));
                }
                Err(msg) => {
                    if let Some(thread) = thread {
                        crate::runtime::threads::resume(lua, &thread, (false, msg));
                    }
                }
            }
            return;
        }
    };
    let Some(thread) = thread else {
        let _ = decode(lua, answer);
        return;
    };
    let packed = decode(lua, answer).and_then(|values| {
        let pack = || -> mlua::Result<Table> {
            let t = lua.create_table()?;
            let n = values.len();
            for (i, v) in values.into_iter().enumerate() {
                t.raw_set(i + 1, v)?;
            }
            t.raw_set("n", n)?;
            Ok(t)
        };
        pack().map_err(|e| e.to_string())
    });
    match packed {
        Ok(t) => crate::runtime::threads::resume(lua, &thread, (true, t)),
        Err(msg) => crate::runtime::threads::resume(lua, &thread, (false, msg)),
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let helper: Function = lua
        .load(
            r#"
            local wait, error, unpack = ...
            return function(id)
                local ok, results = wait(id)
                if not ok then
                    error(results, 0)
                end
                return unpack(results, 1, results.n)
            end
            "#,
        )
        .set_name("=[rook] web")
        .call((
            api_fn(lua, wait)?,
            lua.globals().get::<Function>("error")?,
            lua.globals()
                .get::<Table>("table")?
                .get::<Function>("unpack")?,
        ))?;
    lua.set_named_registry_value("rook.web_wait", helper)
}
