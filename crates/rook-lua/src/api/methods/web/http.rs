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
use crate::json;
use crate::runtime::ctx::args::{Args, display_value, format_number, number_of};
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("HttpService", "JSONEncode") => super::json::encode,
        ("HttpService", "JSONDecode") => super::json::decode,
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

/// `v` as JSON, as `JSONEncode` writes it. Values JSON has no form for
/// are `null`, or an error when `strict`.
pub(crate) fn to_json(v: &LuaValue, depth: usize, strict: bool) -> ApiResult<serde_json::Value> {
    use serde_json::Value as J;
    if depth > 100 {
        return Err(ScriptError("Can't convert to JSON".into()));
    }
    Ok(match v {
        LuaValue::Boolean(b) => J::Bool(*b),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            json::number(crate::runtime::ctx::args::number_of(v).unwrap_or(0.0))
        }
        LuaValue::String(s) => J::String(s.to_string_lossy()),
        LuaValue::Table(t) => match json::array_length(t) {
            Some(len) => {
                let mut out = Vec::with_capacity(len);
                for i in 1..=len {
                    out.push(to_json(&t.raw_get::<LuaValue>(i)?, depth + 1, strict)?);
                }
                J::Array(out)
            }
            None => {
                let mut out = serde_json::Map::new();
                for pair in t.clone().pairs::<LuaValue, LuaValue>() {
                    let (k, v) = pair?;
                    let key = json::object_key(&k)
                        .ok_or_else(|| ScriptError("Can't convert to JSON".into()))?;
                    out.insert(key, to_json(&v, depth + 1, strict)?);
                }
                J::Object(out)
            }
        },
        LuaValue::Nil => J::Null,
        _ if strict => return Err(ScriptError("Can't convert to JSON".into())),
        _ => J::Null,
    })
}

