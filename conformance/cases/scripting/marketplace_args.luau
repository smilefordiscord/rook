-- MarketplaceService's arguments and callback, without opening a prompt or
-- asking Roblox's web services anything (docs/behavior/scripting/marketplace.md):
-- what each Prompt*Purchase raises for a player that isn't one, and for an
-- id that can't be cast (a cast fails before the call runs, so no prompt
-- opens); ownership and product-info calls with bad arguments; how
-- ProcessReceipt takes and refuses values; and the marketplace enums.
--! player
local MarketplaceService = game:GetService("MarketplaceService")
local player = game:GetService("Players"):GetPlayers()[1]

local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = if type(r[i]) == "table" then "table" else tostring(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end
-- `f` in a thread of its own, given 5 s to finish.
local function within(f)
    local result
    task.spawn(function()
        result = table.pack(caught(f))
    end)
    for _ = 1, 50 do
        if result then
            break
        end
        task.wait(0.1)
    end
    if not result then
        return "pending"
    end
    return table.unpack(result, 1, result.n)
end
local function call(name, ...)
    local args = table.pack(...)
    return within(function()
        return MarketplaceService[name](MarketplaceService, table.unpack(args, 1, args.n))
    end)
end

-- Enums.
local function items(enum)
    local list = {}
    for _, item in enum:GetEnumItems() do
        list[#list + 1] = item.Value .. "=" .. item.Name
    end
    table.sort(list, function(a, b)
        return tonumber(a:match("^%-?%d+")) < tonumber(b:match("^%-?%d+"))
    end)
    return table.concat(list, " ")
end
log("ProductPurchaseDecision", items(Enum.ProductPurchaseDecision))
log("InfoType", items(Enum.InfoType))
log("CurrencyType", items(Enum.CurrencyType))
log("ProductPurchaseChannel", items(Enum.ProductPurchaseChannel))
log("SubscriptionPeriod", items(Enum.SubscriptionPeriod))
log("MembershipType", items(Enum.MembershipType))

-- ProcessReceipt.
local function receipt(value)
    return caught(function()
        MarketplaceService.ProcessReceipt = value
    end)
end
log("read ProcessReceipt", caught(function() return MarketplaceService.ProcessReceipt end))
log("set function", receipt(function() return Enum.ProductPurchaseDecision.PurchaseGranted end))
log("read after set", caught(function() return MarketplaceService.ProcessReceipt end))
log("set again", receipt(function() end))
log("set C function", receipt(print))
log("set nil", receipt(nil))
log("set number", receipt(5))
log("set string", receipt("x"))
log("set table", receipt({}))
log("set callable table", receipt(setmetatable({}, { __call = function() end })))
log("set lowercase", caught(function() MarketplaceService.processReceipt = function() end end))
log("rawget", caught(function() return rawget(MarketplaceService, "ProcessReceipt") end))
log("unset", receipt(nil))
log("ProcessReceipt on Workspace", caught(function() workspace.ProcessReceipt = function() end end))

-- Prompts with a player that isn't one: nothing can open.
local prompts = {
    { "PromptPurchase", 1 },
    { "PromptGamePassPurchase", 1 },
    { "PromptProductPurchase", 1 },
    { "PromptBundlePurchase", 1 },
    { "PromptSubscriptionPurchase", "EXP-0" },
    { "PromptPremiumPurchase" },
}
local notPlayers = {
    { "nil", nil },
    { "Workspace", workspace },
    { "Part", Instance.new("Part") },
    { "UserId", 1 },
    { "string", "x" },
    { "table", {} },
}
for _, prompt in prompts do
    for _, who in notPlayers do
        log(prompt[1] .. " " .. who[1], call(prompt[1], who[2], prompt[2]))
    end
end
log("PromptPurchase no args", call("PromptPurchase"))

-- Prompts for the player with an id that can't be cast.
for _, name in { "PromptPurchase", "PromptGamePassPurchase", "PromptProductPurchase", "PromptBundlePurchase" } do
    log(name .. " id word", call(name, player, "abc"))
    log(name .. " id table", call(name, player, {}))
end
log("PromptPurchase bad currency", call("PromptPurchase", player, "abc", true, "Nope"))
log("PromptProductPurchase equip table", call("PromptProductPurchase", player, {}, {}))
log("PromptSubscriptionPurchase id table", call("PromptSubscriptionPurchase", player, {}))

-- Ownership, with arguments that can't be asked about.
-- (Not PlayerOwnsAsset with an instance that isn't a Player: Studio's
-- call waits on Roblox's web services, past the 5 s.)
log("UserOwnsGamePassAsync no args", call("UserOwnsGamePassAsync"))
log("UserOwnsGamePassAsync nil user", call("UserOwnsGamePassAsync", nil, 1))
log("UserOwnsGamePassAsync word user", call("UserOwnsGamePassAsync", "abc", 1))
log("UserOwnsGamePassAsync table user", call("UserOwnsGamePassAsync", {}, 1))
log("UserOwnsGamePassAsync Workspace user", call("UserOwnsGamePassAsync", workspace, 1))
log("UserOwnsGamePassAsync nil pass", call("UserOwnsGamePassAsync", 1, nil))
log("UserOwnsGamePassAsync word pass", call("UserOwnsGamePassAsync", 1, "abc"))
log("PlayerOwnsAsset nil", call("PlayerOwnsAsset", nil, 1))
log("PlayerOwnsAsset UserId", call("PlayerOwnsAsset", 1, 1))
log("PlayerOwnsAsset word id", call("PlayerOwnsAsset", player, "abc"))
log("PlayerOwnsBundle Workspace", call("PlayerOwnsBundle", workspace, 1))
log("PlayerOwnsBundleAsync word id", call("PlayerOwnsBundleAsync", player, "abc"))
log("GetUserSubscriptionStatusAsync nil", call("GetUserSubscriptionStatusAsync", nil, "EXP-0"))
log("GetUserSubscriptionStatusAsync Workspace", call("GetUserSubscriptionStatusAsync", workspace, "EXP-0"))
log("GetUserSubscriptionStatusAsync table id", call("GetUserSubscriptionStatusAsync", player, {}))

-- Product info with ids that can't be cast, and an InfoType that isn't one.
log("GetProductInfo no args", call("GetProductInfo"))
log("GetProductInfo word", call("GetProductInfo", "abc"))
log("GetProductInfo table", call("GetProductInfo", {}))
log("GetProductInfo bad type name", call("GetProductInfo", 1, "Nope"))
log("GetProductInfo other enum", call("GetProductInfo", 1, Enum.Material.Wood))
log("GetProductInfo bad type number", call("GetProductInfo", 1, 99))
log("GetProductInfoAsync word", call("GetProductInfoAsync", "abc"))
log("GetProductInfoAsync other enum", call("GetProductInfoAsync", 1, Enum.Material.Wood))
log("GetSubscriptionProductInfoAsync table", call("GetSubscriptionProductInfoAsync", {}))
log("GetSubscriptionProductInfoAsync nil", call("GetSubscriptionProductInfoAsync", nil))

-- Members called on something else.
log("dot call", caught(function() return MarketplaceService.PromptPurchase(player, 1) end))
log("unknown member", caught(function() return MarketplaceService.PromptNothing end))
