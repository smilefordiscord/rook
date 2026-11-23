//! MarketplaceService on a server: item info, developer products,
//! ownership and subscriptions, offline and from an endpoint, and the
//! prompts' argument checks (docs/behavior/scripting/marketplace.md). The
//! prompts themselves need a client: `rook-engine`'s tests.

use rook_platform::endpoints::Endpoints;

use crate::tests::*;

/// Run `source` on a server whose Players holds Cy (user 7), with every
/// web domain at `base` (offline without one), each request answered
/// with `answer(its URL)` as `(status, JSON)`. Returns the output and the
/// URLs asked.
fn run_market(
    source: &str,
    base: Option<&str>,
    answer: fn(&str) -> (u16, &'static str),
) -> (Vec<String>, Vec<String>) {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let endpoints = base.map_or_else(Endpoints::offline, |b| Endpoints::new(Some(b), []));
        w.web.endpoints = std::sync::Arc::new(endpoints);
        let players = w.dom.get_service("Players").unwrap();
        let cy = w.dom.create_by_name("Player").unwrap();
        w.dom.set_name(cy, "Cy".into()).unwrap();
        w.dom
            .set_property_by_name(cy, "UserId", rook_types::Value::Int64(7))
            .unwrap();
        w.dom.set_parent(cy, Some(players)).unwrap();
    }
    rt.run_source("Workspace.Script", source, None).unwrap();
    let mut asked = Vec::new();
    for _ in 0..3 {
        loop {
            let outbox = rt.take_web_requests();
            if outbox.is_empty() {
                break;
            }
            for (id, request) in outbox {
                let (status, body) = answer(&request.url);
                asked.push(request.url);
                let response = rook_platform::network::Response {
                    status,
                    reason: String::new(),
                    headers: Vec::new(),
                    body: body.as_bytes().to_vec(),
                };
                rt.deliver_web(id, Ok(response));
            }
        }
        step(&rt);
    }
    (finish(&rt, 0), asked)
}

fn offline(source: &str) -> Vec<String> {
    run_market(source, None, |_| (500, "")).0
}

/// Offline, every item with a positive id exists, unnamed and free, in
/// creator-docs' shape for its info type; an id no item can have raises
/// Roblox's bad-request error.
#[test]
fn product_info_answers_locally() {
    let out = offline(
        r#"
        local market = game:GetService("MarketplaceService")
        for _, kind in { Enum.InfoType.Asset, Enum.InfoType.Product, Enum.InfoType.GamePass } do
            local info = market:GetProductInfo(12, kind)
            print(kind.Name, info.Name == "", info.PriceInRobux, info.TargetId, info.AssetId, info.ProductId, info.ProductType, info.IsForSale, info.Creator.CreatorType)
        end
        print(market:GetProductInfoAsync(12).AssetId, market:GetProductInfo(12, Enum.InfoType.Bundle).ProductType)
        print(pcall(market.GetProductInfo, market, 0))
        print(pcall(market.GetProductInfo, market, 1, Enum.Material.Plastic))
        "#,
    );
    assert_eq!(
        out,
        [
            "Asset true 0 12 12 0 User Product true User",
            "Product true 0 12 0 12 Developer Product true User",
            "GamePass true 0 12 0 0 Game Pass true User",
            "12 Bundle",
            "false MarketplaceService:getProductInfo() failed because HTTP 400 (Bad Request)",
            "false Unable to cast Material to InfoType",
        ]
    );
}

