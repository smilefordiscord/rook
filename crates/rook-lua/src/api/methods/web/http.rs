//! HttpService: JSON, GUIDs and URL encoding, and scripts' own requests
//! (`GetAsync`, `PostAsync`, `RequestAsync`), which a server sends to any
//! site on the public internet when its place enables HTTP; the rules are in
//! `docs/behavior/scripting/http-requests.md`. CoreScripts' requests to
//! Roblox's web domains (`HttpService:RequestInternal`,
//! HttpRbxApiService) go out only where the domain has an endpoint
//! ([`super::requests`]), and fail as offline otherwise, which the
//! CoreScripts handle.

use std::io::Write;
use std::time::Duration;

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;
use rook_platform::network::{NetworkError, Reach, Response};

use super::RawMethod;
use super::requests::{self as web, WebAnswer, WebRequest};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("HttpService", "JSONEncode") => json_encode,
        ("HttpService", "JSONDecode") => json_decode,
        ("HttpService", "GenerateGUID") => generate_guid,
        ("HttpService", "UrlEncode") => url_encode,
        ("HttpService", "GetAsync") => get_async,
        ("HttpService", "PostAsync") => post_async,
        ("HttpService", "RequestAsync") => request_async,
        ("HttpRbxApiService", "GetAsync") => |l, _, a| {
            let url = api_url(l, &a.string(0)?);
            rbx_api(l, "GET", url, None)
        },
        ("HttpRbxApiService", "GetAsyncFullUrl") => |l, _, a| {
            let url = web::rewrite(l, &a.string(0)?);
            rbx_api(l, "GET", url, None)
        },
        ("HttpRbxApiService", "PostAsync") => |l, _, a| {
            let url = api_url(l, &a.string(0)?);
            rbx_api(
                l,
                "POST",
                url,
                Some((a.string(1)?, content_type(l, a.get(3)))),
            )
        },
        ("HttpRbxApiService", "PostAsyncFullUrl") => |l, _, a| {
            let url = web::rewrite(l, &a.string(0)?);
            rbx_api(
                l,
                "POST",
                url,
                Some((a.string(1)?, content_type(l, a.get(3)))),
            )
        },
        ("HttpRbxApiService", "RequestAsync") => rbx_request,
        _ => return None,
    })
}

/// `RequestInternal(options)` gives a request whose `Start(callback)`
/// calls back with `(sent, response)` from another thread.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("HttpService", "RequestInternal") => {
            r#"local std = ...
            local task, send = std.task, std.web_request
            return function(_, options)
                local request = {}
                function request:Start(callback)
                    task.spawn(function()
                        callback(send(options))
                    end)
                end
                function request:Cancel() end
                return request
            end"#
        }
        _ => return None,
    })
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let std: Table = lua.named_registry_value("rook.std")?;
    std.set("web_request", api_fn(lua, request_internal)?)
}

/// A path on the legacy `api` domain, which HttpRbxApiService's
/// relative paths (`assets/1/versions`) are on.
fn api_url(lua: &Lua, path: &str) -> Option<String> {
    web::url(lua, "api", &format!("/{}", path.trim_start_matches('/')))
}

/// An `Enum.HttpContentType` as a MIME type; JSON by default.
fn content_type(lua: &Lua, v: &LuaValue) -> &'static str {
    match crate::types::enums::item_name(lua, v, "HttpContentType") {
        Some("ApplicationUrlEncoded") => "application/x-www-form-urlencoded",
        Some("ApplicationXml") => "application/xml",
        Some("TextPlain") => "text/plain",
        Some("TextXml") => "text/xml",
        _ => "application/json",
    }
}

/// HttpRbxApiService's answer: the body of a 2xx response, raised
/// otherwise.
fn rbx_api(
    lua: &Lua,
    method: &str,
    url: Option<String>,
    body: Option<(String, &str)>,
) -> ApiResult<Outcome> {
    let Some(url) = url else {
        return Err(ScriptError("HttpError: ConnectFail".into()));
    };
    let mut request = WebRequest::get(url);
    request.method = method.to_owned();
    if let Some((body, kind)) = body {
        request
            .headers
            .push(("Content-Type".into(), kind.to_owned()));
        request.body = body.into_bytes();
    }
    web::call(
        lua,
        request,
        Box::new(|lua, answer| {
            let response = answer.map_err(|e| format!("HttpError: ConnectFail ({e})"))?;
            if !(200..300).contains(&response.status) {
                return Err(format!("HTTP {}", response.status));
            }
            let body = lua
                .create_string(&response.body)
                .map_err(|e| e.to_string())?;
            body.into_lua_multi(lua).map_err(|e| e.to_string())
        }),
    )
}

