-- MarketplaceService's purchases, made as Studio's test purchases: nothing
-- is charged, and what is bought is kept for the session only
-- (docs/behavior/scripting/marketplace.md).
--
-- The server keeps each player's open prompt and sends it to the player's
-- client as a firing of the service ("prompt"); a client prompting its own
-- player asks the server to ("ask"). The client asks the player
-- (`purchase_prompt`), tells the server when they buy ("bought"), and when
-- the prompt closes fires the prompt's Finished event and tells the server
-- ("finished"), which fires it there. The server takes those answers only
-- for the prompt it has open for that player, each once: a client can't
-- buy what it wasn't offered. A developer product bought becomes a receipt
-- the server hands to ProcessReceipt until it's granted.
local std, pcall = ...
local game, Enum = std.game, std.Enum
local ipairs, tostring, typeof, error = std.ipairs, std.tostring, std.typeof, std.error
local spawn, clone, remove, format = std.task.spawn, std.table.clone, std.table.remove, std.string.format
local fire, get_callback, send, is_client, products_page =
	std.fire, std.get_callback, std.service_send, std.is_client, std.developer_products_page
local prompt = std.purchase_prompt

local market = game:GetService("MarketplaceService")
local players = game:GetService("Players")
local http = game:GetService("HttpService")

-- What each kind of prompt sells, as its title names it.
local TITLES = {
	Asset = "Item",
	GamePass = "Pass",
	Product = "Developer product",
	Bundle = "Bundle",
	Subscription = "Subscription",
	Premium = "Roblox Premium",
}

local api = {}

-- What this session bought: passes, assets, bundles and subscriptions, by
-- kind, user and id. Developer products are bought again and again, not
-- owned (creator-docs PlayerOwnsAssetAsync).
local owned = {}

local function key(kind, user, id)
	return kind .. " " .. tostring(user) .. " " .. tostring(id)
end

function api.owns(kind, user, id)
	return owned[key(kind, user, id)] == true
end

-- Keep `id` as `user`'s; whether they didn't have it.
local function own(kind, user, id)
	if kind == "Product" or kind == "Premium" then
		return false
	end
	local k = key(kind, user, id)
	local new = not owned[k]
	owned[k] = true
	return new
end

-- The prompt's Finished event, with creator-docs' arguments for each.
local function finished(player, kind, id, purchased)
	if kind == "Asset" then
		fire(market, "PromptPurchaseFinished", player, id, purchased)
	elseif kind == "GamePass" then
		fire(market, "PromptGamePassPurchaseFinished", player, id, purchased)
	elseif kind == "Product" then
		fire(market, "PromptProductPurchaseFinished", player.UserId, id, purchased)
	elseif kind == "Bundle" then
		fire(market, "PromptBundlePurchaseFinished", player, id, purchased)
	elseif kind == "Subscription" then
		fire(market, "PromptSubscriptionPurchaseFinished", player, id, purchased)
	else
		fire(market, "PromptPremiumPurchaseFinished")
	end
end

-- Server: receipts not yet granted, by user id, in purchase order, and
-- the purchase ids whose callback is still running.
local pending = {}
local running = {}

-- Whether the game granted `receipt`. With no ProcessReceipt the receipt
-- is acknowledged (Confirmed: creator-docs ProcessReceipt); a callback
-- that errors leaves it unresolved, its error reported as a script's
-- (Inference).
local function grant(receipt)
	local callback = get_callback(market, "ProcessReceipt")
	if not callback then
		return true
	end
	local ok, decision = pcall(callback, clone(receipt))
	if not ok then
		spawn(error, decision, 0)
		return false
	end
	return decision == Enum.ProductPurchaseDecision.PurchaseGranted
end

-- Hand each of `userId`'s unresolved receipts to the game, while they're
-- in the server. Confirmed (creator-docs ProcessReceipt): there is no
-- retry on a timer; an unresolved receipt is handed over again when the
-- user buys another developer product or joins.
local function process(userId)
	if not players:GetPlayerByUserId(userId) then
		return
	end
	for _, receipt in ipairs(clone(pending[userId] or {})) do
		local id = receipt.PurchaseId
		if not running[id] then
			running[id] = true
			spawn(function()
				local granted = grant(receipt)
				running[id] = nil
				local list = pending[userId]
				if granted and list then
					for i, r in ipairs(list) do
						if r == receipt then
							remove(list, i)
							break
						end
					end
				end
			end)
		end
	end
end

