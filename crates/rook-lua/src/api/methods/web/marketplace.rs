//! MarketplaceService: what items are (`GetProductInfo`, the experience's
//! developer products, subscription products), who owns them, and buying
//! them through purchase prompts (docs/behavior/scripting/marketplace.md).
//!
//! Item info and ownership ask the endpoint set for their web domain
//! ([`super::requests`]), or answer locally. Purchases are Studio's test
//! purchases: nothing is charged and nothing is kept past the session.
//! Prompts, the purchases they make, receipts and the events they fire
//! are `luau/marketplace.luau`, its GUI `luau/purchase_prompt.luau`; what
//! the session bought is kept there, and ownership asks it first. Their
//! messages between server and client travel as MarketplaceService's own
//! firings (`Runtime::deliver_event`).

use mlua::{Function, IntoLua, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;
use serde_json::{Value as Json, json};

use super::RawMethod;
use super::http::from_json;
use super::requests::{self as web, WebAnswer, WebRequest};
use super::users::{ask, player_id};
use crate::api::instance::push_instance;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::types::enums::enum_item;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("MarketplaceService", "GetProductInfo" | "GetProductInfoAsync") => product_info,
        ("MarketplaceService", "PlayerOwnsAsset" | "PlayerOwnsAssetAsync") => {
            |lua, _, a| player_owns(lua, a, Item::Asset)
        }
        ("MarketplaceService", "PlayerOwnsBundle" | "PlayerOwnsBundleAsync") => {
            |lua, _, a| player_owns(lua, a, Item::Bundle)
        }
        ("MarketplaceService", "UserOwnsGamePassAsync") => owns_game_pass,
        ("MarketplaceService", "GetUserSubscriptionStatusAsync") => subscription_status,
        ("MarketplaceService", "GetSubscriptionProductInfoAsync") => subscription_info,
        ("MarketplaceService", "GetDeveloperProductsAsync") => developer_products,
        ("MarketplaceService", "PromptPurchase") => |lua, _, a| prompt(lua, a, Item::Asset),
        ("MarketplaceService", "PromptGamePassPurchase") => {
            |lua, _, a| prompt(lua, a, Item::GamePass)
        }
        ("MarketplaceService", "PromptProductPurchase") => {
            |lua, _, a| prompt(lua, a, Item::Product)
        }
        ("MarketplaceService", "PromptBundlePurchase") => |lua, _, a| prompt(lua, a, Item::Bundle),
        ("MarketplaceService", "PromptSubscriptionPurchase") => {
            |lua, _, a| prompt(lua, a, Item::Subscription)
        }
        ("MarketplaceService", "PromptPremiumPurchase") => {
            |lua, _, a| prompt(lua, a, Item::Premium)
        }
        _ => return None,
    })
}

/// What a prompt sells, by the name `marketplace.luau` knows it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    Asset,
    GamePass,
    Product,
    Bundle,
    Subscription,
    Premium,
}

impl Item {
    fn name(self) -> &'static str {
        match self {
            Self::Asset => "Asset",
            Self::GamePass => "GamePass",
            Self::Product => "Product",
            Self::Bundle => "Bundle",
            Self::Subscription => "Subscription",
            Self::Premium => "Premium",
        }
    }

    /// The `Prompt*Purchase` member's error for a player that's nil or
    /// not a Player, each worded its own way (measured).
    fn not_a_player(self) -> String {
        let member = match self {
            Self::Asset => "PromptPurchase",
            Self::GamePass => "PromptGamePassPurchase",
            Self::Product => "PromptProductPurchase",
            Self::Bundle => {
                return "MarketplaceService:PromptBundlePurchase() player should be of type Player"
                    .into();
            }
            Self::Subscription => {
                return "First argument to PromptSubscriptionPurchase must be a player".into();
            }
            Self::Premium => {
                return "First argument to PromptPremiumPurchase must be a Player".into();
            }
        };
        format!("MarketplaceService:{member}(): player should be of type Player.")
    }

    /// The inventory API's item type, for those a user keeps.
    fn inventory(self) -> Option<&'static str> {
        match self {
            Self::Asset => Some("Asset"),
            Self::GamePass => Some("GamePass"),
            Self::Bundle => Some("Bundle"),
            _ => None,
        }
    }
}

/// `marketplace.luau`'s `name` function.
fn luau(lua: &Lua, name: &str) -> mlua::Result<Function> {
    let std: Table = lua.named_registry_value("rook.std")?;
    std.get::<Table>("marketplace")?.get(name)
}

/// Whether this session bought `id` for `user`.
fn bought(lua: &Lua, item: Item, user: i64, id: impl mlua::IntoLua) -> mlua::Result<bool> {
    luau(lua, "owns")?.call((item.name(), user, id))
}

