-- TeleportService and Player:Kick on the server, without kicking anyone or
-- leaving (docs/behavior/scripting/teleports.md,
-- docs/behavior/characters/leaving.md): TeleportOptions' defaults and what
-- its properties and teleport data take, what each teleport method raises
-- for bad arguments, SetTeleportGui's types, the client-only members called
-- on a server, the playtest player's GetJoinData, and Kick called on
-- something that isn't a Player. One real TeleportAsync goes to place 0,
-- which can't exist, to log how it fails (Studio refuses teleports from an
-- unpublished place); TeleportInitFailed is logged until 1 s after it.
--! player
-- Addresses differ between runs; print them as <address>.
local rawlog = log
local function log(...)
    local t = table.pack(...)
    for i = 1, t.n do
        t[i] = (tostring(t[i]):gsub("0x%x+", "<address>"))
    end
    rawlog(table.unpack(t, 1, t.n))
end
local TeleportService = game:GetService("TeleportService")
local Players = game:GetService("Players")
local player = Players:GetPlayers()[1]

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
local function tp(name, ...)
    local args = table.pack(...)
    return within(function()
        return TeleportService[name](TeleportService, table.unpack(args, 1, args.n))
    end)
end

-- TeleportOptions.
local options = Instance.new("TeleportOptions")
log("options", options.Name, ("%q"):format(options.ReservedServerAccessCode), ("%q"):format(options.ServerInstanceId), options.ShouldReserveServer)
log("options data", caught(options.GetTeleportData, options))
log("VipServerId", caught(function() return options.VipServerId end))
log("ReservedServerId", caught(function() return options.ReservedServerId end))
local function set(key, value)
    return caught(function()
        options[key] = value
        return options[key]
    end)
end
log("ServerInstanceId number", set("ServerInstanceId", 5))
log("ServerInstanceId nil", set("ServerInstanceId", nil))
log("ServerInstanceId table", set("ServerInstanceId", {}))
log("ShouldReserveServer string", set("ShouldReserveServer", "x"))
log("ShouldReserveServer nil", set("ShouldReserveServer", nil))
log("ReservedServerAccessCode number", set("ReservedServerAccessCode", 5))
options.ServerInstanceId = ""
options.ShouldReserveServer = false
options.ReservedServerAccessCode = ""

-- Teleport data.
local function data(v)
    return caught(function()
        options:SetTeleportData(v)
        local got = options:GetTeleportData()
        return got, rawequal(got, v)
    end)
end
log("data table", data({ a = 1, b = { 2, 3 } }))
log("data string", data("x"))
log("data number", data(5))
log("data bool", data(true))
log("data nil", data(nil))
log("data Vector3", data(Vector3.new(1, 2, 3)))
log("data Instance", data(workspace))
log("data Instance in table", data({ part = workspace }))
log("data function", data(print))
log("data function in table", data({ f = print }))
log("data mixed table", data({ 1, 2, x = 3 }))
log("data bool key", data({ [true] = 1 }))
do
    local t = {}
    t.self = t
    log("data cycle", data(t))
    local back = { 1 }
    local copy = (function()
        options:SetTeleportData(back)
        return options:GetTeleportData()
    end)()
    back[1] = 2
    log("data copied when set", copy == back, copy[1])
    log("data copied when read", options:GetTeleportData() == options:GetTeleportData())
end
log("SetTeleportData no args", caught(function() options:SetTeleportData() return options:GetTeleportData() end))
log("SetTeleportData nil", caught(function() options:SetTeleportData(nil) return options:GetTeleportData() end))

-- TeleportAsync's arguments.
log("TeleportAsync no args", tp("TeleportAsync"))
log("TeleportAsync -1", tp("TeleportAsync", -1, { player }))
log("TeleportAsync word place", tp("TeleportAsync", "abc", { player }))
log("TeleportAsync empty", tp("TeleportAsync", 0, {}))
log("TeleportAsync no players", tp("TeleportAsync", 0))
log("TeleportAsync string players", tp("TeleportAsync", 0, "x"))
log("TeleportAsync Player not in table", tp("TeleportAsync", 0, player))
log("TeleportAsync Workspace in list", tp("TeleportAsync", 0, { workspace }))
log("TeleportAsync number in list", tp("TeleportAsync", 0, { player, 5 }))
log("TeleportAsync dictionary", tp("TeleportAsync", 0, { p = player }))
log("TeleportAsync Part options", tp("TeleportAsync", 0, { player }, Instance.new("Part")))
log("TeleportAsync string options", tp("TeleportAsync", 0, { player }, "x"))
do
    local many = table.create(51, player)
    log("TeleportAsync 51", tp("TeleportAsync", 0, many))
    local both = Instance.new("TeleportOptions")
    both.ShouldReserveServer = true
    both.ServerInstanceId = "x"
    log("TeleportAsync reserve and instance", tp("TeleportAsync", 0, { player }, both))
    local code = Instance.new("TeleportOptions")
    code.ShouldReserveServer = true
    code.ReservedServerAccessCode = "x"
    log("TeleportAsync reserve and code", tp("TeleportAsync", 0, { player }, code))
    both:Destroy()
    code:Destroy()
