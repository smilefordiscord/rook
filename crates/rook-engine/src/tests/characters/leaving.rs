//! Players leaving: `Player:Kick` from the server and from a LocalScript,
//! and the prompt a kicked client shows (docs/behavior/characters/leaving.md).

use crate::tests::*;

fn players_of(dm: &DataModel) -> Vec<String> {
    let w = dm.runtime.world();
    let players = w.dom.find_service("Players").unwrap();
    let children = w.dom.children(players);
    children.iter().map(|&p| w.dom.name(p).to_owned()).collect()
}

/// A server kick: the call returns with the player still there; then
/// `PlayerRemoving` fires with CreatorKick while it's still in Players
/// with its character, the character goes, and it leaves Players. Its
/// client stops and shows the prompt; the other client sees it go.
#[test]
fn a_kicked_player_leaves_and_its_client_stops() {
    let setup = r#"local Players = game:GetService("Players")
        Players.PlayerRemoving:Connect(function(p, reason)
            print("removing", p.Name, p.Parent == Players, p.Character ~= nil, reason)
        end)
        Players.ChildRemoved:Connect(function(c) print("child removed", c.Name) end)
        Players.PlayerAdded:Connect(function(p)
            if p.Name ~= "A" then return end
            p.CharacterRemoving:Connect(function(c) print("character removing", c.Name) end)
            p.CharacterAdded:Wait()
            task.wait(0.5)
            p:Kick("bye")
            print("kick returned", p.Parent == Players)
            task.wait(0.1)
            print("after", p.Parent, p.Character, #workspace:GetChildren())
        end)
        "#;
    let mut session = session_with(setup, &["A", "B"], 1.0, &Options::default());
    let server = session.server();
    let out = lines(server);
    let at = out
        .iter()
        .position(|l| l.starts_with("kick returned"))
        .unwrap();
    assert_eq!(
        out[at..at + 4],
        [
            "kick returned true",
            "removing A true true Enum.PlayerExitReason.CreatorKick",
            "character removing A",
            "child removed A",
        ]
    );
    assert!(out[at + 4].starts_with("after nil nil"), "{out:?}");
    assert_eq!(players_of(server), ["B"]);
    let a = session.client(0).unwrap();
    assert!(a.session_ended() && !a.left());
    assert!(a.runtime.world().gui.prompt_shown());
    let frozen = a.now();
    session.run_for(0.5);
    let a = session.client(0).unwrap();
    assert_eq!(a.now(), frozen, "the kicked client stopped");
    let b = session.client(1).unwrap();
    assert!(!b.session_ended());
    assert_eq!(players_of(b), ["B"]);
}

/// A LocalScript can kick only its own player; doing so ends its
/// connection, and the server sees the player leave.
#[test]
fn a_local_script_kicks_only_its_own_player() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(
        &server,
        "ServerScriptService",
        "Watch",
        r#"game:GetService("Players").PlayerRemoving:Connect(function(p, reason)
            print("removing", p.Name, reason)
        end)"#,
    );
    add_local_script(
        &server,
        "Kick",
        r#"local Players = game:GetService("Players")
        local me = Players.LocalPlayer
        task.wait(0.2)
        if me.Name ~= "A" then return end
        for _, p in Players:GetPlayers() do
            if p ~= me then print(pcall(function() p:Kick() end)) end
        end
        me:Kick("self")
        print("after kick")
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    session.join("B", 2).unwrap();
    session.run_for(1.0);
    assert_eq!(
        lines(session.client(0).unwrap()),
        [
            "false Cannot kick a non-local Player from a LocalScript",
            "after kick"
        ]
    );
    assert!(session.client(0).unwrap().session_ended());
    assert!(!session.client(1).unwrap().session_ended());
    let server = session.server();
    assert_eq!(lines(server), ["removing A Enum.PlayerExitReason.Unknown"]);
    assert_eq!(players_of(server), ["B"]);
}

/// The prompt takes the window's input: only pressing and releasing the
/// mouse on its button leaves, so a button held down when the kick came
/// doesn't, and it's centered again when the window's size changes.
#[test]
fn the_prompt_leaves_from_its_button() {
    let options = Options::default();
    let mut server = Server::new(&options).unwrap();
    use_r6(&server);
    server.start();
    let mut session = Session::new(server, options);
    session.join("A", 1).unwrap();
    session.run_for(0.1);
    let client = session.client_mut(0).unwrap();
    client.set_viewport(800, 600);
    client.end_session(crate::session::ended::kick_prompt("bye"));
    let draw = |dm: &DataModel| {
        let mut w = dm.runtime.world_mut();
        let w = &mut *w;
        w.gui.draw(&w.dom);
    };
    draw(client);
    let on_button = |dm: &DataModel, x: f32, y: f32| {
        let at = glam::Vec2::new(x, y);
        dm.runtime.world().gui.prompt_button_at(at)
    };
    let y = (0..600)
        .map(|y| y as f32)
        .find(|&y| on_button(client, 400.0, y));
    let y = y.expect("the prompt has a button") + 2.0;
    let button = |dm: &mut DataModel, down| {
        dm.input(InputEvent::MouseButton { button: 1, down });
    };
    client.input(InputEvent::MouseMove { x: 100.0, y });
    button(client, false);
    assert!(!client.left(), "off the button");
    client.input(InputEvent::MouseMove { x: 400.0, y });
    button(client, false);
    assert!(!client.left(), "released on the button, pressed elsewhere");
    assert!(!on_button(client, 200.0, y));
    client.set_viewport(400, 600);
    draw(client);
    assert!(on_button(client, 200.0, y), "centered on the new size");
    client.input(InputEvent::MouseMove { x: 200.0, y });
    button(client, true);
    assert!(!client.left());
    button(client, false);
    assert!(client.left());
    assert_eq!(client.mouse_lock(), MouseLock::Free);
}