/// `RequestAsync({Url, Method, Body, Headers})`: the body, as `rbx_api`.
fn rbx_request(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(options) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let request = request_of(options)?;
    let url = web::rewrite(lua, &request.url);
    let body = (!request.body.is_empty()).then(|| {
        let kind = request
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            .map_or("application/json", |(_, v)| v.as_str());
        (String::from_utf8_lossy(&request.body).into_owned(), kind)
    });
    rbx_api(lua, &request.method, url, body)
}

/// A request from `{Url, Method, Body, Headers}` (Method defaults to GET).
fn request_of(options: &Table) -> ApiResult<WebRequest> {
    let url: String = options.get("Url")?;
    let method: Option<String> = options.get("Method")?;
    let body: Option<mlua::LuaString> = options.get("Body")?;
    let mut request = WebRequest::get(url);
    if let Some(m) = method {
        request.method = m.to_ascii_uppercase();
    }
    if let Some(b) = body {
        request.body = b.as_bytes().to_vec();
    }
    if let Some(headers) = options.get::<Option<Table>>("Headers")? {
        for pair in headers.pairs::<String, String>() {
            request.headers.push(pair?);
        }
    }
    Ok(request)
}

/// `send(options)` for RequestInternal: `(sent, response)`, never raising.
fn request_internal(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "RequestInternal");
    let LuaValue::Table(options) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let mut request = request_of(options)?;
    let Some(url) = web::rewrite(lua, &request.url) else {
        return Ok(Outcome::Return(response(
            lua,
            Err(NetworkError::ConnectFail(
                "no endpoint for its domain".into(),
            )),
        )?));
    };
    request.url = url;
    web::call(
        lua,
        request,
        Box::new(|lua, answer| response(lua, answer).map_err(|e| e.to_string())),
    )
}

/// `(true, {StatusCode, Body, Headers, HttpError = "OK"})` for an answer,
/// `(false, {StatusCode = 0, Body = "", HttpError = "ConnectFail"})` for
/// none.
fn response(lua: &Lua, answer: WebAnswer) -> mlua::Result<MultiValue> {
    let t = lua.create_table()?;
    let Ok(r) = answer else {
        t.set("StatusCode", 0)?;
        t.set("Body", "")?;
        t.set("HttpError", "ConnectFail")?;
        return (false, t).into_lua_multi(lua);
    };
    t.set("StatusCode", r.status)?;
    t.set("Body", lua.create_string(&r.body)?)?;
    let headers = lua.create_table()?;
    for (name, value) in &r.headers {
        headers.set(name.as_str(), value.as_str())?;
    }
    t.set("Headers", headers)?;
    t.set("HttpError", "OK")?;
    (true, t).into_lua_multi(lua)
}

fn to_json(v: &LuaValue, depth: usize) -> ApiResult<serde_json::Value> {
    use serde_json::Value as J;
    if depth > 100 {
        return Err(ScriptError("Can't convert to JSON".into()));
    }
    Ok(match v {
        LuaValue::Boolean(b) => J::Bool(*b),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let n = crate::runtime::ctx::args::number_of(v).unwrap_or(0.0);
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                J::Number((n as i64).into())
            } else {
                serde_json::Number::from_f64(n).map_or(J::Null, J::Number)
            }
        }
        LuaValue::String(s) => J::String(s.to_string_lossy()),
        LuaValue::Table(t) => {
            // Measured in Studio 0.748: a table with a length (`#t`) is an
            // array of 1 to that length, nil as null and other keys
            // dropped; an empty one is `[]`; the rest are objects.
            let len = t.raw_len();
            if len > 0 || t.is_empty() {
                let mut out = Vec::with_capacity(len);
                for i in 1..=len {
                    out.push(to_json(&t.raw_get::<LuaValue>(i)?, depth + 1)?);
                }
                J::Array(out)
            } else {
                let mut out = serde_json::Map::new();
                for pair in t.clone().pairs::<LuaValue, LuaValue>() {
                    let (k, v) = pair?;
                    let key = match &k {
                        LuaValue::String(s) => s.to_string_lossy(),
                        LuaValue::Integer(_) | LuaValue::Number(_) => {
                            crate::runtime::ctx::args::format_number(
                                crate::runtime::ctx::args::number_of(&k).unwrap_or(0.0),
                            )
                        }
                        _ => return Err(ScriptError("Can't convert to JSON".into())),
                    };
                    out.insert(key, to_json(&v, depth + 1)?);
                }
                J::Object(out)
            }
        }
        _ => J::Null,
    })
}

