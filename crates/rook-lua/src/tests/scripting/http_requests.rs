use std::io::Read;

use crate::tests::*;
use rook_platform::network::{NetworkError, Reach, Response};

/// A server runtime whose place has HTTP enabled, running `source`.
fn enabled(source: &str) -> Runtime {
    enabled_with(false, source)
}

/// [`enabled`], on a host that lets requests reach local addresses with
/// `local`.
fn enabled_with(local: bool, source: &str) -> Runtime {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let http = w.dom.get_service("HttpService").unwrap();
        w.dom
            .set_property_by_name(http, "HttpEnabled", rook_types::Value::Bool(true))
            .unwrap();
    }
    rt.set_http_local_addresses(local);
    rt.run_source("Workspace.Script", source, None).unwrap();
    rt
}

fn answer(status: u16, reason: &str, headers: &[(&str, &str)], body: &str) -> Response {
    Response {
        status,
        reason: reason.into(),
        headers: headers
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn requests_are_refused_unless_the_place_enables_http() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http, "https://example.com"))
        print(pcall(http.PostAsync, http, "https://example.com", "{}"))
        print(pcall(http.RequestAsync, http, { Url = "https://example.com" }))
        "#,
        0,
        &[
            "false Http requests are not enabled. Enable via game settings",
            "false Http requests are not enabled. Enable via game settings",
            "false Http requests are not enabled. Enable via game settings",
        ],
    );
}

/// A client never sends them, even in a place that enables HTTP.
#[test]
fn clients_never_send_requests() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let http = w.dom.get_service("HttpService").unwrap();
        w.dom
            .set_property_by_name(http, "HttpEnabled", rook_types::Value::Bool(true))
            .unwrap();
        w.is_client = true;
    }
    rt.run_source(
        "Workspace.LocalScript",
        r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http, "https://example.com"))
        "#,
        None,
    )
    .unwrap();
    assert_eq!(
        finish(&rt, 0),
        ["false Http requests can only be executed by game server"]
    );
    assert!(rt.take_web_requests().is_empty());
}

#[test]
fn get_and_post_send_their_arguments_and_return_the_body() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        task.spawn(function()
            print(http:GetAsync("https://example.com/a?b=1", true, { ["X-Key"] = "k" }))
        end)
        task.spawn(function()
            print(http:PostAsync("https://example.com/c", "x=1", Enum.HttpContentType.ApplicationUrlEncoded))
        end)
        task.spawn(function()
            print(http:PostAsync("http://example.com/d", "{}"))
        end)
        "#,
    );
    let sent = rt.take_web_requests();
    let (get, form, json) = (&sent[0].1, &sent[1].1, &sent[2].1);
    assert_eq!(
        (get.method.as_str(), get.url.as_str()),
        ("GET", "https://example.com/a?b=1")
    );
    assert_eq!(get.headers, [("X-Key".to_owned(), "k".to_owned())]);
    assert_eq!(get.reach, Reach::Script { local: false });
    assert_eq!(
        (form.method.as_str(), form.body.as_slice()),
        ("POST", b"x=1".as_slice())
    );
    assert_eq!(
        form.headers,
        [(
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned()
        )]
    );
    assert_eq!(
        json.headers,
        [("Content-Type".to_owned(), "application/json".to_owned())]
    );
    for (i, body) in ["got", "posted", "{}"].into_iter().enumerate() {
        rt.deliver_web(sent[i].0, Ok(answer(200, "OK", &[], body)));
    }
    assert_eq!(finish(&rt, 1), ["got", "posted", "{}"]);
}

