-- BadgeService, GroupService and Player's group members with arguments
-- they can't take (docs/behavior/scripting/badges-and-groups.md): what each
-- raises for missing, mistyped and uncastable users, badges, groups and
-- badge lists, and CheckUserBadgesAsync's limit of 10. Nothing here asks a
-- question Roblox's web services answer: every call has a bad argument,
-- except the badge lists of user 1 for badges that don't exist (empty and
-- over the limit), whose result is logged only as its length.
--! player
local BadgeService = game:GetService("BadgeService")
local GroupService = game:GetService("GroupService")
local player = game:GetService("Players"):GetPlayers()[1]

local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            local v = r[i]
            s[#s + 1] = if type(v) == "table" then "table #" .. #v else tostring(v)
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
local function on(service)
    return function(name, ...)
        local args = table.pack(...)
        return within(function()
            return service[name](service, table.unpack(args, 1, args.n))
        end)
    end
end
local badge, group = on(BadgeService), on(GroupService)

-- Badges: the user, then the badge.
for _, name in { "AwardBadgeAsync", "AwardBadge", "UserHasBadgeAsync", "UserHasBadge" } do
    log(name .. " no args", badge(name))
    log(name .. " nil user", badge(name, nil, 1))
    log(name .. " word user", badge(name, "abc", 1))
    log(name .. " table user", badge(name, {}, 1))
    log(name .. " Workspace user", badge(name, workspace, 1))
    log(name .. " nil badge", badge(name, 1, nil))
    log(name .. " word badge", badge(name, 1, "abc"))
    log(name .. " table badge", badge(name, 1, {}))
end
log("AwardBadgeAsync Player as user, word badge", badge("AwardBadgeAsync", player, "abc"))

-- CheckUserBadgesAsync: the list.
log("CheckUserBadgesAsync no list", badge("CheckUserBadgesAsync", 1))
log("CheckUserBadgesAsync string list", badge("CheckUserBadgesAsync", 1, "1"))
log("CheckUserBadgesAsync number list", badge("CheckUserBadgesAsync", 1, 1))
log("CheckUserBadgesAsync word in list", badge("CheckUserBadgesAsync", 1, { "abc" }))
log("CheckUserBadgesAsync table in list", badge("CheckUserBadgesAsync", 1, { {} }))
log("CheckUserBadgesAsync dictionary", badge("CheckUserBadgesAsync", 1, { a = 1 }))
log("CheckUserBadgesAsync nil user", badge("CheckUserBadgesAsync", nil, { 1 }))
log("CheckUserBadgesAsync word user", badge("CheckUserBadgesAsync", "abc", { 1 }))
log("CheckUserBadgesAsync empty", badge("CheckUserBadgesAsync", 1, {}))
local eleven = {}
for i = 1, 11 do
    eleven[i] = i
end
log("CheckUserBadgesAsync 11", badge("CheckUserBadgesAsync", 1, eleven))
log("GetBadgeInfoAsync word", badge("GetBadgeInfoAsync", "abc"))
log("GetBadgeInfoAsync nil", badge("GetBadgeInfoAsync", nil))

-- GroupService.
log("GetGroupInfoAsync no args", group("GetGroupInfoAsync"))
log("GetGroupInfoAsync word", group("GetGroupInfoAsync", "abc"))
log("GetGroupInfoAsync table", group("GetGroupInfoAsync", {}))
log("GetGroupsAsync no args", group("GetGroupsAsync"))
log("GetGroupsAsync word", group("GetGroupsAsync", "abc"))
log("GetGroupsAsync Workspace", group("GetGroupsAsync", workspace))
log("GetRolesInGroupAsync nil user", group("GetRolesInGroupAsync", nil, 1))
log("GetRolesInGroupAsync word group", group("GetRolesInGroupAsync", 1, "abc"))
log("GetAlliesAsync word", group("GetAlliesAsync", "abc"))
log("GetEnemiesAsync word", group("GetEnemiesAsync", "abc"))
log("PromptJoinAsync word", group("PromptJoinAsync", "abc"))
log("PromptJoinAsync nil", group("PromptJoinAsync", nil))

-- Player's group members.
local function member(name, ...)
    local args = table.pack(...)
    return within(function()
        return player[name](player, table.unpack(args, 1, args.n))
    end)
end
for _, name in { "GetRankInGroup", "GetRankInGroupAsync", "GetRoleInGroup", "GetRoleInGroupAsync", "IsInGroup", "IsInGroupAsync" } do
    log(name .. " no args", member(name))
    log(name .. " word", member(name, "abc"))
    log(name .. " table", member(name, {}))
end
log("GetRankInGroup on Workspace", caught(function() return player.GetRankInGroup(workspace, 1) end))
