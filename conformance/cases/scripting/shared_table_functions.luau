-- SharedTable's functions (clone, cloneAndFreeze, isFrozen, clear,
-- increment, update, size) with their argument errors, what a frozen
-- table refuses, and SharedTableRegistry.
-- Studio prints addresses, which differ every run.
local function shown(v)
    return (tostring(v):gsub("0x%x+", "<address>"))
end
local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = shown(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end
-- Studio walks string keys in a hash order that changes with the table's
-- size, so they're sorted by key where they stand; whole numbers keep
-- their order.
local function keys(t)
    local seen, strings = {}, {}
    for k, v in t do
        seen[#seen + 1] = tostring(k) .. "=" .. (typeof(v) == "SharedTable" and "{}" or tostring(v))
        if type(k) == "string" then
            table.insert(strings, { slot = #seen, key = k, item = seen[#seen] })
        end
    end
    local sorted = table.clone(strings)
    table.sort(sorted, function(a, b)
        return a.key < b.key
    end)
    for i, s in strings do
        seen[s.slot] = sorted[i].item
    end
    return table.concat(seen, " ")
end

-- size.
log("size", SharedTable.size(SharedTable.new({ 1, 2, a = { 1, 2, 3 } })))
log("size empty", SharedTable.size(SharedTable.new()))
log("size type", typeof(SharedTable.size(SharedTable.new())))
log("size no args", caught(SharedTable.size))
log("size table", caught(SharedTable.size, {}))
log("size number", caught(SharedTable.size, 1))
log("size Instance", caught(SharedTable.size, workspace))

-- clone.
local inner = SharedTable.new({ v = 1 })
local st = SharedTable.new({ a = 1, inner = inner })
local shallow = SharedTable.clone(st)
log("clone", shallow == st, keys(shallow))
log("clone shares inner", shallow.inner == inner)
shallow.a = 2
inner.v = 2
log("clone separate", st.a, shallow.inner.v)
local deep = SharedTable.clone(st, true)
log("deep clone", deep.inner == inner, deep.inner.v)
inner.v = 3
log("deep clone separate", deep.inner.v)
log("clone false", caught(function() return SharedTable.clone(st, false).inner == inner end))
log("clone nil deep", caught(function() return SharedTable.clone(st, nil).inner == inner end))
log("clone number deep", caught(function() return SharedTable.clone(st, 1).inner == inner end))
log("clone string deep", caught(function() return SharedTable.clone(st, "yes").inner == inner end))
log("clone no args", caught(SharedTable.clone))
log("clone table", caught(SharedTable.clone, {}))
log("clone same inner twice", caught(function()
    local t = SharedTable.new({ a = inner, b = inner })
    local c = SharedTable.clone(t, true)
    return c.a == c.b, c.a == inner
end))
log("clone cycle shallow", caught(function()
    local t = SharedTable.new()
    t.me = t
    local c = SharedTable.clone(t)
    return c.me == t, c.me == c
end))
log("clone cycle deep", caught(function()
    local t = SharedTable.new()
    t.me = t
    local c = SharedTable.clone(t, true)
    return c.me == t, c.me == c
end))
log("clone frozen", caught(function()
    local f = SharedTable.cloneAndFreeze(SharedTable.new({ x = { 1 } }), true)
    local c = SharedTable.clone(f)
    local d = SharedTable.clone(f, true)
    return SharedTable.isFrozen(c), SharedTable.isFrozen(c.x), SharedTable.isFrozen(d), SharedTable.isFrozen(d.x)
end))
log("clone keeps order", caught(function()
    return keys(SharedTable.clone(SharedTable.new({ b = 1, [2] = 2, a = 3, [1] = 4 })))
end))

-- cloneAndFreeze and isFrozen.
log("isFrozen new", SharedTable.isFrozen(SharedTable.new()))
local frozen = SharedTable.cloneAndFreeze(st)
log("cloneAndFreeze", frozen == st, SharedTable.isFrozen(frozen), SharedTable.isFrozen(st), keys(frozen))
log("frozen shallow inner", frozen.inner == inner, SharedTable.isFrozen(frozen.inner))
local deepFrozen = SharedTable.cloneAndFreeze(st, true)
log("frozen deep inner", deepFrozen.inner == inner, SharedTable.isFrozen(deepFrozen.inner))
log("frozen read", frozen.a, deepFrozen.inner.v)
log("cloneAndFreeze frozen", caught(function()
    local f = SharedTable.cloneAndFreeze(frozen)
    return f == frozen, SharedTable.isFrozen(f)
end))
log("cloneAndFreeze no args", caught(SharedTable.cloneAndFreeze))
log("cloneAndFreeze table", caught(SharedTable.cloneAndFreeze, {}))
log("isFrozen no args", caught(SharedTable.isFrozen))
log("isFrozen table", caught(SharedTable.isFrozen, table.freeze({})))
log("frozen write", caught(function() frozen.a = 5 end))
log("frozen add", caught(function() frozen.zz = 5 end))
log("frozen remove", caught(function() frozen.a = nil end))
log("frozen same value", caught(function() frozen.a = frozen.a end))
log("frozen bad key", caught(function() frozen[1.5] = 1 end))
log("frozen bad value", caught(function() frozen.a = print end))
log("frozen deep inner write", caught(function() deepFrozen.inner.v = 9 end))
log("frozen clear", caught(SharedTable.clear, frozen))
log("frozen increment", caught(SharedTable.increment, frozen, "a", 1))
log("frozen update", caught(function()
    local ran = false
    local r = table.pack(pcall(SharedTable.update, frozen, "a", function(v)
        ran = true
        return v
    end))
    return ran, r[1], (tostring(r[2]):gsub("^[^:]*:%d+: ", ""))
end))
log("frozen unchanged", frozen.a, SharedTable.size(frozen))
log("frozen into new", caught(function()
    local t = SharedTable.new({ f = frozen })
    return t.f == frozen, SharedTable.isFrozen(t.f)
end))
log("new from frozen", caught(function()
    local t = SharedTable.new(frozen)
    return t == frozen, SharedTable.isFrozen(t)
end))

-- clear.
local toClear = SharedTable.new({ 1, 2, a = inner })
SharedTable.clear(toClear)
log("clear", SharedTable.size(toClear), toClear[1], toClear.a, inner.v)
log("clear empty", caught(SharedTable.clear, SharedTable.new()))
log("clear no args", caught(SharedTable.clear))
log("clear table", caught(SharedTable.clear, { 1 }))
log("clear returns", caught(function() return select("#", SharedTable.clear(SharedTable.new())) end))

-- increment.
local counter = SharedTable.new({ n = 10, s = "5", b = true, list = { 1 } })
log("increment", SharedTable.increment(counter, "n", 5), counter.n)
log("increment negative", SharedTable.increment(counter, "n", -20), counter.n)
log("increment fraction", SharedTable.increment(counter, "n", 0.5), counter.n)
log("increment numeric key", caught(function()
    local t = SharedTable.new({ 7 })
    return SharedTable.increment(t, 1, 1), t[1]
end))
log("increment missing", caught(SharedTable.increment, counter, "missing", 1))
log("increment string value", caught(SharedTable.increment, counter, "s", 1))
log("increment boolean value", caught(SharedTable.increment, counter, "b", 1))
log("increment table value", caught(SharedTable.increment, counter, "list", 1))
log("increment string delta", caught(SharedTable.increment, counter, "n", "2"))
log("increment bad delta", caught(SharedTable.increment, counter, "n", "x"))
log("increment no delta", caught(SharedTable.increment, counter, "n"))
log("increment nil delta", caught(SharedTable.increment, counter, "n", nil))
log("increment nan delta", caught(function()
    local t = SharedTable.new({ n = 1 })
    SharedTable.increment(t, "n", 0 / 0)
    return t.n ~= t.n
end))
log("increment bad key", caught(SharedTable.increment, counter, 1.5, 1))
log("increment no key", caught(SharedTable.increment, counter))
log("increment no args", caught(SharedTable.increment))
log("increment table", caught(SharedTable.increment, { n = 1 }, "n", 1))
log("increment after", counter.n, counter.s, counter.b)

-- update.
local u = SharedTable.new({ n = 1, s = "x" })
log("update returns", caught(function() return select("#", SharedTable.update(u, "n", function(v) return v + 1 end)) end))
log("update result", u.n)
log("update missing", caught(function()
    local got
    SharedTable.update(u, "new", function(v)
        got = v
        return "made"
    end)
    return got, u.new
end))
log("update to nil", caught(function()
    SharedTable.update(u, "new", function() return nil end)
    return u.new, SharedTable.size(u)
end))
log("update no return", caught(function()
    SharedTable.update(u, "s", function() end)
    return u.s
end))
log("update args", caught(function()
    local n
    SharedTable.update(u, "n", function(...)
        n = select("#", ...)
        return (...)
    end)
    return n
end))
log("update two returns", caught(function()
    SharedTable.update(u, "n", function(v) return v + 1, "extra" end)
    return u.n
end))
log("update to table", caught(function()
    SharedTable.update(u, "t", function() return { 1, 2 } end)
    return typeof(u.t), u.t[2]
end))
log("update nested value", caught(function()
    local got
    SharedTable.update(u, "t", function(v)
        got = typeof(v)
        return v
    end)
    return got
end))
log("update to function", caught(SharedTable.update, u, "n", function() return print end))
log("update keeps on error", u.n)
log("update error", caught(SharedTable.update, u, "n", function() error("boom", 0) end))
log("update error value", caught(function()
    local ok, e = pcall(SharedTable.update, u, "n", function() error({ code = 1 }) end)
    return ok, typeof(e), type(e) == "table" and e.code
end))
log("update yields", caught(SharedTable.update, u, "n", function(v)
    task.wait()
    return v
end))
log("update after yield", u.n)
log("update not function", caught(SharedTable.update, u, "n", 5))
log("update no function", caught(SharedTable.update, u, "n"))
log("update bad key", caught(SharedTable.update, u, true, function() end))
log("update table", caught(SharedTable.update, {}, "n", function() end))
log("update writes other key", caught(function()
    local t = SharedTable.new({ n = 1 })
    local calls = 0
    local ok, e = pcall(SharedTable.update, t, "n", function(v)
        calls += 1
        if calls == 1 then
            t.other = v
        end
        return v + 1
    end)
    return ok, (tostring(e):gsub("^[^:]*:%d+: ", "")), calls, t.n, t.other
end))
log("update reads during", caught(function()
    local t = SharedTable.new({ n = 1, m = 5 })
    SharedTable.update(t, "n", function(v) return v + t.m end)
    return t.n
end))
log("nested update", caught(function()
    local t = SharedTable.new({ a = 1, b = 1 })
    SharedTable.update(t, "a", function(v)
        SharedTable.update(t, "b", function(w) return w + 1 end)
        return v + 1
    end)
    return t.a, t.b
end))

-- SharedTableRegistry.
local registry = game:GetService("SharedTableRegistry")
log("registry", registry.ClassName, registry.Name, typeof(registry))
local one = registry:GetSharedTable("RookConformanceA")
log("get new", typeof(one), SharedTable.size(one), SharedTable.isFrozen(one))
log("get same", registry:GetSharedTable("RookConformanceA") == one)
one.x = 1
log("get keeps", registry:GetSharedTable("RookConformanceA").x)
local mine = SharedTable.new({ y = 2 })
registry:SetSharedTable("RookConformanceA", mine)
log("set replaces", registry:GetSharedTable("RookConformanceA") == mine, registry:GetSharedTable("RookConformanceA") == one)
registry:SetSharedTable("RookConformanceA", nil)
local after = registry:GetSharedTable("RookConformanceA")
log("set nil", after == mine, SharedTable.size(after))
log("set no table", caught(function() registry:SetSharedTable("RookConformanceA") end))
log("set Luau table", caught(function() registry:SetSharedTable("RookConformanceA", { 1 }) end))
log("set number", caught(function() registry:SetSharedTable("RookConformanceA", 5) end))
log("set frozen", caught(function()
    local f = SharedTable.cloneAndFreeze(SharedTable.new({ z = 3 }))
    registry:SetSharedTable("RookConformanceA", f)
    local got = registry:GetSharedTable("RookConformanceA")
    return got == f, SharedTable.isFrozen(got)
end))
log("same table two names", caught(function()
    registry:SetSharedTable("RookConformanceB", mine)
    registry:SetSharedTable("RookConformanceA", mine)
    return registry:GetSharedTable("RookConformanceA") == registry:GetSharedTable("RookConformanceB")
end))
log("name empty", caught(function() return SharedTable.size(registry:GetSharedTable("")) end))
log("name number", caught(function() return registry:GetSharedTable(5) == registry:GetSharedTable("5") end))
log("name nil", caught(function() return registry:GetSharedTable(nil) end))
log("name none", caught(function() return registry:GetSharedTable() end))
log("name table", caught(function() return registry:GetSharedTable({}) end))
log("set name nil", caught(function() registry:SetSharedTable(nil, mine) end))
log("dot call", caught(function() return registry.GetSharedTable("RookConformanceA") end))
log("children", #registry:GetChildren())
registry:SetSharedTable("RookConformanceA", nil)
registry:SetSharedTable("RookConformanceB", nil)
registry:SetSharedTable("", nil)
registry:SetSharedTable("5", nil)