/// Whether `user` owns `id`: bought in this session, or so says
/// `GET inventory/v1/users/{user}/items/{type}/{id}` (true when `data`
/// isn't empty); with no endpoint, nothing else is owned.
pub(super) fn owns(lua: &Lua, item: Item, user: i64, id: i64) -> ApiResult<Outcome> {
    if bought(lua, item, user, id)? {
        return Outcome::ret(lua, true);
    }
    let kind = item.inventory().unwrap_or("Asset");
    let path = format!("/v1/users/{user}/items/{kind}/{id}");
    ask(lua, "inventory", &path, false, |d| !d.is_empty())
}

/// [`owns`] for a Player and an id. Measured: nil is `Argument 1 missing
/// or nil`; a bundle's player that isn't a Player is `Argument 1 is
/// invalid: expected 'Player' instance type`.
fn player_owns(lua: &Lua, a: &Args, item: Item) -> ApiResult<Outcome> {
    let item_id = a.int64(1)?;
    let player = a.object(0)?.ok_or_else(|| a.missing(0))?;
    if item == Item::Bundle {
        player_only(lua, player)?;
    }
    owns(lua, item, player_id(lua, player), item_id)
}

/// `UserOwnsGamePassAsync(user, pass)`.
fn owns_game_pass(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pass = a.int64(1)?;
    owns(lua, Item::GamePass, a.user(0)?, pass)
}

/// Raise Roblox's error for an argument 1 that isn't a Player.
fn player_only(lua: &Lua, player: InstanceId) -> ApiResult<()> {
    if with_world(lua, |w| w.dom.is_a(player, "Player")) {
        return Ok(());
    }
    Err(ScriptError(
        "Argument 1 is invalid: expected 'Player' instance type".into(),
    ))
}

/// A subscription id parameter. Measured: nil is `Argument 1 missing or
/// nil`, while a table passes the cast (a server's
/// `GetSubscriptionProductInfoAsync({})` raises its client-only error) and
/// is then refused as `Invalid SubscriptionId`, `None` here. Inference:
/// what else is refused, here anything but a non-empty string or a number.
fn subscription_id(a: &Args, i: usize) -> ApiResult<Option<String>> {
    Ok(match a.get(i) {
        LuaValue::Nil => return Err(a.missing(i)),
        LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_) => {
            Some(a.string(i)?).filter(|s| !s.is_empty())
        }
        _ => None,
    })
}

fn invalid_subscription() -> ScriptError {
    ScriptError("Invalid SubscriptionId".into())
}

/// The reason phrase Roblox's errors print after a status.
fn reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    }
}

/// `GetProductInfo`'s error for a request that failed with `status`
/// (0: no answer), as Roblox words it.
fn info_failed(status: u16, why: &str) -> String {
    format!("MarketplaceService:getProductInfo() failed because HTTP {status} ({why})")
}

/// `GetProductInfo(id, infoType)`: the item's dictionary as its web
/// domain answers it, in Roblox's shape (`economy`'s
/// `/v2/assets/{id}/details` and `/v2/developer-products/{id}/info`,
/// `apis`' `/game-passes/v1/game-passes/{id}/product-info`). With no
/// endpoint, or for bundles and subscriptions, the local answer.
fn product_info(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let kind = match a.get(1) {
        LuaValue::Nil => 0,
        _ => a.enum_value(lua, 1, "InfoType")?,
    };
    let item_id = a.int64(0)?;
    let request = match kind {
        0 => Some(("economy", format!("/v2/assets/{item_id}/details"))),
        1 => Some(("economy", format!("/v2/developer-products/{item_id}/info"))),
        2 => Some((
            "apis",
            format!("/game-passes/v1/game-passes/{item_id}/product-info"),
        )),
        _ => None,
    };
    if let Some((domain, path)) = request
        && let Some(url) = web::url(lua, domain, &path)
    {
        return web::call(lua, WebRequest::get(url), Box::new(decode_info));
    }
    // Inference: Roblox's answer for an id no item can have.
    if item_id <= 0 {
        return Err(ScriptError(info_failed(400, "Bad Request")));
    }
    let info = local_info(lua, kind, item_id);
    Outcome::ret(lua, from_json(lua, &info)?)
}

fn decode_info(lua: &Lua, answer: WebAnswer) -> Result<MultiValue, String> {
    // Roblox's message names no cause, so the cause goes to the log.
    let response = answer.map_err(|e| {
        tracing::debug!("GetProductInfo's request failed: {e}");
        info_failed(0, "HttpError: ConnectFail")
    })?;
    if !(200..300).contains(&response.status) {
        return Err(info_failed(response.status, reason(response.status)));
    }
    let json: Json = serde_json::from_slice(&response.body).map_err(|e| {
        tracing::debug!("GetProductInfo's answer isn't JSON: {e}");
        "MarketplaceService:getProductInfo() can't parse JSON".to_owned()
    })?;
    if !json.is_object() {
        return Err("MarketplaceService:getProductInfo() can't parse JSON".to_owned());
    }
    let table = from_json(lua, &json).map_err(|e| e.to_string())?;
    super::users::values(lua, table)
}