/// `GetAsync` and `PostAsync` return only a 2xx answer's body; any other
/// status, or no answer at all, raises.
#[test]
fn get_and_post_raise_for_failures() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        task.spawn(function()
            print(pcall(http.GetAsync, http, "https://example.com/gone"))
        end)
        task.spawn(function()
            print(pcall(http.PostAsync, http, "https://example.com/down", "{}"))
        end)
        task.spawn(function()
            print(pcall(http.GetAsync, http, "https://example.com/slow"))
        end)
        task.spawn(function()
            print(pcall(http.GetAsync, http, "https://nowhere.example"))
        end)
        "#,
    );
    let sent = rt.take_web_requests();
    rt.deliver_web(sent[0].0, Ok(answer(404, "Not Found", &[], "gone")));
    rt.deliver_web(sent[1].0, Ok(answer(503, "Service Unavailable", &[], "")));
    rt.deliver_web(sent[2].0, Err(NetworkError::TimedOut("global".into())));
    rt.deliver_web(sent[3].0, Err(NetworkError::DnsResolve("host".into())));
    assert_eq!(
        finish(&rt, 1),
        [
            "false HTTP 404 (Not Found)",
            "false HTTP 503 (Service Unavailable)",
            "false HttpError: TimedOut",
            "false HttpError: DnsResolve",
        ]
    );
}

/// Why the network refused or gave up, as each raises.
#[test]
fn refusals_and_limits_raise_their_errors() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        for i = 1, 4 do
            task.spawn(function()
                print(pcall(http.GetAsync, http, "https://example.com/" .. i))
            end)
        end
        "#,
    );
    let sent = rt.take_web_requests();
    rt.deliver_web(sent[0].0, Err(NetworkError::NotPublic("10.0.0.1".into())));
    rt.deliver_web(
        sent[1].0,
        Err(NetworkError::RobloxDomain("www.roblox.com".into())),
    );
    rt.deliver_web(sent[2].0, Err(NetworkError::TooLarge(16 << 20)));
    rt.deliver_web(
        sent[3].0,
        Err(NetworkError::InvalidRequest("header".into())),
    );
    assert_eq!(
        finish(&rt, 1),
        [
            "false HttpError: ConnectFail",
            "false HttpService is not allowed to access that Roblox resource",
            "false HttpError: OutOfMemory",
            "false HttpError: Unknown",
        ]
    );
}

/// The host's setting lets requests reach local addresses.
#[test]
fn the_host_can_let_requests_reach_local_addresses() {
    let rt = enabled_with(
        true,
        r#"task.spawn(game.HttpService.GetAsync, game.HttpService, "http://127.0.0.1/")"#,
    );
    let sent = rt.take_web_requests();
    assert_eq!(sent[0].1.reach, Reach::Script { local: true });
}

/// `RequestAsync` answers whatever the status, with Roblox's fields.
#[test]
fn request_async_answers_every_status() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        task.spawn(function()
            local r = http:RequestAsync({
                Url = "https://example.com/a",
                Method = "patch",
                Body = "{}",
                Headers = { ["Content-Type"] = "application/json" },
            })
            print(r.Success, r.StatusCode, r.StatusMessage, r.Body, r.Headers["content-type"], r.Headers["set-cookie"])
        end)
        task.spawn(function()
            local r = http:RequestAsync({ Url = "https://example.com/b" })
            print(r.Success, r.StatusCode, r.StatusMessage, r.Body)
        end)
        task.spawn(function()
            print(pcall(http.RequestAsync, http, { Url = "https://example.com/c" }))
        end)
        "#,
    );
    let sent = rt.take_web_requests();
    let patch = &sent[0].1;
    assert_eq!(
        (patch.method.as_str(), patch.body.as_slice()),
        ("PATCH", b"{}".as_slice())
    );
    assert_eq!(sent[1].1.method, "GET");
    let headers = [
        ("content-type", "text/plain"),
        ("set-cookie", "a=1"),
        ("set-cookie", "b=2"),
    ];
    rt.deliver_web(
        sent[0].0,
        Ok(answer(418, "I'm a teapot", &headers, "short")),
    );
    rt.deliver_web(sent[1].0, Ok(answer(204, "No Content", &[], "")));
    rt.deliver_web(sent[2].0, Err(NetworkError::ConnectFail("refused".into())));
    assert_eq!(
        finish(&rt, 1),
        [
            "false 418 I'm a teapot short text/plain a=1, b=2",
            "true 204 No Content ",
            "false HttpError: ConnectFail",
        ]
    );
}

