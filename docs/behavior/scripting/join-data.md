# Join data

Checked against Studio 0.748 on 2026-11-22, from the creator docs only: nothing here is measured in Studio yet.

What Roblox's servers hand a game about how a player arrived:
`Player:GetJoinData` and `TeleportService:GetLocalPlayerTeleportData`.
The code is `rook-lua`'s `api/methods/web/teleport.rs`; the test is
`rook-lua`'s `tests/scripting/join_data.rs`.

- Each of `GetJoinData`'s fields (`SourceGameId`, `SourcePlaceId`,
  `ReferredByPlayerId`, `Members`, `TeleportData`, `LaunchData`,
  `GameJoinContext`) is present only for the way of joining it describes:
  a teleport, an invite, a share link or an attribution link (Confirmed:
  creator-docs Player:GetJoinData, JoinSource). Every player in a Rook
  session joined it directly, so the table is empty (Inference).
- `GetLocalPlayerTeleportData` returns the data the local player was
  teleported with (Confirmed: creator-docs TeleportService); no teleport
  brought anyone to a Rook session, so it returns nil, on a server too
  (Inference: the docs say it's for clients, not what a server gets).

## Needs a Studio check

Whether a Studio playtest's player gets any fields, and what a server's
`GetLocalPlayerTeleportData` does.