fn from_json(lua: &Lua, v: &serde_json::Value) -> mlua::Result<LuaValue> {
    use serde_json::Value as J;
    Ok(match v {
        J::Null => LuaValue::Nil,
        J::Bool(b) => LuaValue::Boolean(*b),
        J::Number(n) => LuaValue::Number(n.as_f64().unwrap_or(0.0)),
        J::String(s) => LuaValue::String(lua.create_string(s)?),
        J::Array(a) => {
            let t = lua.create_table_with_capacity(a.len(), 0)?;
            for (i, x) in a.iter().enumerate() {
                t.raw_set(i + 1, from_json(lua, x)?)?;
            }
            LuaValue::Table(t)
        }
        J::Object(o) => {
            let t = lua.create_table_with_capacity(0, o.len())?;
            for (k, x) in o {
                t.raw_set(k.as_str(), from_json(lua, x)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

fn json_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let json = to_json(a.get(0), 0)?;
    Outcome::ret(lua, json.to_string())
}

fn json_decode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| ScriptError("Can't parse JSON".into()))?;
    Outcome::ret(lua, from_json(lua, &json)?)
}

/// A random version 4 UUID, uppercase, in braces unless asked otherwise
/// (creator-docs HttpService:GenerateGUID).
fn generate_guid(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let braces = !matches!(a.get(0), LuaValue::Boolean(false));
    let math: Table = lua.globals().get("math")?;
    let random: mlua::Function = math.get("random")?;
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = random.call::<u8>((0, 255))?;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let guid = format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    );
    Outcome::ret(lua, if braces { format!("{{{guid}}}") } else { guid })
}

fn url_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Outcome::ret(lua, out)
}

/// What a request to Roblox's own web domains raises.
const ROBLOX_REFUSED: &str = "HttpService is not allowed to access ROBLOX resources";

/// Requests HttpService sends in a minute at most; more fail
/// (Confirmed, creator-docs HttpService:RequestAsync "Limitations").
const REQUESTS_PER_MINUTE: usize = 500;

/// How long a request may take; `RequestAsync`'s `Timeout` may only
/// shorten it. Approximation: the network's own limit, as Roblox's default
/// isn't documented.
const MAX_TIMEOUT: f64 = rook_platform::network::TIMEOUT.as_secs_f64();

/// `GetAsync(url, nocache, headers)`. Approximation: `nocache` changes
/// nothing, as Rook keeps no answers to reuse and every call is sent.
fn get_async(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    refuse_unless_allowed(lua)?;
    let mut request = WebRequest::get(a.string(0)?);
    add_headers(&mut request, a.get(2))?;
    send(lua, request, body_of_success)
}

/// `PostAsync(url, data, content_type, compress, headers)`.
fn post_async(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    refuse_unless_allowed(lua)?;
    let mut request = WebRequest::get(a.string(0)?);
    request.method = "POST".into();
    request.body = a.string(1)?.into_bytes();
    request.headers.push((
        "Content-Type".into(),
        content_type(lua, a.get(2)).to_owned(),
    ));
    add_headers(&mut request, a.get(4))?;
    if a.truthy(3) {
        gzip(&mut request)?;
    }
    send(lua, request, body_of_success)
}

/// `RequestAsync({Url, Method, Headers, Body, Compress, Timeout})`.
fn request_async(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    refuse_unless_allowed(lua)?;
    let LuaValue::Table(options) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let mut request = request_of(options)?;
    request.headers.retain(|(name, _)| !is_locked(name));
    // Confirmed (creator-docs): GET and HEAD take no Body. Inference: the
    // request is refused rather than sent without it.
    if !request.body.is_empty() && matches!(request.method.as_str(), "GET" | "HEAD") {
        return Err(ScriptError(format!(
            "{} requests can't have a Body",
            request.method
        )));
    }
    let compress: LuaValue = options.get("Compress")?;
    if crate::types::enums::item_name(lua, &compress, "HttpCompression") == Some("Gzip") {
        gzip(&mut request)?;
    }
    if let Some(seconds) = options.get::<Option<f64>>("Timeout")? {
        if !(seconds > 0.0 && seconds <= MAX_TIMEOUT) {
            return Err(ScriptError(format!(
                "Timeout must be greater than 0 and at most {MAX_TIMEOUT}"
            )));
        }
        request.timeout = Some(Duration::from_secs_f64(seconds));
    }
    send(lua, request, response_table)
}

/// Raises why a script's request can't be sent: only servers send them,
/// and only when the place enables HTTP. `HttpEnabled` is saved in the
/// place and game scripts can't write it, so it's read where it's stored.
/// Inference: the messages are Roblox's as players report them.
fn refuse_unless_allowed(lua: &Lua) -> ApiResult<()> {
    with_world(lua, |w| {
        if w.is_client {
            return Err(ScriptError(
                "Http requests can only be executed by game server".into(),
            ));
        }
        let enabled = w
            .dom
            .find_service("HttpService")
            .and_then(|s| w.dom.bool_property(s, "HttpEnabled"));
        if enabled == Some(true) {
            Ok(())
        } else {
            Err(ScriptError(
                "Http requests are not enabled. Enable via game settings".into(),
            ))
        }
    })
}

/// Headers Roblox sets itself (`User-Agent`, `Roblox-Id`) or works out
/// from the body (`Content-Length`), which a script's are dropped for
/// (Confirmed that they're locked, creator-docs HttpService:RequestAsync;
/// Inference that setting one is ignored rather than raised).
fn is_locked(name: &str) -> bool {
    ["User-Agent", "Roblox-Id", "Content-Length"]
        .iter()
        .any(|locked| name.eq_ignore_ascii_case(locked))
}

/// Adds a script's headers table (`GetAsync` and `PostAsync`'s last
/// argument) to `request`; a script's header replaces one Rook set.
fn add_headers(request: &mut WebRequest, headers: &LuaValue) -> ApiResult<()> {
    let LuaValue::Table(t) = headers else {
        return Ok(());
    };
    for pair in t.clone().pairs::<String, String>() {
        let (name, value) = pair?;
        if is_locked(&name) {
            continue;
        }
        request
            .headers
            .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
        request.headers.push((name, value));
    }
    Ok(())
}

/// Gzips the body and says so in `Content-Encoding` (creator-docs
/// HttpService:PostAsync, Enum.HttpCompression).
fn gzip(request: &mut WebRequest) -> ApiResult<()> {
    let failed = |e: std::io::Error| ScriptError(format!("couldn't compress the body: {e}"));
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&request.body).map_err(failed)?;
    request.body = encoder.finish().map_err(failed)?;
    request
        .headers
        .retain(|(name, _)| !name.eq_ignore_ascii_case("Content-Encoding"));
    request
        .headers
        .push(("Content-Encoding".into(), "gzip".into()));
    Ok(())
}

