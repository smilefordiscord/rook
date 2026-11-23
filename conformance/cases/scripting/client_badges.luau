-- BadgeService seen from the playtest client (badges-and-groups.md).
--! client
local Players = game:GetService("Players")
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

-- BadgeService.
log("AwardBadgeAsync", within(function() return BadgeService:AwardBadgeAsync(player.UserId, 1) end))
log("AwardBadgeAsync word badge", within(function() return BadgeService:AwardBadgeAsync(player.UserId, "abc") end))