end

-- One real attempt, to a place that can't exist.
do
    local failed = {}
    local c = TeleportService.TeleportInitFailed:Connect(function(who, result, message, placeId, opts)
        table.insert(failed, table.concat({ show(who), tostring(result), tostring(message), tostring(placeId), show(opts) }, " | "))
    end)
    log("TeleportAsync place 0", tp("TeleportAsync", 0, { player }))
    task.wait(1)
    c:Disconnect()
    log("TeleportInitFailed", #failed, failed[1])
    log("player stayed", player.Parent == Players)
end

-- The legacy methods, with a player that isn't one.
log("Teleport Workspace", tp("Teleport", 0, workspace))
log("Teleport string player", tp("Teleport", 0, "x"))
log("Teleport no player", tp("Teleport", 0))
log("Teleport word place", tp("Teleport", "abc", player))
log("TeleportToSpawnByName Workspace", tp("TeleportToSpawnByName", 0, "x", workspace))
log("TeleportToPlaceInstance Workspace", tp("TeleportToPlaceInstance", 0, "x", workspace))
log("TeleportToPrivateServer empty", tp("TeleportToPrivateServer", 0, "x", {}))
log("TeleportToPrivateServer Workspace", tp("TeleportToPrivateServer", 0, "x", { workspace }))
log("TeleportPartyAsync empty", tp("TeleportPartyAsync", 0, {}))
log("TeleportPartyAsync Workspace", tp("TeleportPartyAsync", 0, { workspace }))
log("ReserveServerAsync word", tp("ReserveServerAsync", "abc"))
log("ReserveServer no args", tp("ReserveServer"))
log("GetPlayerPlaceInstanceAsync word", tp("GetPlayerPlaceInstanceAsync", "abc"))
log("GetPlayerPlaceInstanceAsync nil", tp("GetPlayerPlaceInstanceAsync", nil))

-- Client members, called on the server.
log("GetLocalPlayerTeleportData", tp("GetLocalPlayerTeleportData"))
log("GetArrivingTeleportGui", tp("GetArrivingTeleportGui"))
local gui = Instance.new("ScreenGui")
log("SetTeleportGui ScreenGui", tp("SetTeleportGui", gui))
log("SetTeleportGui nil", tp("SetTeleportGui", nil))
log("SetTeleportGui no args", tp("SetTeleportGui"))
log("SetTeleportGui Part", tp("SetTeleportGui", Instance.new("Part")))
log("SetTeleportGui Frame", tp("SetTeleportGui", Instance.new("Frame")))
log("SetTeleportGui BillboardGui", tp("SetTeleportGui", Instance.new("BillboardGui")))
log("SetTeleportGui string", tp("SetTeleportGui", "x"))
log("SetTeleportGui number", tp("SetTeleportGui", 5))
log("GetArrivingTeleportGui after", tp("GetArrivingTeleportGui"))
tp("SetTeleportGui", nil)
gui:Destroy()
log("SetTeleportSetting", tp("SetTeleportSetting", "conformance", 5))
log("GetTeleportSetting", tp("GetTeleportSetting", "conformance"))
log("SetTeleportSetting table", tp("SetTeleportSetting", "conformance", { 1 }))
log("GetTeleportSetting table", tp("GetTeleportSetting", "conformance"))
log("SetTeleportSetting Instance", tp("SetTeleportSetting", "conformance", workspace))
log("GetTeleportSetting missing", tp("GetTeleportSetting", "conformance-missing"))
log("SetTeleportSetting nil key", tp("SetTeleportSetting", nil, 5))
tp("SetTeleportSetting", "conformance", nil)

-- Join data.
log("GetJoinData", caught(player.GetJoinData, player))
log("GetJoinData fresh table", player:GetJoinData() == player:GetJoinData())
log("GetJoinData on Workspace", caught(function() return player.GetJoinData(workspace) end))

-- Kick, called on things that aren't a Player: it can't kick anyone.
log("Kick on Workspace", caught(function() return player.Kick(workspace, "x") end))
log("Kick on nil", caught(function() return player.Kick(nil, "x") end))
log("Kick on Players", caught(function() return player.Kick(Players, "x") end))
log("Kick member of Workspace", caught(function() return workspace.Kick end))
log("Kick is a function", typeof(player.Kick))
log("player still here", player.Parent == Players)
options:Destroy()