/// With an endpoint, the item's info is what its web domain answers, and
/// a failed request raises with its status.
#[test]
fn product_info_asks_the_endpoint() {
    let (out, asked) = run_market(
        r#"
        local market = game:GetService("MarketplaceService")
        local info = market:GetProductInfo(5, Enum.InfoType.Product)
        print(info.Name, info.PriceInRobux, info.Creator.Name)
        print(market:GetProductInfo(6, Enum.InfoType.GamePass).Name)
        print(pcall(market.GetProductInfo, market, 404))
        "#,
        Some("http://web.test"),
        |url| match url {
            "http://web.test/economy/v2/developer-products/5/info" => (
                200,
                r#"{"Name":"Gems","PriceInRobux":25,"Creator":{"Name":"Cy"}}"#,
            ),
            "http://web.test/apis/game-passes/v1/game-passes/6/product-info" => {
                (200, r#"{"Name":"VIP"}"#)
            }
            _ => (404, "{}"),
        },
    );
    assert_eq!(
        out,
        [
            "Gems 25 Cy",
            "VIP",
            "false MarketplaceService:getProductInfo() failed because HTTP 404 (Not Found)",
        ]
    );
    assert_eq!(
        asked,
        [
            "http://web.test/economy/v2/developer-products/5/info",
            "http://web.test/apis/game-passes/v1/game-passes/6/product-info",
            "http://web.test/economy/v2/assets/404/details",
        ]
    );
}

/// Offline the experience has no developer products: one empty, final
/// page. An endpoint's pages are fetched as the script advances.
#[test]
fn developer_products_come_in_pages() {
    let script = r#"
        local pages = game:GetService("MarketplaceService"):GetDeveloperProductsAsync()
        while true do
            for _, product in pages:GetCurrentPage() do
                print(product.ProductId, product.Name)
            end
            if pages.IsFinished then break end
            pages:AdvanceToNextPageAsync()
        end
        print("done", (pcall(pages.AdvanceToNextPageAsync, pages)))
    "#;
    assert_eq!(offline(script), ["done false"]);
    let (out, asked) = run_market(script, Some("http://web.test"), |url| {
        if url.ends_with("page=1") {
            (
                200,
                r#"{"DeveloperProducts":[{"ProductId":3,"Name":"Gems"}],"FinalPage":false}"#,
            )
        } else {
            (
                200,
                r#"{"DeveloperProducts":[{"ProductId":4,"Name":"Coins"}],"FinalPage":true}"#,
            )
        }
    });
    assert_eq!(out, ["3 Gems", "4 Coins", "done false"]);
    assert_eq!(
        asked,
        [
            "http://web.test/api/developerproducts/list?universeId=0&page=1",
            "http://web.test/api/developerproducts/list?universeId=0&page=2",
        ]
    );
}

/// Offline nobody owns anything this session hasn't bought, and nobody
/// is subscribed; an endpoint's inventory answers for passes, assets and
/// bundles.
#[test]
fn ownership_answers_locally_or_from_the_inventory() {
    let script = r#"
        local market = game:GetService("MarketplaceService")
        local cy = game.Players.Cy
        print(market:UserOwnsGamePassAsync(7, 1), market:PlayerOwnsAsset(cy, 2), market:PlayerOwnsAssetAsync(cy, 2))
        print(market:PlayerOwnsBundle(cy, 3), market:PlayerOwnsBundleAsync(cy, 3))
        local status = market:GetUserSubscriptionStatusAsync(cy, "EXP-1")
        print(status.IsSubscribed, status.IsRenewing)
    "#;
    assert_eq!(
        offline(script),
        ["false false false", "false false", "false false"]
    );
    let (out, asked) = run_market(script, Some("http://web.test"), |url| {
        if url.contains("/Bundle/") {
            (200, r#"{"data":[{"id":3}]}"#)
        } else {
            (200, r#"{"data":[]}"#)
        }
    });
    assert_eq!(out, ["false false false", "true true", "false false"]);
    assert_eq!(
        asked,
        [
            "http://web.test/inventory/v1/users/7/items/GamePass/1",
            "http://web.test/inventory/v1/users/7/items/Asset/2",
            "http://web.test/inventory/v1/users/7/items/Asset/2",
            "http://web.test/inventory/v1/users/7/items/Bundle/3",
            "http://web.test/inventory/v1/users/7/items/Bundle/3",
        ]
    );
}

/// The prompts check their player; a subscription's product info is the
/// client's to ask for.
#[test]
fn prompts_need_a_player() {
    let out = offline(
        r#"
        local market = game:GetService("MarketplaceService")
        print(pcall(market.PromptProductPurchase, market, nil, 1))
        print(pcall(market.PromptPurchase, market, workspace, 1))
        print(pcall(market.PromptGamePassPurchase, market, game.Players.Cy, 1))
        print(pcall(market.GetSubscriptionProductInfoAsync, market, "EXP-1"))
        "#,
    );
    assert_eq!(
        out,
        [
            "false MarketplaceService:PromptProductPurchase(): player should be of type Player.",
            "false MarketplaceService:PromptPurchase(): player should be of type Player.",
            "true",
            "false Can only call GetSubscriptionProductInfoAsync from Script with RunContext = Client",
        ]
    );
}
