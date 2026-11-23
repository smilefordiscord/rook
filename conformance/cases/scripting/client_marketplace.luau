-- MarketplaceService seen from the playtest client (marketplace.md). No
-- call here can prompt.
--! client
local Players = game:GetService("Players")
local MarketplaceService = game:GetService("MarketplaceService")
local player = Players.LocalPlayer

local function show(v)
    if type(v) == "table" then
        local keys = {}
        for k, x in pairs(v) do
            keys[#keys + 1] = tostring(k) .. "=" .. show(x)
        end
        table.sort(keys)
        return "{" .. table.concat(keys, ", ") .. "}"
    end
    return typeof(v) == "Instance" and v.ClassName or tostring(v)
end
local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = show(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end
local function within(f)
    local result
    task.spawn(function()
        result = table.pack(caught(f))
    end)
    for _ = 1, 30 do
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

-- MarketplaceService.
log("ProcessReceipt set", caught(function() MarketplaceService.ProcessReceipt = function() end end))
log("ProcessReceipt read", caught(function() return MarketplaceService.ProcessReceipt end))
log("ProcessReceipt unset", caught(function() MarketplaceService.ProcessReceipt = nil end))
log("PromptPurchase Workspace", caught(MarketplaceService.PromptPurchase, MarketplaceService, workspace, 1))
log("PromptPurchase nil", caught(MarketplaceService.PromptPurchase, MarketplaceService, nil, 1))
log("PromptGamePassPurchase Workspace", caught(MarketplaceService.PromptGamePassPurchase, MarketplaceService, workspace, 1))
log("PromptProductPurchase word id", caught(MarketplaceService.PromptProductPurchase, MarketplaceService, player, "abc"))
log("UserOwnsGamePassAsync word user", within(function() return MarketplaceService:UserOwnsGamePassAsync("abc", 1) end))
log("GetSubscriptionProductInfoAsync table", within(function() return MarketplaceService:GetSubscriptionProductInfoAsync({}) end))
