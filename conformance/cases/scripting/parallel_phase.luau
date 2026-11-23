-- What desynchronized code may do. A Script made at runtime under an
-- Actor (its Source set by this case, which Studio's MCP allows at plugin
-- identity) calls task.desynchronize() and tries reads, writes, method
-- calls, signals, callbacks, Instance.new, require, task functions and
-- messages, then synchronizes and tries writes again. It reports its
-- lines back through a BindableEvent.
local actor = Instance.new("Actor")
local module = Instance.new("ModuleScript")
module.Name = "Module"
local worker = Instance.new("Script")
worker.Name = "Worker"
local sourceSet = pcall(function()
    module.Source = "return { loaded = true }"
end)
log("Source settable", sourceSet)
if not sourceSet then
    actor:Destroy()
    return
end
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
-- Only whether it worked, for calls whose result differs between places.
local function status(f, ...)
    local ok, e = pcall(f, ...)
    if ok then
        return "ok"
    end
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end

local actor = script:GetActor()
local done = actor.Done
local run, runError = pcall(function()
    local part = Instance.new("Part")
    local folder = Instance.new("Folder")
    local frame = Instance.new("Frame")
    local value = Instance.new("IntValue")
    local bindable = Instance.new("BindableEvent")
    local bf = Instance.new("BindableFunction")
    local event = bindable.Event
    local own = Instance.new("Part")
    own.Name = "Own"
    own.Parent = actor
    local child = Instance.new("Folder")
    child.Name = "Child"
    child.Parent = folder
    local st = SharedTable.new({ n = 0 })
    local cs = game:GetService("CollectionService")
    local http = game:GetService("HttpService")
    local runService = game:GetService("RunService")
    local registry = game:GetService("SharedTableRegistry")

    rec("serial write", caught(function() part.Name = "Serial" end))
    task.desynchronize()
    rec("desynchronized")
    rec("desynchronize again", caught(task.desynchronize))

    -- Reads.
    rec("read Name", caught(function() return part.Name end))
    rec("read Size", caught(function() return part.Size end))
    rec("read Parent", caught(function() return child.Parent end))
    rec("read ClassName", caught(function() return part.ClassName end))
    rec("read Value", caught(function() return value.Value end))
    rec("read AbsoluteSize", caught(function() return frame.AbsoluteSize end))
    rec("read AbsolutePosition", caught(function() return frame.AbsolutePosition end))
    rec("read Gravity", status(function() return workspace.Gravity end))
    rec("read game.Workspace", status(function() return game.Workspace end))
    rec("read script", caught(function() return script.Name end))
    rec("read missing member", caught(function() return part.Nope end))
    rec("read child by name", caught(function() return folder.Child.Name end))

    -- Writes.
    rec("write Name", caught(function() part.Name = "x" end))
    rec("write Size", caught(function() part.Size = Vector3.one end))
    rec("write Anchored", caught(function() part.Anchored = true end))
    rec("write same value", caught(function() part.Name = part.Name end))
    rec("write Parent", caught(function() part.Parent = folder end))
    rec("write Value", caught(function() value.Value = 1 end))
    rec("write frame Size", caught(function() frame.Size = UDim2.new() end))
    rec("write own instance", caught(function() own.Name = "Mine" end))
    rec("write missing member", caught(function() part.Nope = 1 end))
    rec("write wrong type", caught(function() part.Anchored = "x" end))
    rec("after writes", part.Name, part.Anchored, part.Parent, value.Value, own.Name)

    -- Attributes and tags.
    rec("SetAttribute", caught(function() part:SetAttribute("a", 1) end))
    rec("GetAttribute", caught(function() return part:GetAttribute("a") end))
    rec("GetAttributes", caught(function() return typeof(part:GetAttributes()) end))
    rec("AddTag", caught(function() part:AddTag("RookConformance") end))
    rec("HasTag", caught(function() return part:HasTag("RookConformance") end))
    rec("GetTags", caught(function() return #part:GetTags() end))
    rec("CollectionService:HasTag", caught(function() return cs:HasTag(part, "RookConformance") end))
    rec("CollectionService:AddTag", caught(function() cs:AddTag(part, "RookConformance") end))

    -- Methods.
    rec("FindFirstChild", caught(function() return folder:FindFirstChild("Child") end))
    rec("findFirstChild", caught(function() return folder:findFirstChild("Child") end))
    rec("FindFirstChildOfClass", caught(function() return folder:FindFirstChildOfClass("Folder") end))
    rec("FindFirstChildWhichIsA", caught(function() return folder:FindFirstChildWhichIsA("Instance") end))
    rec("FindFirstAncestor", caught(function() return child:FindFirstAncestor("x") end))
    rec("FindFirstDescendant", caught(function() return folder:FindFirstDescendant("Child") end))
    rec("GetChildren", caught(function() return #folder:GetChildren() end))
    rec("children", caught(function() return #folder:children() end))
    rec("GetDescendants", caught(function() return #folder:GetDescendants() end))
    rec("IsA", caught(function() return part:IsA("BasePart") end))
    rec("isA", caught(function() return part:isA("BasePart") end))
    rec("IsDescendantOf", caught(function() return child:IsDescendantOf(folder) end))
    rec("GetFullName", caught(function() return child:GetFullName() end))
    rec("GetActor", caught(function() return own:GetActor() == actor end))
    rec("WaitForChild", caught(function() return folder:WaitForChild("Child") end))
    rec("Clone", caught(function() return part:Clone() end))
    rec("Destroy", caught(function() Instance.new("Folder"):Destroy() end))
    rec("ClearAllChildren", caught(function() Instance.new("Folder"):ClearAllChildren() end))
    rec("GetPropertyChangedSignal", caught(function() return part:GetPropertyChangedSignal("Name") end))
    rec("GetAttributeChangedSignal", caught(function() return part:GetAttributeChangedSignal("a") end))
    rec("GetPivot", caught(function() return part:GetPivot() end))
    rec("PivotTo", caught(function() part:PivotTo(CFrame.new()) end))
    rec("GetMass", caught(function() return part:GetMass() end))
    rec("GetService", caught(function() return game:GetService("Workspace") end))
    rec("FindService", caught(function() return game:FindService("Workspace") end))
    rec("Raycast", status(function() return workspace:Raycast(Vector3.new(0, 1000, 0), Vector3.new(0, 1, 0)) end))
    rec("GetPartBoundsInBox", status(function() return workspace:GetPartBoundsInBox(CFrame.new(0, 1000, 0), Vector3.one) end))
    rec("IsServer", caught(function() return runService:IsServer() end))
    rec("JSONEncode", caught(function() return http:JSONEncode({ 1 }) end))
    rec("GenerateGUID", status(function() return http:GenerateGUID() end))
    rec("missing method", caught(function() part:Nope() end))

    -- Signals and callbacks.
    rec("read event", caught(function() return part.Touched end))
    rec("read Changed", caught(function() return part.Changed end))
    rec("read bindable Event", caught(function() return bindable.Event end))
    rec("Connect held signal", caught(function() return event:Connect(function() end) end))
    rec("Once held signal", caught(function() return event:Once(function() end) end))
    rec("ConnectParallel held signal", caught(function() return event:ConnectParallel(function() end) end))
    rec("Fire", caught(function() bindable:Fire() end))
    rec("set OnInvoke", caught(function() bf.OnInvoke = function() return 1 end end))
    rec("read OnInvoke", caught(function() return bf.OnInvoke end))

    -- Instances.
    local made
    rec("Instance.new", caught(function()
        made = Instance.new("Part")
        return typeof(made), made.Name
    end))
    rec("Instance.new with parent", caught(function() return Instance.new("Part", folder) end))
    rec("write new instance", caught(function() made.Name = "Made" end))
    rec("Destroy new instance", caught(function() made:Destroy() end))

    -- Modules, messages, SharedTables.
    rec("require", caught(function() return require(actor.Module).loaded end))
    rec("SendMessage", caught(function() actor:SendMessage("Nobody", 1) end))
    rec("BindToMessage", caught(function() return actor:BindToMessage("x", function() end) end))
    rec("BindToMessageParallel", caught(function() return actor:BindToMessageParallel("x", function() end) end))
    rec("SharedTable.increment", caught(function() return SharedTable.increment(st, "n", 1), st.n end))
    rec("SharedTableRegistry", caught(function()
        return typeof(registry:GetSharedTable("RookConformanceParallel"))
    end))

    -- Globals.
    rec("print", status(print, "parallel print"))
    rec("warn", status(warn, "parallel warn"))
    rec("Vector3 math", caught(function() return Vector3.new(1, 2, 3) * 2 end))
    rec("tick", status(tick))
    rec("os.clock", status(os.clock))
    rec("Random", status(function() return Random.new(1):NextNumber() end))
    rec("game", caught(function() return game.ClassName end))

    -- Threads made or resumed in parallel.
    local function stillParallel(label)
        return function()
            rec(label, caught(function() part.Name = "y" end))
        end
    end
    rec("spawn", status(task.spawn, stillParallel("spawned")))
    rec("defer", status(task.defer, stillParallel("deferred")))
    rec("delay", status(task.delay, 0, stillParallel("delayed")))
    rec("coroutine.wrap", caught(function() coroutine.wrap(stillParallel("wrapped"))() end))
    rec("legacy spawn", status(spawn, stillParallel("legacy spawned")))
    task.wait()
    stillParallel("after task.wait")()
    wait()
    stillParallel("after wait")()
    task.wait(0.1)
    stillParallel("after delays")()

    task.synchronize()
    rec("synchronized")
    rec("synchronize again", caught(task.synchronize))
    rec("write after synchronize", caught(function() part.Name = "Serial" end))
    rec("Clone after synchronize", caught(function() return part:Clone().Name end))
    rec("Fire after synchronize", caught(function() bindable:Fire() end))
    rec("require after synchronize", caught(function() return require(actor.Module).loaded end))
    rec("SetAttribute after synchronize", caught(function() part:SetAttribute("b", 2) end))
    task.defer(stillParallel("deferred after synchronize"))
    task.wait()
    part:Destroy()
    folder:Destroy()
    registry:SetSharedTable("RookConformanceParallel", nil)
end)
if not run then
    rec("ACTOR ERROR", runError)
end
task.synchronize()
done:Fire(table.concat(lines, "\n"))
]==]
worker.Parent = actor
module.Parent = actor
local done = Instance.new("BindableEvent")
done.Name = "Done"
done.Parent = actor
local report
done.Event:Connect(function(text)
    report = text
end)
actor.Parent = game:GetService("ServerScriptService")
local waited = 0
while report == nil and waited < 20 do
    waited += task.wait()
end
for _, line in (report or "no report"):split("\n") do
    log(line)
end
actor:Destroy()
