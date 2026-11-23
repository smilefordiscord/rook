use super::*;
use crate::runtime::scheduler::Queue;

/// Run `source` as a script, step `frames` 60 Hz frames, and return the
/// output lines (errors prefixed with `ERR `, warnings with `WARN `).
fn run(source: &str, frames: u32) -> Vec<String> {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source("Workspace.Script", source, None).unwrap();
    finish(&rt, frames)
}

/// [`run`], as Roblox's own code: a script that came compiled.
fn run_core(source: &str, frames: u32) -> Vec<String> {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    let script = rt.world_mut().dom.create_by_name("Script").unwrap();
    let code = mlua::chunk::Compiler::new().compile(source).unwrap();
    let core = Compiled {
        code: code.into(),
        core: true,
    };
    rt.world_mut().bytecode.insert(script, core);
    assert!(rt.run_script(script));
    finish(&rt, frames)
}

fn finish(rt: &Runtime, frames: u32) -> Vec<String> {
    for _ in 0..frames {
        step(rt);
    }
    rt.take_log()
        .into_iter()
        .map(|e| match e.kind {
            MessageType::Error => format!("ERR {}", e.message),
            MessageType::Warning => format!("WARN {}", e.message),
            _ => e.message,
        })
        .collect()
}

fn step(rt: &Runtime) {
    let dt = rt.world_mut().scheduler.begin_frame();
    rt.render_step(dt);
    rt.restyle();
    rt.resume_due(Queue::Legacy);
    let rs = rt.world().dom.find_service("RunService");
    if let Some(rs) = rs {
        rt.fire_event(rs, "Heartbeat", dt);
        rt.drain_deferred();
    }
    rt.resume_due(Queue::Task);
}

/// [`run`] with Ann (user 5) in Players and the web's domains at `base`
/// (offline without one), each request answered with `answer(its URL)`'s
/// JSON. Returns the output lines and the requests, as `METHOD url`.
fn run_with_ann(
    source: &str,
    frames: u32,
    base: Option<&str>,
    answer: fn(&str) -> &'static str,
) -> (Vec<String>, Vec<String>) {
    use rook_platform::endpoints::Endpoints;
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let endpoints = base.map_or_else(Endpoints::offline, |b| Endpoints::new(Some(b), []));
        w.web.endpoints = std::sync::Arc::new(endpoints);
        let players = w.dom.get_service("Players").unwrap();
        let ann = w.dom.create_by_name("Player").unwrap();
        w.dom.set_name(ann, "Ann".into()).unwrap();
        w.dom
            .set_property_by_name(ann, "UserId", rook_types::Value::Int64(5))
            .unwrap();
        w.dom.set_parent(ann, Some(players)).unwrap();
    }
    rt.run_source("Workspace.Script", source, None).unwrap();
    let mut requests = Vec::new();
    for _ in 0..frames {
        loop {
            let outbox = rt.take_web_requests();
            if outbox.is_empty() {
                break;
            }
            for (id, request) in outbox {
                requests.push(format!("{} {}", request.method, request.url));
                let response = rook_platform::network::Response {
                    status: 200,
                    headers: Vec::new(),
                    body: answer(&request.url).as_bytes().to_vec(),
                };
                rt.deliver_web(id, Ok(response));
            }
        }
        step(&rt);
    }
    (finish(&rt, 0), requests)
}

#[track_caller]
fn expect(source: &str, frames: u32, lines: &[&str]) {
    let out = run(source, frames);
    assert_eq!(out, lines, "\nsource:\n{source}");
}

mod characters;
mod gui;
mod physics;
mod pivots;
mod scripting;
