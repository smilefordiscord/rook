-- HttpService's requests (docs/behavior/scripting/http-requests.md): what
-- GetAsync, PostAsync and RequestAsync raise for missing and mistyped
-- arguments, URLs that aren't http(s), Roblox's own domains, local
-- addresses, bad methods and headers, a Body with GET, Timeout, and with
-- HttpEnabled off, which of these come before "not enabled". HttpEnabled is
-- logged first, so the mode the place was recorded in shows.
-- Requests only ever go to port 1 of this machine (refused at once) or to
-- roblox.com (refused before sending); a success logs only its status.
-- Every request runs in a thread of its own and the lines are logged in
-- order once all have settled, or as "pending" after 10 s.
local HttpService = game:GetService("HttpService")

-- Studio's MCP, which records the cases, runs them as Assistant, whose own
-- HTTP permission is checked where HttpEnabled is: either refusal logs as
-- "<not enabled>".
local function strip(e)
    if type(e) ~= "string" then
        return tostring(e)
    end
    e = e:gsub("^[^:]*:%d+: ", "")
    if e == "Assistant HTTP permissions are not configured" or e:find("^Http requests are not enabled") then
        return "<not enabled>"
    end
    return e
end
local function settle(f)
    local ok, v = pcall(f)
    if not ok then
        return "error " .. strip(v)
    end
    if type(v) == "table" then
        return "ok table " .. tostring(v.StatusCode) .. " " .. tostring(v.Success)
    end
    return "ok " .. type(v)
end

log("HttpEnabled", HttpService.HttpEnabled)

local jobs = {}
local function req(label, f)
    table.insert(jobs, { label = label, f = f })
end

local LOCAL = "http://127.0.0.1:1/"

-- GetAsync: the URL argument.
req("get no args", function() return HttpService:GetAsync() end)
req("get nil", function() return HttpService:GetAsync(nil) end)
req("get number", function() return HttpService:GetAsync(5) end)
req("get table", function() return HttpService:GetAsync({}) end)
req("get bool", function() return HttpService:GetAsync(true) end)
req("get empty", function() return HttpService:GetAsync("") end)
req("get no scheme", function() return HttpService:GetAsync("127.0.0.1:1/") end)
req("get words", function() return HttpService:GetAsync("not a url") end)
req("get ftp", function() return HttpService:GetAsync("ftp://127.0.0.1:1/") end)
req("get file", function() return HttpService:GetAsync("file:///C:/x.txt") end)
req("get scheme only", function() return HttpService:GetAsync("http://") end)
req("get upper scheme", function() return HttpService:GetAsync("HTTP://127.0.0.1:1/") end)
req("get space in host", function() return HttpService:GetAsync("http://a b/") end)

-- Where a request may go.
req("get 127.0.0.1", function() return HttpService:GetAsync(LOCAL) end)
req("get localhost", function() return HttpService:GetAsync("http://localhost:1/") end)
req("get ::1", function() return HttpService:GetAsync("http://[::1]:1/") end)
req("get 0.0.0.0", function() return HttpService:GetAsync("http://0.0.0.0:1/") end)
req("get https local", function() return HttpService:GetAsync("https://127.0.0.1:1/") end)
req("get roblox.com", function() return HttpService:GetAsync("https://www.roblox.com/") end)
req("get ROBLOX.COM", function() return HttpService:GetAsync("https://ROBLOX.COM/") end)
req("get apis.roblox.com", function() return HttpService:GetAsync("https://apis.roblox.com/") end)
req("get roblox in query", function() return HttpService:GetAsync(LOCAL .. "?u=roblox.com") end)
req("get roblox in path", function() return HttpService:GetAsync(LOCAL .. "www.roblox.com/") end)
req("get unresolvable", function() return HttpService:GetAsync("http://nothing.invalid/") end)

-- GetAsync: nocache and headers.
req("get nocache", function() return HttpService:GetAsync(LOCAL, true) end)
req("get nocache string", function() return HttpService:GetAsync(LOCAL, "yes") end)
req("get headers", function() return HttpService:GetAsync(LOCAL, false, { ["X-A"] = "1" }) end)
req("get headers string", function() return HttpService:GetAsync(LOCAL, false, "x") end)
req("get headers array", function() return HttpService:GetAsync(LOCAL, false, { "a" }) end)
req("get header number", function() return HttpService:GetAsync(LOCAL, false, { X = 5 }) end)
req("get header table", function() return HttpService:GetAsync(LOCAL, false, { X = {} }) end)
req("get header bool", function() return HttpService:GetAsync(LOCAL, false, { X = true }) end)
req("get User-Agent", function() return HttpService:GetAsync(LOCAL, false, { ["User-Agent"] = "x" }) end)
req("get user-agent", function() return HttpService:GetAsync(LOCAL, false, { ["user-agent"] = "x" }) end)
req("get Roblox-Id", function() return HttpService:GetAsync(LOCAL, false, { ["Roblox-Id"] = "x" }) end)
req("get Content-Length", function() return HttpService:GetAsync(LOCAL, false, { ["Content-Length"] = "5" }) end)
req("get space in name", function() return HttpService:GetAsync(LOCAL, false, { ["Bad Name"] = "x" }) end)
req("get newline in value", function() return HttpService:GetAsync(LOCAL, false, { X = "a\nb" }) end)
req("get empty name", function() return HttpService:GetAsync(LOCAL, false, { [""] = "x" }) end)

