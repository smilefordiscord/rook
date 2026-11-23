-- SharedTable: the global table, constructing one, which keys and values
-- it takes and the errors for the rest, nesting, equality and identity,
-- printing, the Luau operators and raw functions, and iteration order.
-- The SharedTable.* functions and the registry are shared_table_functions.
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

-- The global.
log("global", typeof(SharedTable), table.isfrozen(SharedTable))
local names = {}
for k, v in pairs(SharedTable) do
    names[#names + 1] = tostring(k) .. ":" .. typeof(v)
end
table.sort(names)
log("functions", table.concat(names, " "))
log("assign global", caught(function() SharedTable.x = 1 end))

-- Constructing.
local empty = SharedTable.new()
log("new", shown(empty), typeof(empty), type(empty), SharedTable.size(empty))
log("new nil", caught(function() return SharedTable.size(SharedTable.new(nil)) end))
log("new number", caught(SharedTable.new, 5))
log("new string", caught(SharedTable.new, "abc"))
log("new boolean", caught(SharedTable.new, true))
log("new Instance", caught(SharedTable.new, workspace))
log("new two tables", caught(function() return SharedTable.size(SharedTable.new({ a = 1 }, { b = 2 })) end))
local fromShared = SharedTable.new({ a = 1 })
log("new SharedTable", caught(function()
    local copy = SharedTable.new(fromShared)
    return copy == fromShared, copy.a, SharedTable.size(copy)
end))
log("method call", caught(function() return SharedTable:new() end))

local st = SharedTable.new({ a = 1, b = true, c = "s", [1] = "one", [2] = "two" })
log("read", st.a, st.b, st.c, st[1], st[2], st.missing, st[3])
log("size", SharedTable.size(st))
log("string number keys distinct", caught(function() return st["1"], st[1.0] end))

-- Keys.
local function setKey(k)
    return caught(function()
        local t = SharedTable.new()
        t[k] = "v"
        return t[k], SharedTable.size(t)
    end)
end
log("key empty string", setKey(""))
log("key 0", setKey(0))
log("key -0", setKey(-0))
log("key 2^32-1", setKey(2 ^ 32 - 1))
log("key 2^32", setKey(2 ^ 32))
log("key -1", setKey(-1))
log("key 1.5", setKey(1.5))
log("key nan", setKey(0 / 0))
log("key inf", setKey(math.huge))
log("key true", setKey(true))
log("key Vector3", setKey(Vector3.one))
log("key Instance", setKey(workspace))
log("key table", setKey({}))
log("key SharedTable", setKey(SharedTable.new()))
log("read key 1.5", caught(function() return st[1.5] end))
log("read key -1", caught(function() return st[-1] end))
log("read key true", caught(function() return st[true] end))
log("read key nil", caught(function() return st[nil] end))
log("write key nil", caught(function() st[nil] = 1 end))
log("new key 1.5", caught(function() return SharedTable.new({ [1.5] = 1 }) end))
log("new key true", caught(function() return SharedTable.new({ [true] = 1 }) end))
log("new key -1", caught(function() return SharedTable.new({ [-1] = 1 }) end))
log("new key table", caught(function() return SharedTable.new({ [{}] = 1 }) end))

-- Values.
local function setValue(v)
    return caught(function()
        local t = SharedTable.new()
        t.k = v
        local back = t.k
        return typeof(back), back, back == v
    end)
end
log("value nil", caught(function()
    local t = SharedTable.new({ k = 1 })
    t.k = nil
    return t.k, SharedTable.size(t)
end))
log("value integer", setValue(7))
log("value fraction", setValue(-2.25))
log("value nan", caught(function()
    local t = SharedTable.new()
    t.k = 0 / 0
    return t.k ~= t.k
end))
log("value inf", setValue(-math.huge))
log("value false", setValue(false))
log("value string", setValue("hello"))
log("value empty string", setValue(""))
log("value embedded zero", caught(function()
    local t = SharedTable.new()
    t.k = "a\0b"
    return #t.k
end))
log("value Vector3", setValue(Vector3.new(1, 2, 3)))
log("value vector", setValue(vector.create(1, 2, 3)))
log("value Vector2", setValue(Vector2.new(1, 2)))
log("value Vector3int16", setValue(Vector3int16.new(1, 2, 3)))
log("value CFrame", setValue(CFrame.new(1, 2, 3)))
log("value Color3", setValue(Color3.new(1, 0, 0)))
log("value BrickColor", setValue(BrickColor.new("Bright red")))
log("value UDim", setValue(UDim.new(1, 2)))
log("value UDim2", setValue(UDim2.new(1, 2, 3, 4)))
log("value Rect", setValue(Rect.new(1, 2, 3, 4)))
log("value NumberRange", setValue(NumberRange.new(1, 2)))
log("value NumberSequence", setValue(NumberSequence.new(1)))
log("value ColorSequence", setValue(ColorSequence.new(Color3.new(0, 1, 0))))
log("value Ray", setValue(Ray.new(Vector3.zero, Vector3.one)))
log("value Region3", setValue(Region3.new(Vector3.zero, Vector3.one)))
log("value Font", setValue(Font.new("rbxasset://fonts/families/Arial.json")))
log("value EnumItem", setValue(Enum.Material.Plastic))
log("value Enum", setValue(Enum.Material))
log("value Enums", setValue(Enum))
log("value TweenInfo", setValue(TweenInfo.new(1)))
log("value DateTime", setValue(DateTime.fromUnixTimestamp(0)))
log("value PhysicalProperties", setValue(PhysicalProperties.new(1, 0.3, 0.5)))
log("value Random", setValue(Random.new(1)))
log("value Content", setValue(Content.fromUri("rbxasset://a.png")))
log("value function", setValue(print))
log("value Luau function", setValue(function() end))
log("value thread", setValue(coroutine.create(function() end)))
log("value buffer", setValue(buffer.create(4)))
log("value Instance", setValue(workspace))
log("value unparented Instance", setValue(Instance.new("Folder")))
log("value userdata", setValue(newproxy()))
log("value RBXScriptSignal", setValue(workspace.ChildAdded))
log("value RBXScriptConnection", caught(function()
    local c = workspace.ChildAdded:Connect(function() end)
    local r = table.pack(setValue(c))
    c:Disconnect()
    return table.unpack(r, 1, r.n)
end))
log("new function value", caught(function() return SharedTable.new({ f = print }) end))
log("new Instance value", caught(function() return SharedTable.new({ workspace }) end))
log("new nested function", caught(function() return SharedTable.new({ inner = { deeper = { f = print } } }) end))
log("failed assign keeps old", caught(function()
    local t = SharedTable.new({ k = 1 })
    pcall(function() t.k = print end)
    return t.k
end))

-- Tables as values.
local nested = SharedTable.new({ inner = { x = 1, list = { 10, 20 } } })
log("nested", typeof(nested.inner), nested.inner.x, typeof(nested.inner.list), nested.inner.list[2])
log("assign table", caught(function()
    local t = SharedTable.new()
    local plain = { y = 2 }
    t.k = plain
    plain.y = 3
    return typeof(t.k), t.k.y
end))
log("assign frozen table", caught(function()
    local t = SharedTable.new()
    t.k = table.freeze({ 1 })
    return typeof(t.k), SharedTable.isFrozen(t.k)
end))
log("assign empty table", caught(function()
    local t = SharedTable.new()
    t.k = {}
    return typeof(t.k), SharedTable.size(t.k)
end))
log("table with metatable", caught(function()
    local t = SharedTable.new({ k = setmetatable({}, { __index = { a = 1 } }) })
    return typeof(t.k), t.k.a
end))
log("table with sparse keys", caught(function()
    local t = SharedTable.new({ [1] = "a", [3] = "c", [100] = "z" })
    return t[1], t[2], t[3], t[100], SharedTable.size(t)
end))
log("same table twice", caught(function()
    local inner = { 1 }
    local t = SharedTable.new({ a = inner, b = inner })
    return t.a == t.b
end))
log("cycle", caught(function()
    local c = {}
    c.self = c
    return SharedTable.new(c)
end))
log("assign cycle", caught(function()
    local c = {}
    c.self = c
    local t = SharedTable.new()
    t.k = c
    return SharedTable.size(t)
end))
log("SharedTable in itself", caught(function()
    local t = SharedTable.new()
    t.me = t
    return t.me == t, t.me.me == t, SharedTable.size(t)
end))
log("SharedTable shared", caught(function()
    local inner = SharedTable.new({ v = 1 })
    local t = SharedTable.new({ a = inner })
    inner.v = 2
    t.b = inner
    return t.a.v, t.a == inner, t.b == t.a
end))

-- Identity and equality.
log("equal self", st == st, rawequal(st, st))
log("equal empty", SharedTable.new() == SharedTable.new())
log("equal nested read twice", nested.inner == nested.inner, rawequal(nested.inner, nested.inner))
log("equal other types", st == nil, st == {}, st == "SharedTable")

-- Operators and raw functions.
log("length", caught(function() return #st end))
log("length empty", caught(function() return #SharedTable.new() end))
log("length array", caught(function() return #SharedTable.new({ 1, 2, 3 }) end))
log("concat", caught(function() return "x" .. st end))
log("add", caught(function() return st + 1 end))
log("compare", caught(function() return st < st end))
log("call", caught(function() return st() end))
log("tostring", shown(st), shown(nested.inner))
log("string.format", caught(string.format, "%s", st))
log("rawget", caught(rawget, st, "a"))
log("rawset", caught(rawset, st, "a", 2))
log("rawlen", caught(rawlen, st))
log("getmetatable", caught(function() return type(getmetatable(st)) end))
log("setmetatable", caught(setmetatable, st, {}))
log("table.isfrozen", caught(table.isfrozen, st))
log("table.freeze", caught(table.freeze, st))
log("table.insert", caught(table.insert, st, 1))
log("table.concat", caught(table.concat, st))
log("table.clone", caught(table.clone, st))
log("unpack", caught(function() return table.unpack(SharedTable.new({ 1, 2 })) end))
log("json", caught(function() return game:GetService("HttpService"):JSONEncode(SharedTable.new({ a = 1 })) end))
log("attribute", caught(function()
    local f = Instance.new("Folder")
    f:SetAttribute("st", st)
end))
log("method on table", caught(function() return st:size() end))
log("field named size", caught(function()
    local t = SharedTable.new({ size = 5 })
    return t.size, SharedTable.size(t)
end))

-- Iteration.
-- Studio walks string keys in a hash order that changes with the table's
-- size, so they're sorted by key where they stand; whole numbers keep
-- their order.
local function sortStrings(seen, strings)
    local sorted = table.clone(strings)
    table.sort(sorted, function(a, b)
        return a.key < b.key
    end)
    for i, s in strings do
        seen[s.slot] = sorted[i].item
    end
end
local function walk(t)
    local seen, strings = {}, {}
    for k, v in t do
        seen[#seen + 1] = typeof(k) .. ":" .. tostring(k) .. "=" .. tostring(v)
        if type(k) == "string" then
            table.insert(strings, { slot = #seen, key = k, item = seen[#seen] })
        end
    end
    sortStrings(seen, strings)
    return table.concat(seen, " ")
end
local mixed = SharedTable.new({
    b = 1, a = 2, B = 3, aa = 4, [""] = 5, ["10"] = 6, ["2"] = 7,
    [3] = "x", [1] = "y", [2] = "z", [0] = "w", [10] = "v", [4294967295] = "u",
})
log("order", walk(mixed))
log("order inserted", caught(function()
    local t = SharedTable.new()
    t.z = 1
    t[5] = 1
    t.a = 1
    t[1] = 1
    t.m = 1
    return walk(t)
end))
log("order after delete", caught(function()
    local t = SharedTable.new({ 1, 2, 3, a = 1, b = 2 })
    t[2] = nil
    t.a = nil
    t[2] = 2
    t.a = 1
    return walk(t)
end))
log("iterate empty", caught(function() return walk(SharedTable.new()) end))
log("iterate nested", caught(function()
    local t = SharedTable.new({ inner = { 1 } })
    for k, v in t do
        return k, typeof(v), v == t.inner
    end
end))
log("pairs", caught(function()
    for k in pairs(SharedTable.new({ a = 1 })) do
        return k
    end
end))
log("ipairs", caught(function()
    for i, v in ipairs(SharedTable.new({ 5 })) do
        return i, v
    end
end))
log("next", caught(next, SharedTable.new({ a = 1 })))
log("add while iterating", caught(function()
    local t = SharedTable.new({ a = 1, b = 2 })
    local seen = {}
    for k in t do
        seen[#seen + 1] = k
        t["new" .. k] = 1
    end
    table.sort(seen)
    return table.concat(seen, " "), SharedTable.size(t)
end))
log("remove while iterating", caught(function()
    local t = SharedTable.new({ a = 1, b = 2, c = 3 })
    local seen = {}
    for k, v in t do
        seen[#seen + 1] = k .. "=" .. tostring(v)
        t.b = nil
        t.c = 30
    end
    table.sort(seen)
    return table.concat(seen, " ")
end))
log("break and restart", caught(function()
    local t = SharedTable.new({ 1, 2, 3 })
    for k in t do
        break
    end
    local n = 0
    for k in t do
        n += 1
    end
    return n
end))

-- Passing through a bindable.
log("through BindableFunction", caught(function()
    local f = Instance.new("BindableFunction")
    f.OnInvoke = function(t, list)
        return t == st, typeof(t), typeof(list[1]), list[1] == st
    end
    local r = table.pack(f:Invoke(st, { st }))
    f:Destroy()
    return table.unpack(r, 1, r.n)
end))
