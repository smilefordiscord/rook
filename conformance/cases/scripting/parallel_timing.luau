-- Where the frame's parallel phases are. A Script made at runtime under an
-- Actor (its Source set by this case, which Studio's MCP allows at plugin
-- identity) connects serial and parallel handlers to RunService's frame
-- events and, from the first Heartbeat it sees, records the order in
-- which they and the threads, deferred work, messages and bindable
-- handlers started there run, for two frames. Each label carries the
-- number of Heartbeats seen so far.
local actor = Instance.new("Actor")
local worker = Instance.new("Script")
worker.Name = "Worker"
local sourceSet = pcall(function()
    worker.Source = [==[
local actor = script:GetActor()
local done = actor.Done
local rs = game:GetService("RunService")
local trace = {}
local n = 0
local recording = true
local function add(label)
    if recording and n >= 1 then
        table.insert(trace, label .. " " .. n)
    end
end
local connections = {}
local function keep(c)
    table.insert(connections, c)
end
local bindable = Instance.new("BindableEvent")
keep(bindable.Event:Connect(function()
    add("bindable serial")
end))
keep(bindable.Event:ConnectParallel(function()
    add("bindable parallel")
end))
keep(actor:BindToMessage("Ping", function()
    add("message serial")
end))
keep(actor:BindToMessageParallel("Ping", function()
    add("message parallel")
end))
keep(rs.Heartbeat:Connect(function()
    n += 1
    if n == 3 then
        recording = false
        for _, c in connections do
            c:Disconnect()
        end
        bindable:Destroy()
        done:Fire(table.concat(trace, "\n"))
        return
    end
    add("Heartbeat serial")
    if n ~= 1 then
        return
    end
    task.spawn(function()
        add("spawned")
        task.desynchronize()
        add("desynchronized from Heartbeat")
        task.synchronize()
        add("synchronized after desynchronizing")
    end)
    task.spawn(function()
        task.wait()
        add("waited from Heartbeat")
    end)
    task.defer(function()
        add("deferred from Heartbeat")
    end)
    task.delay(0, function()
        add("delayed from Heartbeat")
    end)
    actor:SendMessage("Ping")
    bindable:Fire()
    add("Heartbeat serial end")
end))
keep(rs.Heartbeat:ConnectParallel(function()
    add("Heartbeat parallel")
    if n ~= 1 then
        return
    end
    task.defer(function()
        add("deferred in parallel")
    end)
    task.spawn(function()
        add("spawned in parallel")
    end)
    task.delay(0, function()
        add("delayed in parallel")
    end)
    task.synchronize()
    add("synchronized from Heartbeat parallel")
end))
for _, name in { "PreSimulation", "Stepped", "PostSimulation" } do
    local signal = rs[name]
    keep(signal:Connect(function()
        add(name .. " serial")
    end))
    keep(signal:ConnectParallel(function()
        add(name .. " parallel")
    end))
end
]==]
end)
log("Source settable", sourceSet)
if not sourceSet then
    actor:Destroy()
    return
end
worker.Parent = actor
local done = Instance.new("BindableEvent")
done.Name = "Done"
done.Parent = actor
local report
done.Event:Connect(function(text)
    report = text
end)
actor.Parent = game:GetService("ServerScriptService")
local waited = 0
while report == nil and waited < 10 do
    waited += task.wait()
end
for _, line in (report or "no report"):split("\n") do
    log(line)
end
actor:Destroy()
