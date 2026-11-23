# Badges and groups

Checked against Studio 0.748 on 2026-11-23.

What the members do with their arguments, and a client's
`AwardBadgeAsync`, are measured in a Studio playtest of an unpublished
place (conformance cases `scripting/badges_groups_args` and
`scripting/client_badges`); what Roblox's web services answer is from
the creator docs.

BadgeService and GroupService, and a player's group members, answered
by the session or by the endpoint configured for their domain
([web endpoints](../../design/web-endpoints.md)). The code is
`rook-lua`'s `api/methods/web/badges.rs` and `api/methods/web/groups.rs`;
the tests are `rook-lua`'s `tests/scripting/badges_and_groups.rs`.

## Arguments

Confirmed (measured): every member here casts its arguments as Roblox's
reflected methods do, the last argument first, before it does anything
else. A user (`AwardBadgeAsync`'s, `UserHasBadgeAsync`'s and
`CheckUserBadgesAsync`'s first argument, `GetGroupsAsync`'s and
`GetRolesInGroupAsync`'s) that doesn't cast raises `Unable to cast string
to User` (`Array` for a table, `Instance` for an instance); a badge or
group id `Unable to cast string to int64` (`Array`); a missing or nil one
`Argument n missing or nil`, so `AwardBadgeAsync()` raises `Argument 2
missing or nil`. `CheckUserBadgesAsync`'s list that isn't a table raises
`Unable to cast string to Array` (`double` for a number, `Dictionary`
for a table with keys but no first item). The Player members
(`GetRankInGroup`, `GetRoleInGroup`, `IsInGroup` and their `Async`
names) cast their group id the same way, and called with `.` on another
instance raise `Expected ':' not '.' calling member function
GetRankInGroup`.

The members Rook lacks (`GetBadgeInfoAsync`, `GetAlliesAsync`,
`GetEnemiesAsync`, `PromptJoinAsync`) cast their arguments the same way
before raising "not implemented" (Confirmed for the casts).

## Badges

- `AwardBadgeAsync(user, badge)` (and the deprecated `AwardBadge`)
  returns whether it awarded the badge. Only a server script awards,
  only to a player in the game, and not a badge they have (Confirmed:
  creator-docs BadgeService). Rook returns false for a user who isn't
  one of the session's players or already has the badge (Inference: the
  docs list the conditions, not what a failed award returns). On a
  client it returns false once its arguments cast (Confirmed:
  measured).
- With no `badges` endpoint, every badge exists and is enabled, and a
  badge is had by the players the session awarded it to, until the run
  ends (Approximation: Roblox checks the badge belongs to the game and
  is enabled, and keeps awards for good). `UserHasBadgeAsync` (and the
  deprecated `UserHasBadge`) and `CheckUserBadgesAsync` answer from
  those awards.
- `CheckUserBadgesAsync(user, badgeIds)` returns the ones the user has,
  and takes at most 10 (Confirmed: creator-docs); more raise
  `CheckUserBadgesAsync: Cannot query more than 10 badges.`, an id that
  isn't a number `CheckUserBadgesAsync: badgeIds must be numeric.`, and
  an empty list returns an empty one (Confirmed: measured). Inference: a
  string that reads as a number is refused too.
- With an endpoint, ownership is
  `GET /v1/users/{user}/badges/awarded-dates?badgeIds={ids}` and an
  award is Rook's own `POST /v1/users/{user}/badges/{badge}/award` (see
  [web endpoints](../../design/web-endpoints.md)).

## Groups

- `Player:GetRankInGroup`, `GetRoleInGroup` and `IsInGroup`, and their
  `Async` names, answer the player's rank (0 outside the group), role's
  name (`Guest` outside) and membership (Confirmed: creator-docs Player).
- `GroupService:GetRolesInGroupAsync(user, group)` returns
  `{IsMember, Roles}`, each role `{Id, Name, Rank}` (Confirmed:
  creator-docs GroupService). Roblox's web API gives a member's one
  role, so `Roles` holds at most one.
- `GroupService:GetGroupsAsync(user)` returns a table per group the user
  is in: `{Name, Id, EmblemUrl, EmblemId, Rank, Role, IsPrimary,
  IsInClan}`, `IsInClan` always false (Confirmed: creator-docs).
  `EmblemUrl` is empty and `EmblemId` 0 (Approximation: the emblem comes
  from the thumbnails service, which these requests don't ask).
- `GroupService:GetGroupInfoAsync(group)` returns `{Name, Id, Owner =
  {Name, Id}, EmblemUrl, Description, Roles = {{Name, Rank}}}`, with no
  `Owner` for a group without one (Confirmed: creator-docs), roles by
  rank, lowest first (Inference from the docs' example), `EmblemUrl`
  empty as above.
- With no `groups` endpoint no group exists: nobody is in one,
  `GetGroupsAsync` returns an empty array, `GetRolesInGroupAsync`
  `{IsMember = false, Roles = {}}`, and `GetGroupInfoAsync` raises
  (Inference: the wording is Rook's).

## What Rook lacks

`GroupService:GetAlliesAsync`, `GetEnemiesAsync` and `PromptJoinAsync`,
and `BadgeService:GetBadgeInfoAsync`; the rate limits.

## Needs a Studio check

What a failed `AwardBadgeAsync` returns or raises for a user not in the
game or a badge they have, which a published place's web services
answer; the wording of a failed lookup; and the order of
`GetGroupInfoAsync`'s roles.