/// The local answer: every item with a positive id exists, unnamed, free
/// and for sale, made by the experience's own creator (a local session
/// knows no other). The keys are creator-docs' `GetProductInfoAsync`
/// table's, by `InfoType` (0 Asset, 1 Product, 2 GamePass, 3
/// Subscription, 4 Bundle).
fn local_info(lua: &Lua, kind: u32, item_id: i64) -> Json {
    let (creator, creator_type) = with_world(lua, |w| {
        let root = w.dom.root();
        let creator = w.dom.number_property(root, "CreatorId").unwrap_or(0.0) as i64;
        let kind = w
            .dom
            .enum_item_property(root, "CreatorType")
            .unwrap_or("User");
        (creator, kind.to_owned())
    });
    let (asset, product, product_type) = match kind {
        0 => (item_id, 0, "User Product"),
        1 => (0, item_id, "Developer Product"),
        2 => (0, 0, "Game Pass"),
        4 => (0, 0, "Bundle"),
        _ => (0, 0, "Subscription"),
    };
    json!({
        "Name": "",
        "PriceInRobux": 0,
        "UserBasePriceInRobux": 0,
        "PriceDiscountDetails": [],
        "AssetId": asset,
        "ProductId": product,
        "TargetId": item_id,
        "ProductType": product_type,
        "AssetTypeId": 0,
        "IconImageAssetId": 0,
        "Creator": {
            "CreatorType": creator_type,
            "CreatorTargetId": creator,
            "Id": creator,
            "Name": "",
            "HasVerifiedBadge": false,
        },
        "ContentRatingTypeId": 0,
        "MinimumMembershipLevel": 0,
        "IsForSale": true,
        "IsPublicDomain": false,
        "IsLimited": false,
        "IsLimitedUnique": false,
        "IsNew": false,
        "Remaining": 0,
        "Sales": 0,
        "CanBeSoldInThisGame": true,
    })
}

/// `GetDeveloperProductsAsync()`: `marketplace.luau`'s pages, as plain
/// tables (Approximation, as `GetSortedAsync`'s: Roblox returns a Pages
/// instance; scripts use the same members).
fn developer_products(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Ok(Outcome::Call(
        luau(lua, "developerProducts")?,
        MultiValue::new(),
    ))
}

/// `std.developer_products_page(page)`: page `page` (from 1) of the
/// experience's developer products and whether it's the last, from `GET
/// api/developerproducts/list?universeId={GameId}&page={page}` (reading
/// `DeveloperProducts` and `FinalPage`); with no endpoint, none.
fn developer_products_page(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "GetDeveloperProductsAsync");
    let page = a.number(0)? as i64;
    let universe = with_world(lua, |w| {
        let root = w.dom.root();
        w.dom.number_property(root, "GameId").unwrap_or(0.0) as i64
    });
    let path = format!("/developerproducts/list?universeId={universe}&page={page}");
    let Some(url) = web::url(lua, "api", &path) else {
        return Outcome::ret(lua, (lua.create_table()?, true));
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(|lua, answer| {
            let json = web::json(answer)?;
            let entries = json["DeveloperProducts"].clone();
            let entries = match entries {
                Json::Array(_) => from_json(lua, &entries),
                _ => lua.create_table().map(LuaValue::Table),
            }
            .map_err(|e| e.to_string())?;
            let last = json["FinalPage"].as_bool().unwrap_or(true);
            super::users::values(lua, (entries, last))
        }),
    )
}

/// `GetUserSubscriptionStatusAsync(user, subscription)`: subscribed, and
/// renewing, once bought in this session (Roblox's subscription service
/// has no public API to ask). The user must be a Player (measured, as
/// [`player_owns`]'s for bundles).
fn subscription_status(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let subscription = subscription_id(a, 1)?;
    let player = a.object(0)?.ok_or_else(|| a.missing(0))?;
    player_only(lua, player)?;
    let subscription = subscription.ok_or_else(invalid_subscription)?;
    let subscribed = bought(
        lua,
        Item::Subscription,
        player_id(lua, player),
        subscription,
    )?;
    let status = lua.create_table()?;
    status.set("IsSubscribed", subscribed)?;
    status.set("IsRenewing", subscribed)?;
    Outcome::ret(lua, status)
}

