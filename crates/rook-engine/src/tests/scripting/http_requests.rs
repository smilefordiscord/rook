//! HttpService's requests sent through the host's network, to a server on
//! this machine.

use crate::tests::*;

/// A server DataModel whose place enables HTTP; with `local`, the host
/// lets its requests reach local addresses, as the servers these tests
/// stand up are.
fn http_enabled(local: bool) -> Server {
    let s = Server::new(&Options {
        http_local_addresses: Some(local),
        ..Options::default()
    })
    .unwrap();
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
    let mut s = http_enabled(true);
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

/// No host's frame waits for an answer: the script that asked waits, and
/// the DataModel steps on. A server that holds its answer back must not
/// hold the frame (rook-server's frame waited out each request).
#[test]
fn a_slow_server_holds_up_no_frame() {
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (base, _requests) = serve_http(move |_, _| {
        // The test may be over, with no one left to release it.
        let _ = released.recv();
        Reply::ok(b"late".to_vec())
    });
    let script = r#"
        local http = game:GetService("HttpService")
        task.spawn(function()
            print(http:GetAsync(BASE .. "/slow"))
        end)
        local frames = 0
        game:GetService("RunService").Heartbeat:Connect(function()
            frames += 1
            if frames == 30 then
                print("stepped")
            end
        end)
    "#
    .replace("BASE", &format!("{base:?}"));
    let mut s = http_enabled(true);
    s.run_extra_script(&script).unwrap();
    let started = std::time::Instant::now();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "stepped"));
    assert_eq!(out, ["stepped"]);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    release.send(()).unwrap();
    assert_eq!(step_until(&mut s, |out| !out.is_empty()), ["late"]);
}

/// Loopback, the LAN and the like are off the public internet, which is
/// all a Roblox game server reaches; the host's setting opens them.
#[test]
fn local_addresses_are_refused_unless_the_host_allows_them() {
    let (base, requests) = serve_http(|_, _| Reply::ok(b"reached".to_vec()));
    let port = base.rsplit_once(':').unwrap().1;
    let script = r#"
        local http = game:GetService("HttpService")
        for _, url in { BASE, "http://localhost:PORT/", "http://[::ffff:127.0.0.1]:PORT/", "http://169.254.169.254/" } do
            print(pcall(http.GetAsync, http, url))
        end
        print("done")
    "#
    .replace("BASE", &format!("{base:?}"))
    .replace("PORT", port);
    let mut s = http_enabled(false);
    s.run_extra_script(&script).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "false HttpError: ConnectFail",
            "false HttpError: ConnectFail",
            "false HttpError: ConnectFail",
            "false HttpError: ConnectFail",
            "done",
        ]
    );
    assert_eq!(requests.try_iter().count(), 0);

    let mut s = http_enabled(true);
    s.run_extra_script(&format!(
        "print(game:GetService('HttpService'):GetAsync({base:?}))"
    ))
    .unwrap();
    assert_eq!(step_until(&mut s, |out| !out.is_empty()), ["reached"]);
}

/// A redirect is checked as the first URL is, so it can't reach Roblox's
/// web domains; one that may go is followed.
#[test]
fn redirects_are_checked_like_the_first_request() {
    let (base, requests) = serve_http(|_, request| {
        let to = match request.path.as_str() {
            "/to-roblox" => "https://www.roblox.com/home",
            "/to-here" => "/landed",
            _ => return Reply::ok(b"landed".to_vec()),
        };
        Reply {
            status: 302,
            reason: "Found",
            headers: vec![("Location", to)],
            body: Vec::new(),
        }
    });
    let script = r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http, BASE .. "/to-roblox"))
        print(pcall(http.GetAsync, http, BASE .. "/to-here"))
        print("done")
    "#
    .replace("BASE", &format!("{base:?}"));
    let mut s = http_enabled(true);
    s.run_extra_script(&script).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "false HttpService is not allowed to access ROBLOX resources",
            "true landed",
            "done",
        ]
    );
    let paths: Vec<String> = requests.try_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/to-roblox", "/to-here", "/landed"]);
}

/// An answer past the limit raises rather than arriving cut short, and a
/// header HTTP can't carry is refused without anything sent.
#[test]
fn oversized_answers_and_bad_headers_raise() {
    let (base, requests) = serve_http(|_, _| Reply::ok(vec![b'x'; (16 << 20) + 1]));
    let script = r#"
        local http = game:GetService("HttpService")
        print(pcall(http.GetAsync, http, BASE .. "/big"))
        print(pcall(http.RequestAsync, http, { Url = BASE, Headers = { ["Bad Name"] = "x" } }))
        print(pcall(http.RequestAsync, http, { Url = BASE, Headers = { ["X-Value"] = "a" .. string.char(10) .. "b" } }))
        print("done")
    "#
    .replace("BASE", &format!("{base:?}"));
    let mut s = http_enabled(true);
    s.run_extra_script(&script).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "false HttpError: OutOfMemory",
            "false HttpError: Unknown",
            "false HttpError: Unknown",
            "done",
        ]
    );
    let paths: Vec<String> = requests.try_iter().map(|r| r.path).collect();
    assert_eq!(paths, ["/big"]);
}