-- PostAsync.
req("post no data", function() return HttpService:PostAsync(LOCAL) end)
req("post number data", function() return HttpService:PostAsync(LOCAL, 5) end)
req("post table data", function() return HttpService:PostAsync(LOCAL, {}) end)
req("post content type", function() return HttpService:PostAsync(LOCAL, "x", Enum.HttpContentType.TextPlain) end)
req("post content type name", function() return HttpService:PostAsync(LOCAL, "x", "TextPlain") end)
req("post content type bad name", function() return HttpService:PostAsync(LOCAL, "x", "json") end)
req("post content type number", function() return HttpService:PostAsync(LOCAL, "x", 1) end)
req("post content type other enum", function() return HttpService:PostAsync(LOCAL, "x", Enum.Material.Wood) end)
req("post compress", function() return HttpService:PostAsync(LOCAL, "x", nil, true) end)
req("post headers string", function() return HttpService:PostAsync(LOCAL, "x", nil, false, "x") end)
req("post Content-Type header", function()
    return HttpService:PostAsync(LOCAL, "x", nil, false, { ["Content-Type"] = "text/csv" })
end)
req("post roblox.com", function() return HttpService:PostAsync("https://www.roblox.com/", "x") end)
req("post ftp", function() return HttpService:PostAsync("ftp://127.0.0.1:1/", "x") end)

-- RequestAsync: the options table.
req("request no args", function() return HttpService:RequestAsync() end)
req("request nil", function() return HttpService:RequestAsync(nil) end)
req("request string", function() return HttpService:RequestAsync("x") end)
req("request array", function() return HttpService:RequestAsync({ LOCAL }) end)
req("request empty", function() return HttpService:RequestAsync({}) end)
req("request Url number", function() return HttpService:RequestAsync({ Url = 5 }) end)
req("request Url table", function() return HttpService:RequestAsync({ Url = {} }) end)
req("request lowercase url key", function() return HttpService:RequestAsync({ url = LOCAL }) end)
req("request Url only", function() return HttpService:RequestAsync({ Url = LOCAL }) end)
req("request unknown key", function() return HttpService:RequestAsync({ Url = LOCAL, Foo = 1 }) end)
req("request ftp", function() return HttpService:RequestAsync({ Url = "ftp://127.0.0.1:1/" }) end)
req("request roblox.com", function() return HttpService:RequestAsync({ Url = "https://www.roblox.com/" }) end)
req("request localhost", function() return HttpService:RequestAsync({ Url = "http://localhost:1/" }) end)

-- RequestAsync: Method.
local function method(m, body)
    return function()
        return HttpService:RequestAsync({ Url = LOCAL, Method = m, Body = body })
    end
end
req("method GET", method("GET"))
req("method get", method("get"))
req("method POST", method("POST", "x"))
req("method PATCH", method("PATCH", "x"))
req("method OPTIONS", method("OPTIONS"))
req("method FOO", method("FOO"))
req("method with space", method("BAD METHOD"))
req("method empty", method(""))
req("method number", method(5))
req("method table", method({}))
req("GET with Body", method("GET", "x"))
req("get with Body", method("get", "x"))
req("HEAD with Body", method("HEAD", "x"))
req("GET with empty Body", method("GET", ""))
req("POST without Body", method("POST"))
req("POST Body number", method("POST", 5))
req("POST Body table", method("POST", {}))

-- RequestAsync: Headers.
local function headers(h)
    return function()
        return HttpService:RequestAsync({ Url = LOCAL, Headers = h })
    end
end
req("Headers string", headers("x"))
req("Headers array", headers({ "a" }))
req("Headers number value", headers({ X = 5 }))
req("Headers table value", headers({ X = {} }))
req("Headers User-Agent", headers({ ["User-Agent"] = "x" }))
req("Headers Roblox-Id", headers({ ["Roblox-Id"] = "x" }))
req("Headers Content-Length", headers({ ["Content-Length"] = "5" }))
req("Headers space in name", headers({ ["Bad Name"] = "x" }))
req("Headers newline in value", headers({ X = "a\r\nb" }))

-- RequestAsync: Compress and Timeout.
local function option(key, value)
    return function()
        return HttpService:RequestAsync({ Url = LOCAL, Method = "POST", Body = "x", [key] = value })
    end
end
req("Compress Gzip", option("Compress", Enum.HttpCompression.Gzip))
req("Compress None", option("Compress", Enum.HttpCompression.None))
req("Compress name", option("Compress", "Gzip"))
req("Compress true", option("Compress", true))
req("Compress other enum", option("Compress", Enum.Material.Wood))
req("Timeout 1", option("Timeout", 1))
req("Timeout 0", option("Timeout", 0))
req("Timeout -1", option("Timeout", -1))
req("Timeout 1000", option("Timeout", 1000))
req("Timeout string", option("Timeout", "x"))

-- Run them all at once; log in order.
local results = {}
local left = #jobs
for i, job in jobs do
    task.spawn(function()
        results[i] = settle(job.f)
        left -= 1
    end)
end
for _ = 1, 100 do
    if left == 0 then
        break
    end
    task.wait(0.1)
end
for i, job in jobs do
    log(job.label, results[i] or "pending")
end

-- Calling them as functions rather than methods.
log("get dot", settle(function() return HttpService.GetAsync(LOCAL) end))
log("request on Workspace", settle(function() return HttpService.RequestAsync(workspace, { Url = LOCAL }) end))
log("HttpEnabled after", HttpService.HttpEnabled)
