//! HttpService's requests sent through the host's network, to a server on
//! this machine.

use crate::tests::*;

/// A server DataModel whose place enables HTTP.
fn http_enabled() -> Server {
    let s = Server::new(&Options::default()).unwrap();
    let http = service(&s, "HttpService");
    s.runtime()
        .world_mut()
        .dom
        .set_property_by_name(http, "HttpEnabled", Value::Bool(true))
        .unwrap();
    s
}

/// What goes over the wire and what comes back: the status line's code
/// with its standard reason, the answer's headers by lowercase name,
/// gzipped bodies, and Roblox's errors when no answer comes.
#[test]
fn requests_reach_the_host_and_answer() {
    let (base, requests) = serve_http(|_, request| match request.path.as_str() {
        "/teapot?x=1" => Reply {
            status: 418,
            reason: "Short And Stout",
            headers: vec![("X-Answer", "yes")],
            body: b"tip me over".to_vec(),
        },
        "/missing" => Reply {
            status: 404,
            reason: "Not Found",
            headers: Vec::new(),
            body: Vec::new(),
        },
        _ => Reply::ok(request.body.clone()),
    });
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_url = format!("http://{}/", closed.local_addr().unwrap());
    drop(closed);
    // Takes connections and never answers them.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let silent_url = format!("http://{}/", silent.local_addr().unwrap());
    let script = r#"
        local http = game:GetService("HttpService")
        local r = http:RequestAsync({
            Url = BASE .. "/teapot?x=1",
            Method = "PUT",
            Body = string.rep("rook ", 50),
            Compress = Enum.HttpCompression.Gzip,
            Headers = { ["X-Key"] = "k", ["User-Agent"] = "me" },
        })
        print(r.Success, r.StatusCode, r.StatusMessage, r.Headers["x-answer"], r.Body)
        print(pcall(http.GetAsync, http, BASE .. "/missing"))
        print(http:PostAsync(BASE .. "/echo", "a=1", Enum.HttpContentType.ApplicationUrlEncoded))
        print(pcall(http.GetAsync, http, CLOSED))
        print(pcall(http.RequestAsync, http, { Url = SILENT, Timeout = 1 }))
        print("done")
    "#
    .replace("BASE", &format!("{base:?}"))
    .replace("CLOSED", &format!("{closed_url:?}"))
    .replace("SILENT", &format!("{silent_url:?}"));
    let mut s = http_enabled();
    s.run_extra_script(&script).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "false 418 I'm a teapot yes tip me over",
            "false HTTP 404 (Not Found)",
            "a=1",
            "false HttpError: ConnectFail",
            "false HttpError: TimedOut",
            "done",
        ]
    );
    drop(silent);
    let seen: Vec<Seen> = requests.try_iter().collect();
    let lines: Vec<String> = seen.iter().skip(1).map(Seen::line).collect();
    assert_eq!(lines, ["GET /missing ", "POST /echo a=1"]);
    let put = &seen[0];
    assert_eq!(
        (put.method.as_str(), put.path.as_str()),
        ("PUT", "/teapot?x=1")
    );
    assert_eq!(put.header("content-encoding"), Some("gzip"));
    assert_eq!(put.header("x-key"), Some("k"));
    assert_ne!(put.header("user-agent"), Some("me"));
    // Gzip's magic number, and smaller than the 250 bytes it holds.
    assert_eq!(put.body[..2], [0x1f, 0x8b]);
    assert!(put.body.len() < 250);
    assert_eq!(
        seen[2].header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
}
