# Badges and groups

Checked against Studio 0.748 on 2026-11-22, from the creator docs only: nothing here is measured in Studio yet.

BadgeService and GroupService, and a player's group members, answered
by the session or by the endpoint configured for their domain
([web endpoints](../../design/web-endpoints.md)). The code is
`rook-lua`'s `api/methods/web/badges.rs` and `api/methods/web/groups.rs`;
the tests are `rook-lua`'s `tests/scripting/badges_and_groups.rs`.

## Badges

- `AwardBadgeAsync(user, badge)` (and the deprecated `AwardBadge`)
  returns whether it awarded the badge. Only a server script awards,
  only to a player in the game, and not a badge they have (Confirmed:
  creator-docs BadgeService). Rook returns false for a user who isn't
  one of the session's players or already has the badge (Inference: the
  docs list the conditions, not what a failed award returns), and
  raises on a client (Inference: the wording is Rook's).
- With no `badges` endpoint, every badge exists and is enabled, and a
  badge is had by the players the session awarded it to, until the run
  ends (Approximation: Roblox checks the badge belongs to the game and
  is enabled, and keeps awards for good). `UserHasBadgeAsync` (and the
  deprecated `UserHasBadge`) and `CheckUserBadgesAsync` answer from
  those awards.
- `CheckUserBadgesAsync(user, badgeIds)` returns the ones the user has,
  and takes at most 10 (Confirmed: creator-docs); more raises an error
  worded by Rook (Inference).
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

What a failed `AwardBadgeAsync` returns or raises (a user not in the
game, a badge they have, a client); the errors' wording; and the order of
`GetGroupInfoAsync`'s roles.
