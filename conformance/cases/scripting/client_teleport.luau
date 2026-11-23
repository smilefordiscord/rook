-- TeleportService and Kick seen from the playtest client (teleports.md,
-- docs/behavior/characters/leaving.md). No call here can teleport or kick.
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


-- TeleportService.
log("GetJoinData", caught(player.GetJoinData, player))
log("GetLocalPlayerTeleportData", caught(TeleportService.GetLocalPlayerTeleportData, TeleportService))
log("GetArrivingTeleportGui", caught(TeleportService.GetArrivingTeleportGui, TeleportService))
local gui = Instance.new("ScreenGui")
log("SetTeleportGui ScreenGui", caught(TeleportService.SetTeleportGui, TeleportService, gui))
log("SetTeleportGui nil", caught(TeleportService.SetTeleportGui, TeleportService, nil))
log("SetTeleportGui Part", caught(TeleportService.SetTeleportGui, TeleportService, Instance.new("Part")))
log("SetTeleportGui Frame", caught(TeleportService.SetTeleportGui, TeleportService, Instance.new("Frame")))
log("SetTeleportGui string", caught(TeleportService.SetTeleportGui, TeleportService, "x"))
gui:Destroy()
log("SetTeleportSetting", caught(TeleportService.SetTeleportSetting, TeleportService, "conformance", 5))
log("GetTeleportSetting", caught(TeleportService.GetTeleportSetting, TeleportService, "conformance"))
log("SetTeleportSetting nil", caught(TeleportService.SetTeleportSetting, TeleportService, "conformance", nil))
log("TeleportAsync -1", within(function() return TeleportService:TeleportAsync(-1, { player }) end))
log("TeleportAsync empty", within(function() return TeleportService:TeleportAsync(0, {}) end))
log("TeleportAsync Workspace in list", within(function() return TeleportService:TeleportAsync(0, { workspace }) end))
log("TeleportPartyAsync empty", within(function() return TeleportService:TeleportPartyAsync(0, {}) end))
log("TeleportToPrivateServer empty", within(function() return TeleportService:TeleportToPrivateServer(0, "x", {}) end))
log("ReserveServerAsync word", within(function() return TeleportService:ReserveServerAsync("abc") end))
log("GetPlayerPlaceInstanceAsync word", within(function() return TeleportService:GetPlayerPlaceInstanceAsync("abc") end))
log("Teleport Workspace", within(function() return TeleportService:Teleport(0, workspace) end))
log("Teleport word place", within(function() return TeleportService:Teleport("abc") end))
do
    local options = Instance.new("TeleportOptions")
    options:SetTeleportData({ a = 1 })
    log("TeleportOptions on client", options.ShouldReserveServer, show(options:GetTeleportData()))
    options:Destroy()
end

-- Kick, on objects that aren't the local player.
log("Kick on Workspace", caught(function() return player.Kick(workspace, "x") end))
log("Kick on nil", caught(function() return player.Kick(nil) end))

