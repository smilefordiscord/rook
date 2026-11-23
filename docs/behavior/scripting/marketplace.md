# Marketplace

Checked against Studio 0.748 on 2026-11-23, from the creator docs only: nothing here is measured in Studio.

MarketplaceService: what items are, who owns them, and buying them
through purchase prompts, as a Studio playtest does it. Sources are
creator-docs' MarketplaceService and Players class pages and Roblox's
announcement of mock purchases in Studio (DevForum, 2016). The code is
`rook-lua`'s `api/methods/web/marketplace.rs` (item info, ownership, the
members' arguments), `api/luau/marketplace.luau` (prompts, purchases,
receipts, events) and `api/luau/purchase_prompt.luau` (the prompt's
GUI); the tests are `rook-lua`'s `tests/scripting/marketplace.rs` and
`rook-engine`'s `tests/scripting/marketplace.rs`.

## Item info

- `GetProductInfo(id, infoType)` and `GetProductInfoAsync` return a
  dictionary with creator-docs' keys (`Name`, `Description`,
  `PriceInRobux`, `AssetId`, `ProductId`, `TargetId`, `ProductType`,
  `Creator`, `IsForSale`, ...); `infoType` defaults to `Asset`
  (Confirmed: creator-docs).
- With an endpoint, the dictionary is the web domain's answer as it
  comes: `economy`'s `/v2/assets/{id}/details` for assets and
  `/v2/developer-products/{id}/info` for developer products, `apis`'
  `/game-passes/v1/game-passes/{id}/product-info` for passes (Inference:
  Roblox's public web APIs that answer in this shape). A failed request
  raises `MarketplaceService:getProductInfo() failed because HTTP 404
  (Not Found)` with its status, and no answer `HTTP 0 (HttpError:
  ConnectFail)` (Inference: the wording reported on the DevForum, as in
  `HTTP 0 (HttpError: Timedout)` and `HTTP 400 (Bad Request)`).
- With no endpoint, and for bundles and subscriptions, which have no
  request, every item with a positive id exists: unnamed (`Name` is
  empty, `Description` nil), free (`PriceInRobux` 0), for sale, and made
  by the experience's own creator (`game.CreatorId` and `CreatorType`),
  with `TargetId` the id and `AssetId` or `ProductId` the id for assets
  and developer products (Approximation: a local session knows no
  catalog; this keeps shops that read prices running). An id of 0 or
  less raises `... failed because HTTP 400 (Bad Request)` (Inference).
- `GetDeveloperProductsAsync()` returns pages of the experience's
  developer products, a page fetched as the script advances to it:
  with an endpoint, `api`'s `/developerproducts/list?universeId={GameId}&page={n}`,
  reading `DeveloperProducts` and `FinalPage` (Inference: the legacy
  list Roblox's engine read); with none, one empty last page. The pages
  are plain tables with `GetCurrentPage`, `AdvanceToNextPageAsync` and
  `IsFinished` (Approximation, as DataStores' `GetSortedAsync`: Roblox
  returns a Pages instance). Advancing past the last page raises `No
  pages to advance to` (Inference).
- `GetSubscriptionProductInfoAsync(id)` is the client's only, as its
  price is the local player's (Confirmed: creator-docs); on a server it
  raises an error worded by Rook. Locally it answers unnamed, monthly
  (`Enum.SubscriptionPeriod.Month`), for sale, with empty display
  prices.

## Ownership

- `UserOwnsGamePassAsync(user, pass)`, `PlayerOwnsAsset` and
  `PlayerOwnsAssetAsync(player, asset)`, `PlayerOwnsBundle` and
  `PlayerOwnsBundleAsync(player, bundle)` are true for what the session
  bought, on the side that knows of the purchase: the server, and the
  buyer's client. Confirmed for assets: Studio's mock purchases make
  `PlayerOwnsAsset` true for the rest of the session and are kept no
  longer (Roblox's 2016 announcement). Inference for passes (creator-docs:
  the ownership cache follows `PromptGamePassPurchaseFinished`) and
  bundles.
- Otherwise they ask the `inventory` domain
  (`/v1/users/{user}/items/{Asset|GamePass|Bundle}/{id}`), true when
  `data` isn't empty; with no endpoint, nobody owns anything else.
- Developer products are bought again and again, never owned
  (Confirmed: creator-docs PlayerOwnsAssetAsync).
- `GetUserSubscriptionStatusAsync(user, id)` returns `{IsSubscribed,
  IsRenewing}` (Confirmed: creator-docs), both true once the session
  bought the subscription. Roblox's subscription service has no public
  API, so there is no endpoint.

## Purchase prompts

- `PromptPurchase` (assets), `PromptGamePassPurchase`,
  `PromptProductPurchase`, `PromptBundlePurchase`,
  `PromptSubscriptionPurchase` and the deprecated `PromptPremiumPurchase`
  can be called from a server for any player, or from a client
  (Confirmed: creator-docs). The player must be a Player: nil raises
  `Argument 1 missing or nil`, another instance `MarketplaceService:PromptPurchase()
  player should be of type Player, but is of type Workspace` (Inference:
  the wording). A client prompting another player does nothing
  (Inference). `equipIfPurchased` and `currencyType` are ignored
  (Confirmed: creator-docs).
- The prompt shows on the player's client. One shows at a time; a
  prompt asked for while one is open is dropped (Confirmed for
  `PromptVIPServerPurchase` by creator-docs, Inference for the rest).
  The server decides: it keeps each player's open prompt, and a client
  prompting its own player asks the server to open it.
- Purchases are Studio's test purchases: the player is never charged,
  and nothing bought is kept past the session (Confirmed: Roblox's
  announcement), and an item from another experience can be bought
  (Confirmed: creator-docs, "mocked purchases don't check that the
  product belongs to the current game").
- The prompt shows the item's name (or its kind and id when unnamed)
  and price. Buy and Cancel answer it. Buying shows a success message
  with OK. An item that can't be described (its `GetProductInfo`
  raises), isn't for sale, or is owned already (a subscription the
  player has included, as creator-docs' `UserSubscriptionStatusChanged`
  describes) shows a message with OK instead, and finishes unbought
  (Inference for the wording).
- The prompt is Rook's own, a ScreenGui named `PurchasePrompt` in the
  client's CoreGui: a dimmed screen and a centered panel with the
  item, its price, "Test purchase: your account will not be charged."
  and the buttons, each a Modal button so a held mouse is freed
  (Approximation: not laid out from Roblox's prompt, which Roblox's
  CoreScripts draw; M8 step 9 makes Rook's own CoreGui look like
  Roblox's).

## When a prompt closes

- Its Finished event fires on the client, then on the server
  (Inference: the order), with creator-docs' arguments:
  `PromptPurchaseFinished(player, assetId, isPurchased)`,
  `PromptGamePassPurchaseFinished(player, gamePassId, wasPurchased)`,
  `PromptProductPurchaseFinished(userId, productId, isPurchased)` (the
  user's id, not the Player), `PromptBundlePurchaseFinished(player,
  bundleId, wasPurchased)`, `PromptSubscriptionPurchaseFinished(user,
  subscriptionId, didTryPurchasing)` and `PromptPremiumPurchaseFinished()`.
  The purchase flag is true only after Buy; Cancel, or a prompt that
  couldn't sell, gives false (Confirmed: creator-docs).
- The events fire when the prompt closes: after the success message's
  OK, not at Buy (Confirmed: creator-docs, "when they receive a success
  or error message and click OK").
- After a subscription is bought, the server fires
  `Players.UserSubscriptionStatusChanged(player, subscriptionId)` when
  its prompt closes, after the Finished event, on the server only
  (Confirmed: creator-docs Players, "the server only attempts to check
  and update the status after the Subscription Purchase modal has been
  closed"; Inference: the order).
- Premium can't be bought here: Buy closes it and its event fires, but
  `MembershipType` doesn't change (Inference).

## Receipts

- Buying a developer product hands the server's `ProcessReceipt` a
  receipt (Confirmed: Roblox's announcement and creator-docs), at once,
  before the prompt closes (Inference): `PurchaseId` (a GUID string),
  `PlayerId`, `ProductId`, `PlaceIdWherePurchased` (`game.PlaceId`),
  `CurrencySpent` (the product's `PriceInRobux`, 0 offline),
  `CurrencyType` (`Enum.CurrencyType.Robux`) and
  `ProductPurchaseChannel` (`Enum.ProductPurchaseChannel.InExperience`).
  Each call gets its own copy.
- The callback runs in a thread of its own and may yield as long as it
  likes. Returning `Enum.ProductPurchaseDecision.PurchaseGranted`
  resolves the receipt; anything else (`NotProcessedYet`) leaves it
  unresolved (Confirmed: creator-docs). A callback that errors leaves it
  unresolved too, its error reported as a script error (Inference).
- With no `ProcessReceipt` set, the receipt is acknowledged as granted
  (Confirmed: creator-docs).
- There is no retry on a timer: unresolved receipts are handed over
  again, all of them, when the player buys another developer product,
  or when a player with that `UserId` joins the server (Confirmed:
  creator-docs "Retries and Timeouts"). Only while the player is in the
  server; a receipt whose callback hasn't returned isn't handed over a
  second time meanwhile (Inference).
- A purchase counts only for the prompt the server has open for that
  player, once, and its Finished event says it was bought only then.
  A client that says it bought something it wasn't prompted for, says so
  twice, or answers another player's prompt changes nothing (Inference:
  Roblox's prompts are its own servers' to settle, which no client can
  answer for).

## Messages between server and client

A server's prompt reaches the client, and a client's request for a
prompt, its purchase and the prompt's closing reach the server, as
firings of MarketplaceService itself, which scripts never see, as
TextChatService's chat messages do ([chat](../gui/chat.md)). The client
tells the server a prompt closed before firing its own Finished event, so
a prompt a listener asks for next isn't dropped as the old one's.

## What isn't here

- `BindReceiptHandler`, `PromptBulkPurchase`, `PromptRobloxSubscriptionPurchase`,
  `PromptVIPServerPurchase`, `PromptCancelSubscription`, `OpenShop`,
  `GetUserSubscriptionDetailsAsync`, `GetUserSubscriptionPaymentHistoryAsync`
  and the other members raise "not implemented in Rook yet".
- Roblox's own prompt from its CoreScripts (`PurchasePromptApp`), which
  listens to the `Prompt*Requested` events Roblox's code alone sees.

## Studio questions

- The exact error texts for a bad player and an unknown id, and whether
  a client prompting another player errors.
- Whether a pass or bundle bought in a playtest is owned afterwards, and
  what `GetProductInfo` answers for one in Studio.
- Whether Studio drops or queues a prompt asked for while one is open,
  and what an owned item's prompt fires.
- Whether Studio's Premium prompt mock-buys Premium.
- The order of a prompt's Finished event on the client and the server.