/// Sends a script's request and returns what `answer` makes of the
/// response; the calling thread waits meanwhile. Refused, as Roblox
/// refuses them: a URL that isn't `http` or `https` (creator-docs
/// RequestAsync), one on Roblox's own web domains (Inference: the message
/// players report), and requests past the limit per minute. The network
/// checks each hop's host and addresses again ([`Reach::Script`]).
fn send(
    lua: &Lua,
    mut request: WebRequest,
    answer: fn(&Lua, &Response) -> Result<MultiValue, String>,
) -> ApiResult<Outcome> {
    let scheme = request.url.split_once("://").map(|(scheme, _)| scheme);
    if !scheme.is_some_and(|s| s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https")) {
        return Err(ScriptError("HttpError: InvalidUrl".into()));
    }
    if rook_platform::endpoints::roblox_domain(&request.url).is_some() {
        return Err(ScriptError(ROBLOX_REFUSED.into()));
    }
    with_world(lua, |w| {
        let now = w.scheduler.now();
        let sent = &mut w.web.script_sent;
        while sent.front().is_some_and(|&at| at <= now - 60.0) {
            sent.pop_front();
        }
        if sent.len() >= REQUESTS_PER_MINUTE {
            // Inference: the message Roblox gives for its Open Cloud
            // limit (creator-docs "In-game HTTP requests").
            return Err(ScriptError("Number of requests exceeded limit".into()));
        }
        sent.push_back(now);
        request.reach = Reach::Script {
            local: w.web.local_addresses,
        };
        Ok(())
    })?;
    web::call(
        lua,
        request,
        Box::new(move |lua, response| answer(lua, &response.map_err(|e| http_error(&e))?)),
    )
}

