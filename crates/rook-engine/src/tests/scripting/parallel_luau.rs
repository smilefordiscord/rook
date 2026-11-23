//! Where a frame runs its parallel phases
//! (docs/behavior/scripting/parallel-luau.md#phases-in-the-frame).

use crate::tests::*;

/// Each place the frame runs scripts ends with a parallel phase, so a
/// parallel handler runs after the serial handlers of its event and
/// before the frame's next event.
#[test]
fn parallel_handlers_run_after_their_events_serial_handlers() {
    let mut s = Server::new(&Options::default()).unwrap();
    let actor = add_under(&s, service(&s, "ServerScriptService"), "Actor", "Actor", "");
    add_under(
        &s,
        actor,
        "Script",
        "Script",
        r#"
        local rs = game:GetService("RunService")
        local log = {}
        local frames = 0
        rs.Heartbeat:Connect(function()
            frames += 1
            if frames == 2 then
                print(table.concat(log, " ", 1, 6))
            end
        end)
        rs.PreSimulation:ConnectParallel(function() table.insert(log, "pre-parallel") end)
        rs.PreSimulation:Connect(function() table.insert(log, "pre") end)
        rs.PostSimulation:ConnectParallel(function() table.insert(log, "post-parallel") end)
        rs.PostSimulation:Connect(function() table.insert(log, "post") end)
        rs.Heartbeat:ConnectParallel(function() table.insert(log, "heartbeat-parallel") end)
        rs.Heartbeat:Connect(function() table.insert(log, "heartbeat") end)
        "#,
    );
    s.start();
    s.run_for(0.1);
    assert_eq!(
        lines(&s),
        ["pre pre-parallel post post-parallel heartbeat heartbeat-parallel"]
    );
}