-- The receipt for `player` buying developer product `id` (creator-docs
-- ProcessReceipt's `receiptInfo`).
local function receipt(player, id)
	local ok, info = pcall(function()
		return market:GetProductInfo(id, Enum.InfoType.Product)
	end)
	return {
		PurchaseId = http:GenerateGUID(false),
		PlayerId = player.UserId,
		ProductId = id,
		PlaceIdWherePurchased = game.PlaceId,
		CurrencySpent = ok and info.PriceInRobux or 0,
		CurrencyType = Enum.CurrencyType.Robux,
		ProductPurchaseChannel = Enum.ProductPurchaseChannel.InExperience,
	}
end

-- Server: each player's open prompt, by Player: what it sells, whether
-- they bought it, and whether that subscribed them, which Players
-- announces when it closes (creator-docs
-- Players.UserSubscriptionStatusChanged).
local prompts = {}

-- Whether `id` is what a prompt for `kind` takes, as the Prompt members
-- check it.
local function valid(kind, id)
	if typeof(kind) ~= "string" or TITLES[kind] == nil then
		return false
	elseif kind == "Premium" then
		return id == nil
	elseif kind == "Subscription" then
		return typeof(id) == "string"
	end
	return typeof(id) == "number"
end

-- Server: prompt `player` for `kind` `id`. A player sees one prompt at a
-- time; one asked for while another is open is dropped (Confirmed for
-- PromptVIPServerPurchase by creator-docs; Inference for the rest).
local function open(player, kind, id)
	if prompts[player] then
		return
	end
	prompts[player] = { kind = kind, id = id, bought = false, subscribed = false }
	send(market, player, "prompt", kind, id)
end

function api.onServer(player, what, kind, id)
	if not valid(kind, id) then
		return
	end
	if what == "ask" then
		open(player, kind, id)
		return
	end
	local current = prompts[player]
	if not current or current.kind ~= kind or current.id ~= id then
		return
	end
	if what == "bought" and not current.bought then
		current.bought = true
		local changed = own(kind, player.UserId, id)
		if kind == "Subscription" then
			current.subscribed = changed
		elseif kind == "Product" then
			local list = pending[player.UserId] or {}
			pending[player.UserId] = list
			list[#list + 1] = receipt(player, id)
			process(player.UserId)
		end
	elseif what == "finished" then
		prompts[player] = nil
		finished(player, kind, id, current.bought)
		if current.subscribed then
			fire(players, "UserSubscriptionStatusChanged", player, id)
		end
	end
end

players.PlayerAdded:Connect(function(player)
	if not is_client() then
		process(player.UserId)
	end
end)

players.PlayerRemoving:Connect(function(player)
	prompts[player] = nil
end)

-- Client: what the prompt shows for `kind` `id`; raises what
-- GetProductInfo does.
local function describe(kind, id)
	if kind == "Premium" then
		return { Name = "Roblox Premium" }
	elseif kind == "Subscription" then
		return market:GetSubscriptionProductInfoAsync(id)
	end
	return market:GetProductInfo(id, Enum.InfoType[kind])
end

-- Client: whether the local player has `kind` `id` already, as the
-- ownership members say.
local function has(kind, player, id)
	local ok, result = pcall(function()
		if kind == "Asset" then
			return market:PlayerOwnsAsset(player, id)
		elseif kind == "GamePass" then
			return market:UserOwnsGamePassAsync(player.UserId, id)
		elseif kind == "Bundle" then
			return market:PlayerOwnsBundle(player, id)
		elseif kind == "Subscription" then
			return market:GetUserSubscriptionStatusAsync(player, id).IsSubscribed
		end
		return false
	end)
	return ok and result == true
end

local function offer(kind, info)
	if kind == "Premium" then
		return "Subscribe to Roblox Premium?"
	elseif kind == "Subscription" then
		if info.DisplayPrice ~= nil and info.DisplayPrice ~= "" then
			return "Subscribe for " .. info.DisplayPrice .. (info.DisplaySubscriptionPeriod or "") .. "?"
		end
		return "Subscribe?"
	end
	return format("Buy this %s for %d Robux?", TITLES[kind]:lower(), info.PriceInRobux or 0)
end

-- Client: ask the local player, then finish the prompt. The prompt
-- finishes and closes even when asking raises (an item whose web answer
-- isn't the shape the prompt shows), so one bad item can't keep every
-- later prompt from opening.
local function run(kind, id)
	local player = players.LocalPlayer
	local purchased = false
	local asked, problem = pcall(function()
		local ok, info = pcall(describe, kind, id)
		local name = ok and typeof(info) == "table" and info.Name or ""
		local heading = if typeof(name) == "string" and name ~= ""
			then name
			elseif id ~= nil then TITLES[kind] .. " " .. tostring(id)
			else TITLES[kind]
		if not ok then
			prompt.ask(heading, "This item isn't available: " .. tostring(info), { "OK" })
		elseif info.IsForSale == false then
			prompt.ask(heading, "This item isn't for sale.", { "OK" })
		elseif has(kind, player, id) then
			prompt.ask(heading, "You already have this.", { "OK" })
		elseif prompt.ask(heading, offer(kind, info), { "Buy", "Cancel" }) == "Buy" then
			purchased = true
			own(kind, player.UserId, id)
			send(market, "server", "bought", kind, id)
			prompt.ask(heading, "Your purchase succeeded.", { "OK" })
		end
	end)
	prompt.close()
	-- The server hears first, so a prompt a Finished listener asks for
	-- finds this one closed there.
	send(market, "server", "finished", kind, id)
	finished(player, kind, id, purchased)
	if not asked then
		error(problem, 0)
	end
end

-- `Prompt*Purchase(player, id)`, its arguments checked.
function api.prompt(kind, player, id)
	if not is_client() then
		open(player, kind, id)
		return
	end
	-- Inference: a client prompting another player does nothing.
	if player == players.LocalPlayer then
		send(market, "server", "ask", kind, id)
	end
end

function api.onClient(what, kind, id)
	if what == "prompt" and valid(kind, id) then
		spawn(run, kind, id)
	end
end

-- `GetDeveloperProductsAsync`: pages of the experience's developer
-- products, each fetched as the script advances to it.
function api.developerProducts()
	local number = 1
	local entries, last = products_page(number)
	local pages = { IsFinished = last }
	function pages:GetCurrentPage()
		return clone(entries)
	end
	function pages:AdvanceToNextPageAsync()
		if pages.IsFinished then
			-- Inference: the error past the last page.
			error("No pages to advance to", 2)
		end
		number += 1
		entries, last = products_page(number)
		pages.IsFinished = last
	end
	return pages
end

return api