/// `GetSubscriptionProductInfoAsync(subscription)`: client only, as its
/// price is the local player's (creator-docs; the server's error
/// measured, raised before the id is checked). The local answer, like
/// [`local_info`]'s: unnamed, monthly, no price.
fn subscription_info(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let subscription = subscription_id(a, 0)?;
    if !with_world(lua, |w| w.is_client) {
        return Err(ScriptError(
            "Can only call GetSubscriptionProductInfoAsync from Script with RunContext = Client"
                .into(),
        ));
    }
    subscription.ok_or_else(invalid_subscription)?;
    let info = lua.create_table()?;
    info.set("Name", "")?;
    info.set("Description", "")?;
    info.set("IconImageAssetId", 0)?;
    info.set(
        "SubscriptionPeriod",
        enum_item(lua, "SubscriptionPeriod", 0)?,
    )?;
    info.set("DisplayPrice", "")?;
    info.set("DisplaySubscriptionPeriod", "")?;
    info.set("SubscriptionProviderName", "")?;
    info.set("IsForSale", true)?;
    info.set("PriceTier", 0)?;
    info.set("PriceInRobux", 0)?;
    Outcome::ret(lua, info)
}

/// `MarketplaceService.ProcessReceipt = value`. Measured: only a server
/// registers it (`Can only register ProcessReceipt callback on server`,
/// for nil too), and it takes any value, a number or a table as well as a
/// function. Inference: what isn't a function is called as it is when a
/// receipt comes, so one that can't be called errors then and leaves the
/// receipt unresolved.
pub(crate) fn set_process_receipt(lua: &Lua, id: InstanceId, value: LuaValue) -> ApiResult<()> {
    if with_world(lua, |w| w.is_client) {
        return Err(ScriptError(
            "Can only register ProcessReceipt callback on server".into(),
        ));
    }
    let callback = match value {
        LuaValue::Nil => None,
        LuaValue::Function(f) => Some(f),
        other => Some(
            lua.load("local f = ... return function(...) return f(...) end")
                .set_name("=[rook] ProcessReceipt")
                .call::<Function>(other)?,
        ),
    };
    crate::api::methods::instances::bindables::set_callback(lua, id, "ProcessReceipt", callback)?;
    Ok(())
}

/// The `Prompt*Purchase` members. Measured: their arguments cast last to
/// first (`currencyType`, `equipIfPurchased`, the id, the player), then
/// the player must be a Player, nil included, then a subscription's id be
/// valid. The prompt itself is `marketplace.luau`'s.
fn prompt(lua: &Lua, a: &Args, item: Item) -> ApiResult<Outcome> {
    if matches!(item, Item::Asset | Item::Product) {
        if !a.get(3).is_nil() {
            a.enum_value(lua, 3, "CurrencyType")?;
        }
        a.opt_bool(2)?;
    }
    let what = match item {
        Item::Subscription => subscription_id(a, 1)?
            .map(|s| s.into_lua(lua))
            .transpose()?,
        Item::Premium => Some(LuaValue::Nil),
        _ => Some(LuaValue::Number(a.int64(1)? as f64)),
    };
    let player = a
        .object(0)?
        .filter(|&p| with_world(lua, |w| w.dom.is_a(p, "Player")))
        .ok_or_else(|| ScriptError(item.not_a_player()))?;
    let what = what.ok_or_else(invalid_subscription)?;
    let args = vec![
        LuaValue::String(lua.create_string(item.name())?),
        LuaValue::UserData(push_instance(lua, player)?),
        what,
    ];
    Ok(Outcome::Call(
        luau(lua, "prompt")?,
        MultiValue::from_vec(args),
    ))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let std: Table = lua.named_registry_value("rook.std")?;
    std.set(
        "developer_products_page",
        api_fn(lua, developer_products_page)?,
    )?;
    let g = lua.globals();
    for name in ["UDim", "UDim2", "Color3", "Vector2"] {
        std.set(name, g.get::<LuaValue>(name)?)?;
    }
    let prompt: Table = lua
        .load(include_str!("../../luau/purchase_prompt.luau"))
        .set_name("=[rook] PurchasePrompt")
        .call(std.clone())?;
    std.set("purchase_prompt", prompt)?;
    // Asked on use: a DataModel becomes a client after this runs.
    std.set(
        "is_client",
        lua.create_function(|lua, ()| Ok(with_world(lua, |w| w.is_client)))?,
    )?;
    let api: Table = lua
        .load(include_str!("../../luau/marketplace.luau"))
        .set_name("=[rook] MarketplaceService")
        // `pcall` as scripts have it, which hands engine errors over as
        // their messages.
        .call((std.clone(), g.get::<Function>("pcall")?))?;
    std.set("marketplace", api)
}
