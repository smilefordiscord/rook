//! Purchase prompts in a play session: the client's prompt, the events
//! both sides fire when it closes, receipts on the server and what the
//! session owns afterwards (docs/behavior/scripting/marketplace.md).

use crate::tests::*;

/// A session with one player, whose server runs `server_source` and
/// whose client runs `client_source` as a LocalScript.
fn session_with(server_source: &str, client_source: &str) -> (Session, usize) {
    let server = Server::new(&Options::default()).unwrap();
    add_script(&server, "ServerScriptService", "Shop", server_source);
    if !client_source.is_empty() {
        add_local_script(&server, "Buyer", client_source);
    }
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(800, 600);
    session.run_for(0.5);
    (session, index)
}

/// What the client printed since last asked.
fn client_lines(session: &Session, index: usize) -> Vec<String> {
    lines(session.client(index).unwrap())
}

/// Click the open prompt's button called `name` as the player would.
fn click(session: &mut Session, index: usize, name: &str) {
    let (x, y) = {
        let w = session.client(index).unwrap().runtime().world();
        let dom = &w.dom;
        let button = ["PurchasePrompt", "Panel", "Buttons", name]
            .iter()
            .try_fold(dom.find_service("CoreGui").unwrap(), |at, child| {
                dom.find_first_child(at, child, false)
            })
            .unwrap_or_else(|| panic!("no {name} button"));
        let at = dom.vector2_property(button, "AbsolutePosition").unwrap();
        let size = dom.vector2_property(button, "AbsoluteSize").unwrap();
        // AbsolutePosition is counted from below the top bar's inset.
        (
            at.x() + size.x() / 2.0,
            at.y() + size.y() / 2.0 + rook_gui::GUI_INSET,
        )
    };
    session.input(index, InputEvent::MouseMove { x, y });
    session.run_for(0.05);
    for down in [true, false] {
        session.input(index, InputEvent::MouseButton { button: 1, down });
    }
    session.run_for(0.2);
}

/// A server's prompt for a developer product shows on the player's
/// client. Buying hands ProcessReceipt the receipt at once; closing the
/// success message fires PromptProductPurchaseFinished on both sides with
/// the player's id. A receipt not granted waits for the player's next
/// purchase, which hands over both.
#[test]
fn a_developer_product_bought_reaches_process_receipt() {
    let (mut session, index) = session_with(
        r#"
        local market = game:GetService("MarketplaceService")
        local calls = 0
        market.ProcessReceipt = function(r)
            calls += 1
            print("receipt", calls, r.PlayerId, r.ProductId, r.PlaceIdWherePurchased, r.CurrencySpent,
                r.CurrencyType, r.ProductPurchaseChannel, typeof(r.PurchaseId))
            if calls == 1 then
                return Enum.ProductPurchaseDecision.NotProcessedYet
            end
            return Enum.ProductPurchaseDecision.PurchaseGranted
        end
        market.PromptProductPurchaseFinished:Connect(function(user, product, bought)
            print("finished", user, product, bought)
        end)
        game.Players.PlayerAdded:Connect(function(player)
            task.wait(0.2)
            market:PromptProductPurchase(player, 42)
        end)
        "#,
        r#"
        game:GetService("MarketplaceService").PromptProductPurchaseFinished:Connect(function(user, product, bought)
            print("client finished", user, product, bought)
        end)
        "#,
    );
    let server = |s: &Session| lines(s.server());
    server(&session);
    click(&mut session, index, "Buy");
    assert_eq!(
        server(&session),
        [
            "receipt 1 1 42 0 0 Enum.CurrencyType.Robux Enum.ProductPurchaseChannel.InExperience string"
        ]
    );
    click(&mut session, index, "OK");
    assert_eq!(server(&session), ["finished 1 42 true"]);
    assert_eq!(client_lines(&session, index), ["client finished 1 42 true"]);

    // The second purchase hands over the first receipt again, and its own.
    session
        .server()
        .run_extra_script(
            "game:GetService('MarketplaceService'):PromptProductPurchase(game.Players.Player1, 43)",
        )
        .unwrap();
    session.run_for(0.2);
    click(&mut session, index, "Buy");
    let mut receipts = server(&session);
    receipts.sort();
    assert_eq!(
        receipts,
        [
            "receipt 2 1 42 0 0 Enum.CurrencyType.Robux Enum.ProductPurchaseChannel.InExperience string",
            "receipt 3 1 43 0 0 Enum.CurrencyType.Robux Enum.ProductPurchaseChannel.InExperience string",
        ]
    );
}