/// `Compress` gzips the body and `Timeout` sets the wait.
#[test]
fn request_options_shape_what_is_sent() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        local accept = { Accept = "text/plain" }
        task.spawn(http.RequestAsync, http, {
            Url = "https://example.com/a",
            Method = "POST",
            Body = "hello hello hello",
            Compress = Enum.HttpCompression.Gzip,
            Timeout = 5,
            Headers = accept,
        })
        task.spawn(http.PostAsync, http, "https://example.com/b", "hi", Enum.HttpContentType.TextPlain, true, accept)
        task.spawn(http.RequestAsync, http, { Url = "https://example.com/c", Method = "POST", Body = "plain", Compress = Enum.HttpCompression.None })
        "#,
    );
    let sent = rt.take_web_requests();
    let unzip = |body: &[u8]| {
        let mut out = String::new();
        flate2::read::GzDecoder::new(body)
            .read_to_string(&mut out)
            .unwrap();
        out
    };
    let (a, b, c) = (&sent[0].1, &sent[1].1, &sent[2].1);
    assert_eq!(unzip(&a.body), "hello hello hello");
    assert_eq!(
        a.headers,
        [
            ("Accept".to_owned(), "text/plain".to_owned()),
            ("Content-Encoding".to_owned(), "gzip".to_owned()),
        ]
    );
    assert_eq!(a.timeout, Some(std::time::Duration::from_secs(5)));
    assert_eq!(unzip(&b.body), "hi");
    assert_eq!(
        b.headers,
        [
            ("Content-Type".to_owned(), "text/plain".to_owned()),
            ("Accept".to_owned(), "text/plain".to_owned()),
            ("Content-Encoding".to_owned(), "gzip".to_owned()),
        ]
    );
    assert_eq!(
        (c.body.as_slice(), c.headers.len(), c.timeout),
        (b"plain".as_slice(), 0, None)
    );
}

/// What's refused before anything is sent, with Studio 0.748's messages.
#[test]
fn bad_requests_raise_without_sending() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        local url = "https://example.com"
        print(pcall(http.GetAsync, http, "ftp://example.com/a"))
        print(pcall(http.GetAsync, http, "example.com/a"))
        print(pcall(http.GetAsync, http, 5))
        print(pcall(http.GetAsync, http, ""))
        print(pcall(http.GetAsync, http, "https://users.roblox.com/v1/users/1"))
        print(pcall(http.GetAsync, http, "https://www.ROBLOX.com/"))
        print(pcall(http.GetAsync, http, "https://apis.roblox.com/"))
        print(pcall(http.GetAsync, http, url, "yes"))
        print(pcall(http.GetAsync, http, url, false, { "a" }))
        print(pcall(http.GetAsync, http, url, false, { ["user-agent"] = "me" }))
        print(pcall(http.GetAsync, http, url, false, { X = 5 }))
        print(pcall(http.PostAsync, http, url))
        print(pcall(http.PostAsync, http, url, "x", "json"))
        print(pcall(http.PostAsync, http, url, "x", nil, false, { ["Content-Type"] = "text/csv" }))
        print(pcall(http.RequestAsync, http, "x"))
        print(pcall(http.RequestAsync, http, { url }))
        print(pcall(http.RequestAsync, http, {}))
        print(pcall(http.RequestAsync, http, { url = url }))
        print(pcall(http.RequestAsync, http, { Url = url, Body = "x" }))
        print(pcall(http.RequestAsync, http, { Url = url, Method = "HEAD", Body = "" }))
        print(pcall(http.RequestAsync, http, { Url = url, Method = "BAD METHOD" }))
        print(pcall(http.RequestAsync, http, { Url = url, Method = 5 }))
        print(pcall(http.RequestAsync, http, { Url = url, Method = "POST", Body = 5 }))
        print(pcall(http.RequestAsync, http, { Url = url, Headers = "x" }))
        print(pcall(http.RequestAsync, http, { Url = url, Headers = { ["Bad Name"] = "x" } }))
        print(pcall(http.RequestAsync, http, { Url = url, Timeout = 0 }))
        print(pcall(http.RequestAsync, http, { Url = url, Timeout = 121 }))
        print(pcall(http.RequestAsync, http, { Url = url, Timeout = "x" }))
        "#,
    );
    assert_eq!(
        finish(&rt, 0),
        [
            "false URL must be http",
            "false URL must be http",
            "false Invalid URL type, URL should be a string or secret.",
            "false Empty URL",
            "false HttpService is not allowed to access that Roblox resource",
            "false HttpService is not allowed to access that Roblox resource",
            "false Header \"x-api-key\" must be provided for Roblox resources!",
            "false Unable to cast string to bool",
            "false headers value must be a dictionary!",
            "false Header \"user-agent\" is not allowed!",
            "false Header X must have its value be a string or secret!",
            "false Argument 2 missing or nil",
            "false Unable to cast string to HttpContentType",
            "false Header \"Content-Type\" is not allowed!",
            "false Unable to cast string to Dictionary",
            "false Unable to cast Array to Dictionary",
            "false Invalid request options",
            "false Unrecognized request option url",
            "false Invalid request options, Body should be empty for 'GET' or 'HEAD' method",
            "false Invalid request options, Body should be empty for 'GET' or 'HEAD' method",
            "false Invalid Http Method BAD METHOD",
            "false Invalid request options, Method should be a string",
            "false Invalid request options, Body should be a string",
            "false Invalid request options, Headers should be a table",
            "false Header \"Bad Name\" has unallowed character \" \"",
            "false Timeout value must be greater than 0 and no greater than 120 seconds.",
            "false Timeout value must be greater than 0 and no greater than 120 seconds.",
            "false Timeout value must be an integer.",
        ]
    );
    assert!(rt.take_web_requests().is_empty());
}

