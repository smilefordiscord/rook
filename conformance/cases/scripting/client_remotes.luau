-- A cyclic table sent to the server from the playtest client
-- (docs/behavior/replication.md#remote-arguments).
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


-- A cyclic table to the server, through a remote only this client has.
do
    local t = {}
    t.self = t
    local event = Instance.new("RemoteEvent")
    local fn = Instance.new("RemoteFunction")
    log("FireServer cycle", caught(event.FireServer, event, t))
    log("FireServer plain", caught(event.FireServer, event, { 1 }))
    log("InvokeServer cycle", within(function() return fn:InvokeServer(t) end))
    log("FireClient on client", caught(event.FireClient, event, player, 1))
    log("FireAllClients on client", caught(event.FireAllClients, event, 1))
    log("InvokeClient on client", within(function() return fn:InvokeClient(player, 1) end))
    event:Destroy()
    fn:Destroy()
end
log("player still here", player.Parent == Players)
