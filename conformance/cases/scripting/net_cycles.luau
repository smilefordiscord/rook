-- Tables that hold themselves, sent through remotes and bindables
-- (docs/behavior/replication.md#remote-arguments,
-- docs/behavior/scripting/bindables.md): which call raises, its message,
-- whether anything arrives, cycles through keys, dropped keys, metatables
-- and arrays, a table shared without a cycle, and very deep tables. Runs on
-- the server with the playtest's player; the remotes sit in
-- ReplicatedStorage and are destroyed at the end. Nothing listens on the
-- client, so an InvokeClient that sends would wait: those give up after 3 s
-- and log "pending".
--! player
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local player = Players:GetPlayers()[1]

local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = tostring(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end
-- `f` in a thread of its own, given 3 s to finish.
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

-- The tables.
local function selfField()
    local t = {}
    t.self = t
    return t
end
local function selfIndex()
    local t = { 1 }
    t[2] = t
    return t
end
local function twoStep()
    local a, b = {}, {}
    a.b = b
    b.a = a
    return a
end
local function deepCycle()
    local root = { x = { y = { z = {} } } }
    root.x.y.z.back = root
    return root
end
local function selfKey()
    local t = {}
    t[t] = 1
    return t
end
local function droppedKey()
    -- An array: `x` is dropped from the copy, and it's the only way back.
    local t = { 1, 2 }
    t.x = t
    return t
end
local function behindMetatable()
    local t = setmetatable({}, {})
    getmetatable(t).__index = t
    return t
end
local function shared()
    local s = { 1 }
    return { s, s, { s } }
end
local function nest(n)
    local root = {}
    local at = root
    for _ = 2, n do
        local nxt = {}
        at.n = nxt
        at = nxt
    end
    at.leaf = true
    return root
end
-- How many tables deep a received copy goes, and whether its leaf arrived.
local function depth(t)
    local n = 0
    while type(t) == "table" do
        n += 1
        if t.leaf then
            return n .. " leaf"
        end
        t = t.n
    end
    return n .. " cut"
end
local tables = {
    { "self field", selfField },
    { "self index", selfIndex },
    { "two step", twoStep },
    { "deep cycle", deepCycle },
    { "self key", selfKey },
    { "dropped key", droppedKey },
    { "behind metatable", behindMetatable },
    { "shared", shared },
}

-- Remotes, from the server.
local event = Instance.new("RemoteEvent")
event.Name = "ConformanceCycles"
event.Parent = ReplicatedStorage
local unreliable = Instance.new("UnreliableRemoteEvent")
unreliable.Name = "ConformanceCyclesUnreliable"
unreliable.Parent = ReplicatedStorage
local fn = Instance.new("RemoteFunction")
fn.Name = "ConformanceCyclesFunction"
fn.Parent = ReplicatedStorage

for _, case in tables do
    local name, make = case[1], case[2]
    log("FireClient " .. name, caught(event.FireClient, event, player, make()))
    log("FireAllClients " .. name, caught(event.FireAllClients, event, make()))
end
log("FireClient cycle second argument", caught(event.FireClient, event, player, 1, selfField()))
log("FireClient cycle in array", caught(event.FireClient, event, player, { 1, { selfField() } }))
log("unreliable FireAllClients", caught(unreliable.FireAllClients, unreliable, selfField()))
log("unreliable FireClient", caught(unreliable.FireClient, unreliable, player, selfField()))
log("InvokeClient cycle", within(function() return fn:InvokeClient(player, selfField()) end))
log("InvokeClient dropped key", within(function() return fn:InvokeClient(player, droppedKey()) end))
log("FireServer on server", caught(event.FireServer, event, selfField()))
log("InvokeServer on server", caught(fn.InvokeServer, fn, selfField()))
for _, n in { 60, 70, 300, 1000 } do
    log("FireAllClients depth " .. n, caught(event.FireAllClients, event, nest(n)))
end

-- An unparented remote.
do
    local loose = Instance.new("RemoteEvent")
    log("unparented FireAllClients", caught(loose.FireAllClients, loose, selfField()))
    log("unparented FireAllClients plain", caught(loose.FireAllClients, loose, { 1 }))
    loose:Destroy()
end

-- Bindables, for the same tables.
local bindable = Instance.new("BindableEvent")
local bfn = Instance.new("BindableFunction")
local function fire(...)
    local got
    local c = bindable.Event:Connect(function(...)
        got = table.pack(...)
    end)
    local ok, e = pcall(bindable.Fire, bindable, ...)
    task.wait()
    c:Disconnect()
    if not ok then
        return "error " .. (tostring(e):gsub("^[^:]*:%d+: ", "")) .. (got and " (arrived)" or "")
    end
    return got
end
local function describe(got)
    if type(got) == "string" then
        return got
    end
    local t = got[1]
    local keys = {}
    for k, v in pairs(t) do
        keys[#keys + 1] = typeof(k) .. ":" .. (tostring(k):gsub("0x%x+", "0x")) .. "=" .. typeof(v)
    end
    table.sort(keys)
    return "ok " .. table.concat(keys, " ")
end
for _, case in tables do
    local name, make = case[1], case[2]
    log("Fire " .. name, describe(fire(make())))
    log("Invoke " .. name, caught(function()
        bfn.OnInvoke = function(t)
            return typeof(t)
        end
        return bfn:Invoke(make())
    end))
end
do
    local got = fire(shared())
    log("Fire shared copies", if type(got) == "table" then got[1][1] == got[1][2] else got)
    bfn.OnInvoke = function()
        return selfField()
    end
    log("Invoke returns cycle", caught(bfn.Invoke, bfn))
    bfn.OnInvoke = function()
        return droppedKey()
    end
    log("Invoke returns dropped key", caught(function()
        local t = bfn:Invoke()
        return #t, t.x
    end))
    bfn.OnInvoke = function(...)
        return select("#", ...)
    end
    log("Invoke cycle second argument", caught(bfn.Invoke, bfn, 1, selfField()))
end
-- When it raises: before any listener runs, deferred or not.
do
    local ran = false
    local c = bindable.Event:Connect(function()
        ran = true
    end)
    local ok = pcall(bindable.Fire, bindable, selfField())
    local before = ran
    task.wait()
    c:Disconnect()
    log("Fire cycle listener ran", ok, before, ran)
end
for _, n in { 60, 70, 300, 1000 } do
    local got = fire(nest(n))
    log("Fire depth " .. n, type(got) == "table" and depth(got[1]) or got)
end

event:Destroy()
unreliable:Destroy()
fn:Destroy()
bindable:Destroy()
bfn:Destroy()
