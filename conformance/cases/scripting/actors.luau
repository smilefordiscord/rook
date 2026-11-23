-- Actors from outside and inside. Outside (this case runs in no Actor):
-- task.desynchronize/synchronize, GetActor, and the argument errors of
-- SendMessage, BindToMessage, BindToMessageParallel and ConnectParallel.
-- Inside: a Script made at runtime under an Actor (its Source set by this
-- case, which Studio's MCP allows at plugin identity) binds topics and
-- logs what reaches it, in order: messages and their copies, bindings'
-- order, unbound topics, late bindings and disconnects, and the Actor's
-- own globals and module copies. The Actor script reports its lines back
-- through a BindableEvent once this case sends "Report".
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

-- Outside any Actor.
log("desynchronize outside", caught(task.desynchronize))
log("synchronize outside", caught(task.synchronize))
log("task.synchronize value", type(task.synchronize), type(task.desynchronize))

local actor = Instance.new("Actor")
log("actor", actor.ClassName, actor.Name, actor:IsA("Model"), actor:IsA("PVInstance"))
local part = Instance.new("Part")
log("GetActor unparented", part:GetActor())
part.Parent = actor
log("GetActor child", part:GetActor() == actor)
log("GetActor self", actor:GetActor() == actor)
local model = Instance.new("Model")
model.Parent = actor
local innerActor = Instance.new("Actor")
innerActor.Parent = model
local deepPart = Instance.new("Part")
deepPart.Parent = innerActor
local besidePart = Instance.new("Part")
besidePart.Parent = model
log("GetActor nested", deepPart:GetActor() == innerActor, innerActor:GetActor() == innerActor, besidePart:GetActor() == actor)
log("GetActor service", game:GetService("ServerScriptService"):GetActor(), game:GetActor())
log("GetActor dot", caught(part.GetActor))
log("GetActor extra arg", caught(function() return part:GetActor(1) == actor end))
log("GetActor moved", caught(function()
    deepPart.Parent = nil
    return deepPart:GetActor()
end))

log("SendMessage no topic", caught(function() actor:SendMessage() end))
log("SendMessage nil topic", caught(function() actor:SendMessage(nil) end))
log("SendMessage number topic", caught(function() actor:SendMessage(5) end))
log("SendMessage table topic", caught(function() actor:SendMessage({}) end))
log("SendMessage empty topic", caught(function() actor:SendMessage("") end))
log("SendMessage returns", caught(function() return select("#", actor:SendMessage("Nobody")) end))
log("SendMessage function", caught(function() actor:SendMessage("Nobody", print) end))
log("SendMessage Luau function", caught(function() actor:SendMessage("Nobody", function() end) end))
log("SendMessage nested function", caught(function() actor:SendMessage("Nobody", { f = print }) end))
log("SendMessage thread", caught(function() actor:SendMessage("Nobody", coroutine.create(print)) end))
log("SendMessage Instance", caught(function() actor:SendMessage("Nobody", workspace) end))
log("SendMessage cycle", caught(function()
    local c = {}
    c.c = c
    actor:SendMessage("Nobody", c)
end))
log("SendMessage dot", caught(function() actor.SendMessage("Nobody") end))
log("SendMessage on Part", caught(function() part:SendMessage("x") end))

log("BindToMessage outside", caught(function() actor:BindToMessage("x", function() end) end))
log("BindToMessageParallel outside", caught(function() actor:BindToMessageParallel("x", function() end) end))
log("BindToMessage no function", caught(function() actor:BindToMessage("x") end))
log("BindToMessage number", caught(function() actor:BindToMessage("x", 5) end))
log("BindToMessage no topic", caught(function() actor:BindToMessage(nil, function() end) end))
log("BindToMessage builtin", caught(function() actor:BindToMessage("x", print) end))

local bindable = Instance.new("BindableEvent")
log("ConnectParallel outside", caught(function() bindable.Event:ConnectParallel(function() end) end))
log("ConnectParallel no function", caught(function() bindable.Event:ConnectParallel() end))
log("ConnectParallel builtin", caught(function() bindable.Event:ConnectParallel(print) end))
log("ConnectParallel type", typeof(bindable.Event.ConnectParallel))
bindable:Destroy()

-- A module under the Actor, required from here, runs in this case's
-- context.
local module = Instance.new("ModuleScript")
module.Name = "Module"
local sourceSet = pcall(function()
    module.Source = [[
local counter = { n = 0 }
function counter.where()
    return script:GetActor() ~= nil
end
function counter.desynchronize()
    local ok, e = pcall(task.desynchronize)
    return ok, type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or e
end
function counter.connect()
    local b = Instance.new("BindableEvent")
    local ok, e = pcall(function() b.Event:ConnectParallel(function() end) end)
    b:Destroy()
    return ok, e
end
return counter
]]
end)
log("Source settable", sourceSet)
if not sourceSet then
    actor:Destroy()
    return
end
module.Parent = actor

local worker = Instance.new("Script")
worker.Name = "Worker"
worker.Source = [==[
local lines = {}
local function rec(...)
    local n = select("#", ...)
    local parts = table.create(n)
    for i = 1, n do
        parts[i] = tostring((select(i, ...)))
    end
    table.insert(lines, table.concat(parts, " "))
end
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
local function describe(v)
    local t = typeof(v)
    if t == "table" then
        local keys = {}
        for k, x in v do
            table.insert(keys, tostring(k) .. "=" .. describe(x))
        end
        table.sort(keys)
        return "{" .. table.concat(keys, ",") .. "}"
    elseif t == "Instance" then
        return "Instance:" .. v.Name
    end
    return t .. ":" .. tostring(v)
end
local actor = script:GetActor()
local ok, err = pcall(function()
    rec("GetActor", actor == script.Parent, actor.Name)
    rec("globals", _G.RookConformanceMarker, shared.RookConformanceMarker, RookConformanceMarker)
    _G.RookConformanceActor = "actor"
    local m = require(script.Parent.Module)
    m.n += 1
    rec("module", m.n, m.where())
    rec("module connect", m.connect())

    local first = actor:BindToMessage("Order", function(...)
        rec("Order serial first", ...)
    end)
    rec("binding", typeof(first), first.Connected)
    actor:BindToMessage("Order", function(...)
        rec("Order serial second", ...)
    end)
    actor:BindToMessageParallel("Order", function(...)
        rec("Order parallel", ...)
    end)
    actor:BindToMessage("Copy", function(a, b)
        rec("Copy first", a.x, a == b, a.list == b.list, describe(a))
        a.x = "changed by first"
    end)
    actor:BindToMessage("Copy", function(a)
        rec("Copy second", a.x)
    end)
    actor:BindToMessage("Args", function(...)
        local n = select("#", ...)
        local parts = {}
        for i = 1, n do
            parts[i] = describe((select(i, ...)))
        end
        rec("Args", n, table.concat(parts, " "))
    end)
    actor:BindToMessage("Shared", function(st, name)
        rec("Shared", typeof(st), st == game:GetService("SharedTableRegistry"):GetSharedTable(name), st.v)
        st.v = "actor"
    end)
    actor:BindToMessage("Self", function(...)
        rec("Self", ...)
        actor:SendMessage("SelfReply", "from handler")
        rec("Self sent")
    end)
    actor:BindToMessage("SelfReply", function(...)
        rec("SelfReply", ...)
    end)
    actor:BindToMessage("BindLate", function()
        actor:BindToMessage("Late", function(v)
            rec("Late", v)
        end)
        rec("bound Late")
    end)
    local gone = actor:BindToMessage("Gone", function()
        rec("Gone ran")
    end)
    gone:Disconnect()
    rec("disconnected", gone.Connected)
    local dropping
    dropping = actor:BindToMessage("Drop", function(v)
        rec("Drop", v)
        dropping:Disconnect()
    end)
    actor:BindToMessage("Error", function(v)
        rec("Error handler", v)
        error("handler failed")
    end)
    actor:BindToMessage("Error", function(v)
        rec("Error other handler", v)
    end)
    actor:BindToMessage("Yield", function(v)
        rec("Yield before", v)
        task.wait()
        rec("Yield after", v)
    end)
    actor:BindToMessage("Report", function()
        script.Parent.Done:Fire(table.concat(lines, "\n"))
    end)
    rec("bind args", caught(function() actor:BindToMessage("x") end))
    rec("bind other actor", caught(function()
        local other = Instance.new("Actor")
        local r = table.pack(pcall(function() other:BindToMessage("x", function() end) end))
        other:Destroy()
        return r[1], r[2]
    end))
end)
if not ok then
    rec("ACTOR ERROR", err)
end
script.Parent.Ready:Fire(table.concat(lines, "\n"))
table.clear(lines)
]==]
worker.Parent = actor

local ready = Instance.new("BindableEvent")
ready.Name = "Ready"
ready.Parent = actor
local done = Instance.new("BindableEvent")
done.Name = "Done"
done.Parent = actor
local isReady, report = false, nil
ready.Event:Connect(function(text)
    isReady = true
    for _, line in text:split("\n") do
        log("actor:", line)
    end
end)
done.Event:Connect(function(text)
    report = text
end)

_G.RookConformanceMarker = "main"
shared.RookConformanceMarker = "main"
local mainCopy = require(module)
mainCopy.n += 10
log("main module", mainCopy.n, mainCopy.where())
log("main module desynchronize", mainCopy.desynchronize())
log("main module connect", mainCopy.connect())

actor.Parent = game:GetService("ServerScriptService")
local waited = 0
while not isReady and waited < 10 do
    waited += task.wait()
end
log("actor script ran", isReady)
if isReady then
    local registry = game:GetService("SharedTableRegistry")
    local st = registry:GetSharedTable("RookConformanceActors")
    st.v = "main"
    actor:SendMessage("Order", 1)
    actor:SendMessage("Nobody", "x")
    actor:SendMessage("Order", 2, "b")
    local t = { x = "sent", list = { 1, 2 } }
    actor:SendMessage("Copy", t, t)
    t.x = "changed after send"
    actor:SendMessage("Args")
    actor:SendMessage("Args", 1, nil, "three", nil)
    actor:SendMessage("Args", Vector3.new(1, 2, 3), actor, part, true, 0 / 0 ~= 0 / 0)
    log("send mixed table", caught(function() actor:SendMessage("Args", { 1, 2, n = 3 }) end))
    log("send sparse table", caught(function() actor:SendMessage("Args", { [1] = "a", [3] = "c" }) end))
    log("send fraction key", caught(function() actor:SendMessage("Args", { [1.5] = 1 }) end))
    log("send Instance key", caught(function() actor:SendMessage("Args", { [part] = 1 }) end))
    log("send empty table", caught(function() actor:SendMessage("Args", {}) end))
    log("send metatable", caught(function() actor:SendMessage("Args", setmetatable({ a = 1 }, {})) end))
    actor:SendMessage("Shared", st, "RookConformanceActors")
    actor:SendMessage("Self", "first")
    actor:SendMessage("Drop", 1)
    actor:SendMessage("Drop", 2)
    actor:SendMessage("Error", 1)
    actor:SendMessage("Yield", 1)
    actor:SendMessage("Yield", 2)
    actor:SendMessage("Late", 1)
    actor:SendMessage("Order", 3)
    log("main after send", st.v, t.x, _G.RookConformanceActor)
    task.wait(0.3)
    log("main after delivery", st.v)
    actor:SendMessage("BindLate")
    task.wait(0.3)
    actor:SendMessage("Late", 2)
    actor:SendMessage("Gone")
    task.wait(0.3)
    actor:SendMessage("Report")
    waited = 0
    while report == nil and waited < 10 do
        waited += task.wait()
    end
    for _, line in (report or "no report"):split("\n") do
        log("actor:", line)
    end
    registry:SetSharedTable("RookConformanceActors", nil)
end
_G.RookConformanceMarker = nil
shared.RookConformanceMarker = nil
_G.RookConformanceActor = nil
actor:Destroy()