pub(crate) fn from_json(lua: &Lua, v: &serde_json::Value) -> mlua::Result<LuaValue> {
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

/// A random version 4 UUID in lowercase hex, in braces unless asked
/// otherwise (measured in Studio 0.748, which casts the argument to a
/// boolean: `GenerateGUID("x")` raises `Unable to cast string to bool`).
fn generate_guid(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let braces = a.bool_or(0, true)?;
    let guid = random_guid(lua)?;
    Outcome::ret(lua, if braces { format!("{{{guid}}}") } else { guid })
}

/// A random version 4 UUID in lowercase hex, from the scripts'
/// `math.random`.
pub(crate) fn random_guid(lua: &Lua) -> mlua::Result<String> {
    let math: Table = lua.globals().get("math")?;
    let random: mlua::Function = math.get("random")?;
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = random.call::<u8>((0, 255))?;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// Measured in Studio 0.748: only ASCII letters and digits are kept,
/// `-`, `_`, `.` and `~` escaped too.
fn url_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Outcome::ret(lua, out)
}

/// What a request to Roblox's own web domains raises (measured in Studio
/// 0.748).
const ROBLOX_REFUSED: &str = "HttpService is not allowed to access that Roblox resource";

/// Requests HttpService sends in a minute at most; more fail
/// (Confirmed, creator-docs HttpService:RequestAsync "Limitations").
const REQUESTS_PER_MINUTE: usize = 500;

/// The longest `RequestAsync`'s `Timeout` may be, in seconds (measured in
/// Studio 0.748).
const MAX_TIMEOUT: f64 = 120.0;

/// Headers Roblox sets itself (`User-Agent`, `Roblox-Id`) or works out
/// from the body (`Content-Length`): a script's raises `Header "<name>" is
/// not allowed!`, whatever its case (measured in Studio 0.748).
const LOCKED: [&str; 3] = ["User-Agent", "Roblox-Id", "Content-Length"];

/// [`LOCKED`] for `PostAsync`, whose `content_type` sets `Content-Type`
/// (measured in Studio 0.748).
const POST_LOCKED: [&str; 4] = ["User-Agent", "Roblox-Id", "Content-Length", "Content-Type"];

/// `RequestAsync`'s options; any other key raises `Unrecognized request
/// option <key>` (measured in Studio 0.748).
const OPTIONS: [&str; 6] = ["Url", "Method", "Headers", "Body", "Compress", "Timeout"];

// Measured in Studio 0.748, which checks a request in this order: the
// reflection layer's casts (`Argument 1 missing or nil`, `Unable to cast
// string to bool`), then, on a client, `Http requests can only be executed
// by game server`, then the method's own checks of the URL, options and
// headers, then Roblox's own domains, and only then whether the place lets
// requests out ([`send`]).

/// `GetAsync(url, nocache, headers)`. Approximation: `nocache` changes
/// nothing, as Rook keeps no answers to reuse and every call is sent.
fn get_async(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    called_on_http_service(lua, this, "GetAsync")?;
    present(a, 0)?;
    a.bool_or(1, false)?;
    only_on_servers(lua)?;
    let mut request = WebRequest::get(url_of(a.get(0))?);
    add_headers(lua, &mut request, a.get(2), &LOCKED)?;
    send(lua, request, body_of_success)
}

/// `PostAsync(url, data, content_type, compress, headers)`.
fn post_async(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    called_on_http_service(lua, this, "PostAsync")?;
    present(a, 0)?;
    // Measured: a number or even a table passes for `data`. Approximation:
    // a number is sent as Luau writes it, and anything but a string or a
    // number as no body, as what Roblox sends for one isn't measured.
    let body = match a.get(1) {
        LuaValue::Nil => return Err(a.missing(1)),
        LuaValue::String(s) => s.as_bytes().to_vec(),
        v @ (LuaValue::Integer(_) | LuaValue::Number(_)) => {
            format_number(number_of(v).unwrap_or(0.0)).into_bytes()
        }
        _ => Vec::new(),
    };
    if !a.get(2).is_nil() {
        a.enum_value(lua, 2, "HttpContentType")?;
    }
    let compress = a.bool_or(3, false)?;
    only_on_servers(lua)?;
    let mut request = WebRequest::get(url_of(a.get(0))?);
    request.method = "POST".into();
    request.body = body;
    request.headers.push((
        "Content-Type".into(),
        content_type(lua, a.get(2)).to_owned(),
    ));
    add_headers(lua, &mut request, a.get(4), &POST_LOCKED)?;
    if compress {
        gzip(&mut request)?;
    }
    send(lua, request, body_of_success)
}

/// `RequestAsync({Url, Method, Headers, Body, Compress, Timeout})`.
fn request_async(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    called_on_http_service(lua, this, "RequestAsync")?;
    let options = a.dictionary(0)?;
    only_on_servers(lua)?;
    let request = script_request(lua, &options)?;
    send(lua, request, response_table)
}

/// `RequestAsync`'s request from its options, checked as Studio 0.748
/// checks them (measured: each message, and that an unknown key is found
/// before a missing `Url`).
fn script_request(lua: &Lua, options: &Table) -> ApiResult<WebRequest> {
    for pair in options.pairs::<LuaValue, LuaValue>() {
        let (key, _) = pair?;
        let known = matches!(&key, LuaValue::String(s) if OPTIONS.iter().any(|o| s.as_bytes() == o.as_bytes()));
        if !known {
            return Err(ScriptError(format!(
                "Unrecognized request option {}",
                display_value(lua, &key)
            )));
        }
    }
    let invalid = |what: &str| ScriptError(format!("Invalid request options, {what}"));
    let url = match options.raw_get::<LuaValue>("Url")? {
        LuaValue::Nil => return Err(ScriptError("Invalid request options".into())),
        v => url_of(&v)?,
    };
    // Measured: any token passes, in any case, empty included.
    let method = match options.raw_get::<LuaValue>("Method")? {
        LuaValue::Nil => None,
        LuaValue::String(s) => {
            let method = s.to_string_lossy();
            if !method.chars().all(is_token_char) {
                return Err(ScriptError(format!("Invalid Http Method {method}")));
            }
            Some(method)
        }
        _ => return Err(invalid("Method should be a string")),
    };
    let body = match options.raw_get::<LuaValue>("Body")? {
        LuaValue::Nil => None,
        LuaValue::String(s) => Some(s.as_bytes().to_vec()),
        _ => return Err(invalid("Body should be a string")),
    };
    // Measured: any Body, empty too, with `GET` or `HEAD` as written in
    // capitals (`get` passes). Inference: no Method counts as `GET`.
    if body.is_some() && matches!(method.as_deref().unwrap_or("GET"), "GET" | "HEAD") {
        return Err(invalid("Body should be empty for 'GET' or 'HEAD' method"));
    }
    let mut request = WebRequest::get(url);
    // Approximation: an empty Method is sent as `GET`.
    if let Some(m) = method.filter(|m| !m.is_empty()) {
        request.method = m.to_ascii_uppercase();
    }
    if let Some(b) = body {
        request.body = b;
    }
    match options.raw_get::<LuaValue>("Headers")? {
        LuaValue::Nil => {}
        LuaValue::Table(t) if t.raw_get::<LuaValue>(1)?.is_nil() => {
            request.headers = header_list(lua, &t, &LOCKED)?;
        }
        _ => return Err(invalid("Headers should be a table")),
    }
    // Measured: any value passes for Compress.
    let compress: LuaValue = options.raw_get("Compress")?;
    if crate::types::enums::item_name(lua, &compress, "HttpCompression") == Some("Gzip") {
        gzip(&mut request)?;
    }
    match options.raw_get::<LuaValue>("Timeout")? {
        LuaValue::Nil => {}
        v @ (LuaValue::Integer(_) | LuaValue::Number(_)) => {
            let seconds = number_of(&v).unwrap_or(0.0);
            if !(seconds > 0.0 && seconds <= MAX_TIMEOUT) {
                return Err(ScriptError(
                    "Timeout value must be greater than 0 and no greater than 120 seconds.".into(),
                ));
            }
            request.timeout = Some(Duration::from_secs_f64(seconds));
        }
        _ => return Err(ScriptError("Timeout value must be an integer.".into())),
    }
    Ok(request)
}

/// Raises as a call with `.` does when the method is called on another
/// instance (measured: `HttpService.RequestAsync(workspace, options)`).
fn called_on_http_service(lua: &Lua, this: InstanceId, name: &str) -> ApiResult<()> {
    if with_world(lua, |w| w.dom.is_a(this, "HttpService")) {
        return Ok(());
    }
    Err(ScriptError(format!(
        "Expected ':' not '.' calling member function {name}"
    )))
}

/// Raises `Argument <n> missing or nil` for a nil argument the method
/// needs.
fn present(a: &Args, i: usize) -> ApiResult<()> {
    if a.get(i).is_nil() {
        return Err(a.missing(i));
    }
    Ok(())
}

/// A request's URL, as Studio 0.748 checks it (measured): a string (or a
/// Secret, which Rook doesn't have), not empty, starting `http://` or
/// `https://` in any case. Anything past that is the network's to parse.
fn url_of(v: &LuaValue) -> ApiResult<String> {
    let LuaValue::String(s) = v else {
        return Err(ScriptError(
            "Invalid URL type, URL should be a string or secret.".into(),
        ));
    };
    let url = s.to_string_lossy();
    if url.is_empty() {
        return Err(ScriptError("Empty URL".into()));
    }
    let scheme = url.get(..8).unwrap_or(&url).to_ascii_lowercase();
    if !(scheme.starts_with("http://") || scheme.starts_with("https://")) {
        return Err(ScriptError("URL must be http".into()));
    }
    Ok(url)
}

/// Raises on a client: only servers send requests (measured in Studio
/// 0.748).
fn only_on_servers(lua: &Lua) -> ApiResult<()> {
    with_world(lua, |w| {
        if w.is_client {
            return Err(ScriptError(
                "Http requests can only be executed by game server".into(),
            ));
        }
        Ok(())
    })
}

/// Raises unless the place enables HTTP. `HttpEnabled` is saved in the
/// place and game scripts can't write it, so it's read where it's stored.
/// Inference: the message, as players report it. Studio 0.748 checks
/// every argument first, so a bad one raises its own error even with HTTP
/// off (measured, through Studio's MCP, whose own HTTP permission stands
/// at this point).
fn only_if_enabled(lua: &Lua) -> ApiResult<()> {
    with_world(lua, |w| {
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

/// A character HTTP allows in a header's name or a method (RFC 9110's
/// `tchar`). Measured: a space raises; Inference: the rest of the set.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// Adds a script's headers table (`GetAsync` and `PostAsync`'s last
/// argument) to `request`; a script's header replaces one Rook set.
/// Measured: anything but a table without a first item raises `headers
/// value must be a dictionary!`.
fn add_headers(
    lua: &Lua,
    request: &mut WebRequest,
    headers: &LuaValue,
    locked: &[&str],
) -> ApiResult<()> {
    let headers = match headers {
        LuaValue::Nil => return Ok(()),
        LuaValue::Table(t) if t.raw_get::<LuaValue>(1)?.is_nil() => header_list(lua, t, locked)?,
        _ => {
            return Err(ScriptError("headers value must be a dictionary!".into()));
        }
    };
    for (name, value) in headers {
        request
            .headers
            .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
        request.headers.push((name, value));
    }
    Ok(())
}

/// A script's headers, each checked as Studio 0.748 checks them
/// (measured: the messages, a value that isn't a string, a locked name, a
/// space in a name and a line break in a value; an empty name passes).
/// Inference: a NUL in a value raises as a line break does, and characters
/// outside HTTP's token set in a name as a space does.
fn header_list(lua: &Lua, headers: &Table, locked: &[&str]) -> ApiResult<Vec<(String, String)>> {
    let mut out = Vec::new();
    for pair in headers.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair?;
        let name = match &key {
            LuaValue::String(s) => s.to_string_lossy(),
            k => display_value(lua, k),
        };
        let LuaValue::String(value) = value else {
            return Err(ScriptError(format!(
                "Header {name} must have its value be a string or secret!"
            )));
        };
        let value = value.to_string_lossy();
        if locked.iter().any(|l| name.eq_ignore_ascii_case(l)) {
            return Err(ScriptError(format!("Header \"{name}\" is not allowed!")));
        }
        if let Some(c) = name.chars().find(|&c| !is_token_char(c)) {
            return Err(ScriptError(format!(
                "Header \"{name}\" has unallowed character \"{c}\""
            )));
        }
        if let Some(c) = value.chars().find(|c| matches!(c, '\r' | '\n' | '\0')) {
            return Err(ScriptError(format!(
                "Header {name} has unallowed character {c} in value {value}"
            )));
        }
        out.push((name, value));
    }
    Ok(out)
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

/// Sends a script's request once its arguments have passed, and returns
/// what `answer` makes of the response; the calling thread waits
/// meanwhile. Refused, in this order: a URL on Roblox's own web domains
/// (measured: `apis.roblox.com` without an `x-api-key` header raises that
/// it needs one, any other `HttpService is not allowed to access that
/// Roblox resource`; Rook has no Secrets, so `apis` with a key raises the
/// latter), a place that doesn't enable HTTP, and requests past the limit
/// per minute. The network checks each hop's host and addresses again
/// ([`Reach::Script`]).
fn send(
    lua: &Lua,
    mut request: WebRequest,
    answer: fn(&Lua, &Response) -> Result<MultiValue, String>,
) -> ApiResult<Outcome> {
    if let Some((domain, _)) = rook_platform::endpoints::roblox_domain(&request.url) {
        let keyed = request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("x-api-key"));
        if domain == "apis" && !keyed {
            return Err(ScriptError(
                "Header \"x-api-key\" must be provided for Roblox resources!".into(),
            ));
        }
        return Err(ScriptError(ROBLOX_REFUSED.into()));
    }
    only_if_enabled(lua)?;
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
