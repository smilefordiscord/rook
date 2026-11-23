//! `Player:Kick` and players leaving (docs/behavior/characters/leaving.md).

use rook_dom::InstanceId;

use crate::tests::*;

/// A server with a player named "A" in Players.
fn with_player() -> (Runtime, InstanceId) {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    let player = {
        let mut w = rt.world_mut();
        let players = w.dom.get_service("Players").unwrap();
        let player = w.dom.create_by_name("Player").unwrap();
        w.dom.set_name(player, "A".into()).unwrap();
        w.dom.set_parent(player, Some(players)).unwrap();
        player
    };
    (rt, player)
}

/// A server's message over 500 bytes is dropped for the default notice
/// (creator-docs); the call only asks, so the player is still there.
#[test]
fn a_server_kick_waits_for_the_session() {
    let (rt, player) = with_player();
    rt.run_source(
        "Workspace.Script",
        r#"local a = game.Players.A
        a:Kick("bye")
        a:Kick(string.rep("x", 501))
        a:Kick(string.rep("x", 500))
        a:Kick()
        print(a.Parent)"#,
        None,
    )
    .unwrap();
    assert_eq!(finish(&rt, 1), ["Players"]);
    let kicks = rt.take_kicks();
    let messages: Vec<usize> = kicks.iter().map(|(_, m)| m.len()).collect();
    assert_eq!(messages, [3, 0, 500, 0]);
    assert!(kicks.iter().all(|&(p, _)| p == player));
}

/// Leaving fires `PlayerRemoving` with the reason while the player is in
/// Players, then removes it; a player already gone is left alone.
#[test]
fn removing_a_player_fires_player_removing_first() {
    let (rt, player) = with_player();
    rt.run_source(
        "Workspace.Script",
        r#"local Players = game:GetService("Players")
        Players.PlayerRemoving:Connect(function(p, reason)
            print(p.Name, p.Parent, reason)
        end)
        Players.ChildRemoved:Connect(function(c) print("removed", c.Name) end)"#,
        None,
    )
    .unwrap();
    rt.remove_player(player, ExitReason::CreatorKick);
    rt.remove_player(player, ExitReason::Unknown);
    assert_eq!(
        finish(&rt, 1),
        ["A Players Enum.PlayerExitReason.CreatorKick", "removed A"]
    );
    assert_eq!(rt.world().dom.parent(player), None);
}