/// What a request that got no answer raises: `HttpError: ` and the
/// `Enum.HttpError` item for why (Inference from the creator docs'
/// descriptions of the enum's items).
fn http_error(e: &NetworkError) -> String {
    let kind = match e {
        NetworkError::InvalidUrl(_) => "InvalidUrl",
        // Not one the docs describe: "does not map to any other value".
        NetworkError::InvalidRequest(_) => "Unknown",
        NetworkError::DnsResolve(_) => "DnsResolve",
        // Inference: Roblox's game servers have no route to a local
        // address, so connecting to one fails.
        NetworkError::ConnectFail(_) | NetworkError::NotPublic(_) => "ConnectFail",
        NetworkError::TimedOut(_) => "TimedOut",
        NetworkError::TooManyRedirects => "TooManyRedirects",
        NetworkError::InvalidRedirect(_) => "InvalidRedirect",
        NetworkError::Tls(_) => "SslConnectFail",
        NetworkError::ConnectionClosed(_) => "ConnectionClosed",
        // Running out of memory on the answer.
        NetworkError::TooLarge(_) => "OutOfMemory",
        // A redirect there; Inference: refused as the first URL is.
        NetworkError::RobloxDomain(_) => return ROBLOX_REFUSED.to_owned(),
        NetworkError::Other(_) => "NetFail",
    };
    format!("HttpError: {kind}")
}

/// `GetAsync` and `PostAsync`'s answer: the body of a 2xx response, and
/// `HTTP <code> (<reason>)` raised for any other, `HTTP <code>` for a code
/// with no standard reason (Inference: the message players report).
fn body_of_success(lua: &Lua, response: &Response) -> Result<MultiValue, String> {
    if !(200..300).contains(&response.status) {
        return Err(if response.reason.is_empty() {
            format!("HTTP {}", response.status)
        } else {
            format!("HTTP {} ({})", response.status, response.reason)
        });
    }
    let body = lua
        .create_string(&response.body)
        .map_err(|e| e.to_string())?;
    body.into_lua_multi(lua).map_err(|e| e.to_string())
}

/// `RequestAsync`'s answer whatever the status: `{Success, StatusCode,
/// StatusMessage, Headers, Body}`, `Success` when the status is 2xx
/// (Confirmed, creator-docs HttpService:RequestAsync). Inference: a header
/// sent more than once is its values joined with `, `, as HTTP allows.
fn response_table(lua: &Lua, response: &Response) -> Result<MultiValue, String> {
    let table = || -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("Success", (200..300).contains(&response.status))?;
        t.set("StatusCode", response.status)?;
        t.set("StatusMessage", response.reason.as_str())?;
        let headers = lua.create_table()?;
        for (name, value) in &response.headers {
            let joined = match headers.get::<Option<String>>(name.as_str())? {
                Some(before) => format!("{before}, {value}"),
                None => value.clone(),
            };
            headers.set(name.as_str(), joined)?;
        }
        t.set("Headers", headers)?;
        t.set("Body", lua.create_string(&response.body)?)?;
        Ok(t)
    };
    let t = table().map_err(|e| e.to_string())?;
    t.into_lua_multi(lua).map_err(|e| e.to_string())
}
