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

use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;
use serde_json::{Value as Json, json};

use super::RawMethod;
use super::http::from_json;
use super::requests::{self as web, WebAnswer, WebRequest};
use super::users::{ask, id, player_id};
use crate::api::instance::push_instance;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::types::enums::enum_item;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("MarketplaceService", "GetProductInfo" | "GetProductInfoAsync") => product_info,
        ("MarketplaceService", "PlayerOwnsAssetAsync") => {
            |lua, _, a| player_owns(lua, a, Item::Asset)
        }
        ("MarketplaceService", "PlayerOwnsBundle" | "PlayerOwnsBundleAsync") => {
            |lua, _, a| player_owns(lua, a, Item::Bundle)
        }
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

    fn member(self) -> &'static str {
        match self {
            Self::Asset => "PromptPurchase",
            Self::GamePass => "PromptGamePassPurchase",
            Self::Product => "PromptProductPurchase",
            Self::Bundle => "PromptBundlePurchase",
            Self::Subscription => "PromptSubscriptionPurchase",
            Self::Premium => "PromptPremiumPurchase",
        }
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

/// [`owns`] for a Player and an id.
pub(super) fn player_owns(lua: &Lua, a: &Args, item: Item) -> ApiResult<Outcome> {
    let player = a.instance(0)?;
    let item_id = id(a, 1)?;
    owns(lua, item, player_id(lua, player), item_id)
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
    let item_id = id(a, 0)?;
    let kind = match a.get(1) {
        LuaValue::Nil => 0,
        _ => a.enum_value(lua, 1, "InfoType")?,
    };
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
    let response = answer.map_err(|_| info_failed(0, "HttpError: ConnectFail"))?;
    if !(200..300).contains(&response.status) {
        return Err(info_failed(response.status, reason(response.status)));
    }
    let json: Json = serde_json::from_slice(&response.body)
        .map_err(|_| "MarketplaceService:getProductInfo() can't parse JSON".to_owned())?;
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
/// has no public API to ask).
fn subscription_status(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let player = a.instance(0)?;
    let subscription = a.string(1)?;
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
/// price is the local player's (creator-docs). The local answer, like
/// [`local_info`]'s: unnamed, monthly, no price.
fn subscription_info(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    a.string(0)?;
    if !with_world(lua, |w| w.is_client) {
        // Inference: the wording of the server's error.
        return Err(ScriptError(
            "GetSubscriptionProductInfoAsync can only be called from the client".into(),
        ));
    }
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

/// The `Prompt*Purchase` members: the player must be a Player; the
/// prompt itself is `marketplace.luau`'s.
fn prompt(lua: &Lua, a: &Args, item: Item) -> ApiResult<Outcome> {
    let player = a.instance(0)?;
    let class = with_world(lua, |w| {
        (!w.dom.is_a(player, "Player")).then(|| w.dom.class_name(player).to_owned())
    });
    if let Some(class) = class {
        // Inference: the wording, from creator-docs' parameter types.
        return Err(ScriptError(format!(
            "MarketplaceService:{}() player should be of type Player, but is of type {class}",
            item.member()
        )));
    }
    let what = match item {
        Item::Subscription => LuaValue::String(lua.create_string(a.string(1)?)?),
        Item::Premium => LuaValue::Nil,
        _ => LuaValue::Number(id(a, 1)? as f64),
    };
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