/// Studio 0.748 checks the arguments before whether the place enables
/// HTTP, and a client's casts before that it isn't a server.
#[test]
fn arguments_are_checked_before_the_gates() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http))
        print(pcall(http.GetAsync, http, "ftp://x"))
        print(pcall(http.GetAsync, http, "https://www.roblox.com/"))
        print(pcall(http.GetAsync, http, "http://example.com/", false, { ["Roblox-Id"] = "1" }))
        print(pcall(http.RequestAsync, http, { Url = "http://example.com/", Foo = 1 }))
        "#,
        0,
        &[
            "false Argument 1 missing or nil",
            "false URL must be http",
            "false HttpService is not allowed to access that Roblox resource",
            "false Header \"Roblox-Id\" is not allowed!",
            "false Unrecognized request option Foo",
        ],
    );
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.world_mut().is_client = true;
    rt.run_source(
        "Workspace.LocalScript",
        r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http))
        print(pcall(http.RequestAsync, http, "x"))
        print(pcall(http.GetAsync, http, "https://www.roblox.com/"))
        "#,
        None,
    )
    .unwrap();
    assert_eq!(
        finish(&rt, 0),
        [
            "false Argument 1 missing or nil",
            "false Unable to cast string to Dictionary",
            "false Http requests can only be executed by game server",
        ]
    );
}

/// 500 requests a minute go out; the next fails until the oldest is a
/// minute old.
#[test]
fn requests_past_the_limit_per_minute_fail() {
    let rt = enabled(
        r#"
        local http = game:GetService("HttpService")
        for i = 1, 500 do
            task.spawn(http.GetAsync, http, "https://example.com/" .. i)
        end
        print(pcall(http.GetAsync, http, "https://example.com/late"))
        task.wait(60.1)
        task.spawn(http.GetAsync, http, "https://example.com/later")
        print("sent")
        "#,
    );
    assert_eq!(rt.take_web_requests().len(), 500);
    assert_eq!(
        finish(&rt, 3610),
        ["false Number of requests exceeded limit", "sent"]
    );
    let later = rt.take_web_requests();
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].1.url, "https://example.com/later");
}
