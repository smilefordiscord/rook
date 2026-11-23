-- HttpService seen from the playtest client (docs/behavior/scripting/http-requests.md):
-- requests are server-only. Split from a combined client case.
--! client
local Players = game:GetService("Players")
local HttpService = game:GetService("HttpService")
local TeleportService = game:GetService("TeleportService")
local MarketplaceService = game:GetService("MarketplaceService")
local BadgeService = game:GetService("BadgeService")
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


-- HttpService.
local LOCAL = "http://127.0.0.1:1/"
log("HttpEnabled", HttpService.HttpEnabled)
log("GetAsync", within(function() return HttpService:GetAsync(LOCAL) end))
log("GetAsync no args", within(function() return HttpService:GetAsync() end))
log("PostAsync", within(function() return HttpService:PostAsync(LOCAL, "x") end))
log("RequestAsync", within(function() return HttpService:RequestAsync({ Url = LOCAL }) end))
log("RequestAsync string", within(function() return HttpService:RequestAsync("x") end))
log("GetAsync roblox.com", within(function() return HttpService:GetAsync("https://www.roblox.com/") end))
log("JSONEncode", caught(HttpService.JSONEncode, HttpService, { 1, 2 }))