/// A client prompts its own player for a pass. Cancel finishes it unbought;
/// a pass bought is owned on both sides for the rest of the session, and
/// a prompt for it again says so and finishes unbought.
#[test]
fn a_pass_bought_is_owned_for_the_session() {
    let (mut session, index) = session_with(
        r#"
        local market = game:GetService("MarketplaceService")
        market.PromptGamePassPurchaseFinished:Connect(function(player, pass, bought)
            print("finished", player.Name, pass, bought, market:UserOwnsGamePassAsync(player.UserId, pass))
        end)
        "#,
        r#"
        local market = game:GetService("MarketplaceService")
        local me = game.Players.LocalPlayer
        market.PromptGamePassPurchaseFinished:Connect(function(player, pass, bought)
            print("client finished", player == me, pass, bought, market:UserOwnsGamePassAsync(me.UserId, pass))
        end)
        for _ = 1, 3 do
            market:PromptGamePassPurchase(me, 9)
            market.PromptGamePassPurchaseFinished:Wait()
            task.wait(0.1)
        end
        "#,
    );
    click(&mut session, index, "Cancel");
    click(&mut session, index, "Buy");
    click(&mut session, index, "OK");
    click(&mut session, index, "OK");
    assert_eq!(
        client_lines(&session, index),
        [
            "client finished true 9 false false",
            "client finished true 9 true true",
            "client finished true 9 false true",
        ]
    );
    assert_eq!(
        lines(session.server()),
        [
            "finished Player1 9 false false",
            "finished Player1 9 true true",
            "finished Player1 9 false true",
        ]
    );
}

/// A subscription bought: its Finished event, then Players'
/// UserSubscriptionStatusChanged on the server, and the status after.
#[test]
fn a_subscription_bought_changes_the_players_status() {
    let (mut session, index) = session_with(
        r#"
        local market = game:GetService("MarketplaceService")
        market.PromptSubscriptionPurchaseFinished:Connect(function(player, id, tried)
            print("finished", player.Name, id, tried)
        end)
        game.Players.UserSubscriptionStatusChanged:Connect(function(player, id)
            print("changed", player.Name, id, market:GetUserSubscriptionStatusAsync(player, id).IsSubscribed)
        end)
        game.Players.PlayerAdded:Connect(function(player)
            task.wait(0.2)
            market:PromptSubscriptionPurchase(player, "EXP-5")
        end)
        "#,
        "",
    );
    click(&mut session, index, "Buy");
    click(&mut session, index, "OK");
    assert_eq!(
        lines(session.server()),
        ["finished Player1 EXP-5 true", "changed Player1 EXP-5 true"]
    );
}

/// A ProcessReceipt that errors reports its error and leaves the receipt
/// unresolved, so the next purchase hands it over again.
#[test]
fn a_receipt_whose_callback_errors_comes_back() {
    let (mut session, index) = session_with(
        r#"
        local market = game:GetService("MarketplaceService")
        local calls = 0
        market.ProcessReceipt = function(r)
            calls += 1
            if calls == 1 then
                error("no data store")
            end
            print("receipt", r.ProductId)
            return Enum.ProductPurchaseDecision.PurchaseGranted
        end
        game.Players.PlayerAdded:Connect(function(player)
            task.wait(0.2)
            market:PromptProductPurchase(player, 42)
            market.PromptProductPurchaseFinished:Wait()
            market:PromptProductPurchase(player, 43)
        end)
        "#,
        "",
    );
    click(&mut session, index, "Buy");
    click(&mut session, index, "OK");
    assert_eq!(
        lines(session.server()),
        ["ServerScriptService.Shop:7: no data store"]
    );
    session.run_for(0.2);
    click(&mut session, index, "Buy");
    let mut out = lines(session.server());
    out.sort();
    assert_eq!(out, ["receipt 42", "receipt 43"]);
}
