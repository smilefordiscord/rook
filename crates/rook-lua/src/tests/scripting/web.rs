//! HttpService requests: refused on clients and unless the place enables
//! HTTP, sent through the World's web outbox otherwise.

use crate::tests::*;
use rook_platform::network::Response;

fn enable_http(rt: &Runtime) {
    let mut w = rt.world_mut();
    let http = w.dom.get_service("HttpService").unwrap();
    let enabled = w.dom.property_id(http, "HttpEnabled").unwrap();
    w.dom
        .set_property(http, enabled, rook_types::Value::Bool(true))
        .unwrap();
}

#[test]
fn http_requests_refused_unless_enabled() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http, "https://example.com"))
        print(pcall(http.RequestAsync, http, { Url = "https://example.com" }))
        "#,
        0,
        &[
            "false Http requests are not enabled. Enable via game settings",
            "false Http requests are not enabled. Enable via game settings",
        ],
    );
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    enable_http(&rt);
    rt.world_mut().is_client = true;
    rt.run_source(
        "Workspace.Script",
        r#"
        local http = game:GetService("HttpService")
        print(pcall(http.PostAsync, http, "https://example.com", "{}"))
        "#,
        None,
    )
    .unwrap();
    assert_eq!(
        finish(&rt, 0),
        ["false Http requests can only be executed by game server"]
    );
}

#[test]
fn http_requests_go_out_and_answer() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    enable_http(&rt);
    rt.run_source(
        "Workspace.Script",
        r#"
        local http = game:GetService("HttpService")
        task.spawn(function()
            local r = http:RequestAsync({ Url = "https://example.com/a", Method = "post", Body = "{}", Headers = { ["X-Key"] = "k" } })
            print(r.Success, r.StatusCode, r.StatusMessage, r.Body, r.Headers["content-type"])
        end)
        task.spawn(function()
            print(pcall(http.GetAsync, http, "https://example.com/b"))
        end)
        task.spawn(function()
            print(http:PostAsync("https://example.com/c", "x=1", Enum.HttpContentType.ApplicationUrlEncoded))
        end)
        "#,
        None,
    )
    .unwrap();
    let sent = rt.take_web_requests();
    assert_eq!(sent.len(), 3);
    let (a, b, c) = (&sent[0].1, &sent[1].1, &sent[2].1);
    assert_eq!(
        (a.method.as_str(), a.url.as_str(), a.body.as_slice()),
        ("POST", "https://example.com/a", b"{}".as_slice())
    );
    assert!(a.headers.contains(&("X-Key".into(), "k".into())));
    assert_eq!(
        (b.method.as_str(), b.url.as_str()),
        ("GET", "https://example.com/b")
    );
    assert!(c.headers.contains(&(
        "Content-Type".into(),
        "application/x-www-form-urlencoded".into()
    )));
    let answer = |status: u16, body: &str| {
        Ok(Response {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.as_bytes().to_vec(),
        })
    };
    rt.deliver_web(sent[0].0, answer(404, "gone"));
    rt.deliver_web(sent[1].0, answer(503, ""));
    rt.deliver_web(sent[2].0, answer(200, "ok"));
    assert_eq!(
        finish(&rt, 1),
        [
            "false 404 Not Found gone application/json",
            "false HTTP 503 (Service Unavailable)",
            "ok",
        ]
    );
}
