# Teleports

Checked against Studio 0.748 on 2026-11-23, by its creator docs only; nothing here is measured in Studio.

How TeleportService moves players between the places of a game, and what
they carry. Sources: creator-docs TeleportService, TeleportOptions,
TeleportAsyncResult, `Enum.TeleportResult`, `Enum.TeleportState`,
Player:GetJoinData and the "Teleport between places" guide. The design is
M14 step 4's on the [roadmap](../../roadmap/m14-scripting-completeness.md).

Studio's Test mode teleports with `TeleportAsync` only: the destination
opens as a new test server, `JobId` and `PrivateServerId` stay empty, and
the legacy methods and `GetPlayerPlaceInstanceAsync` don't work. Rook runs
places as a live game does, so it follows the docs' live behavior for all
of them. Where the docs give no words or order, Rook's are marked
Inference.

## The universe

Roblox keeps a game's places on its website. Rook reads them from
`universe.toml` in the folder of the place opened (a Rook extension of
the file layout, not of behavior):

```toml
game_id = 4410        # game.GameId; 0 if left out

[places]              # PlaceId = file, beside this one
1001 = "Lobby.rbxl"
1002 = "Arena.rbxl"
```

- The opened file's entry gives `game.PlaceId`, and `game_id`
  `game.GameId`, in every place of the game. Without a manifest, or for a
  file it doesn't name, both are 0, as in an unpublished place, and every
  teleport fails as `GameNotFound`.
- Each server Rook starts has a `JobId` in a GUID's form, counted rather
  than random so runs repeat (Roblox's are random). A reserved server's
  `PrivateServerId` is its id and its `PrivateServerOwnerId` 0
  (creator-docs ReserveServerAsync); public servers have both empty and 0.
  A client reports its server's.

## Teleporting

- `TeleportAsync(placeId, players, options)` is server-only. It checks
  its arguments before anything else, with the docs' names for each
  failure as the error (Inference: the words): `Invalid placeId` (below
  0), `Players empty`, `List of players instances is incorrect` (not a
  table, or anything but Players in it), `TeleportOptions not of correct
  type`, `TeleportAsync called from Client`, `Incompatible Parameters`
  (more than one of `ReservedServerId`, `VipServerId`,
  `ReservedServerAccessCode`, `ShouldReserveServer` and
  `ServerInstanceId`), and Rook's `Cannot teleport more than 50 players at
  once` (the limit is the docs').
- It then yields until the session has routed the teleport, at the end of
  the server's frame, and returns a `TeleportAsyncResult` when options were
  given (its `PrivateServerId` for a reserved destination, and
  `ReservedServerAccessCode` only for a legacy access-code teleport), or
  nothing without them (creator-docs). Inference: the call returns before
  the players leave.
- The legacy methods return at once. `Teleport`, `TeleportToSpawnByName`
  and `TeleportToPlaceInstance` move one player: on a client the local
  player (the only one it may name), on a server the one given (another
  value raises `<method>: player argument must be a Player object`,
  Inference). `TeleportToPrivateServer` and `TeleportPartyAsync` are
  server-only; `TeleportPartyAsync` waits and returns the destination's
  `JobId`. A LocalScript's teleport goes to its server, which carries it
  out as its own.
- A player who leaves for another server leaves its server as any player
  leaves ([leaving](../characters/leaving.md)): `PlayerRemoving` with
  `Enum.PlayerExitReason.Unknown` (Inference: the enum names no teleport).

## Which server

As Roblox's matchmaking chooses (creator-docs TeleportOptions):

- No routing: a public server of the place with room for the group
  (`Players.MaxPlayers`), the oldest first (Inference: Roblox matches by
  the first player), or a new one.
- `ServerInstanceId` and `TeleportToPlaceInstance`: that `JobId`;
  `GameEnded` if no such server of the place runs, `GameFull` if it has
  no room.
- `ReservedServerId`: the reserved server with that id, started by the
  first teleport using it.
- `ShouldReserveServer`: a new reserved server, whose access code the
  result gives.
- `ReserveServer` and `ReserveServerAsync` give an access code and the
  `PrivateServerId`; nothing starts until a teleport uses the code. Rook's
  codes name their place and server (Roblox's are opaque), so a code used
  for another place fails as `Unauthorized`.
- `VipServerId`: Rook has no VIP servers; the teleport fails as
  `Unauthorized`.
- `TeleportPartyAsync` and group `TeleportAsync` route the group once,
  to one server.

A teleport moves the players to a running server, or starts the place's
server from the universe's file. While the player's destination reads and
loads its place on its own thread, its old client keeps running; then the
window switches to the new client and the old server, empty, closes on
its own thread. Several servers held in one process (tests) keep running
empty (Approximation: Roblox closes an empty server once its
`BindToClose` callbacks have run). Every host routes and sends players
off the same way, in `rook-engine`'s `session/router.rs`; only where a
destination's server runs differs.

## Failures

A teleport that can't go fires `TeleportService.TeleportInitFailed`
(player, `Enum.TeleportResult`, message, placeId, options) once per
player, on the server and on that player's client, and the players stay
(creator-docs). The options are new ones that retry the same destination:
the original routing field and teleport data. Results Rook gives:
`GameNotFound` (a place not in the universe), `GameEnded`, `GameFull`,
`Unauthorized`, `IsTeleporting` (the player is already in another teleport
asked for in the same frame) and `Failure` (the player has no client).
Inference: `TeleportAsync` itself returns then rather than raising, and
the messages are Rook's own words.

A destination that fails to load after the player left (its file can't be
read or loaded, or the player can't join it) ends the old client with a
`Teleport Failed` prompt and a Leave button, Rook's own.

## Player.OnTeleport

Fires on the client for its local player (Inference: not on the server):
`RequestedFromServer` (for a server's teleport), `Started`,
`WaitingForServer` and `InProgress` before it leaves, or `Failed` before
`TeleportInitFailed` (Inference: which states fire and their order). Its
place id is the destination's, its spawn name the one asked for.

## What carries over

- `Player:GetJoinData()` on the destination's server (creator-docs): for a
  server's teleport, `SourcePlaceId`, `SourceGameId`, `TeleportData` and,
  for a group, `Members` (the group's user ids); `LaunchData` from the
  player's `--launch-data` on its first join. A client's own legacy
  teleport gives none of these. A client's `GetJoinData` is empty: join
  data is the server's.
- Teleport data (`TeleportOptions:SetTeleportData`, the legacy methods'
  `teleportData`) is copied as remote arguments are; instances are
  removed with an error in the output (creator-docs). The arriving client
  reads it with `GetLocalPlayerTeleportData` (nil on a server).
- `SetTeleportGui`'s ScreenGui (or a legacy method's
  `customLoadingScreen` on a client) goes with the player: copied with its
  descendants, references outside it cleared (creator-docs), put in the new
  client's CoreGui before its scripts start. `GetArrivingTeleportGui`
  returns it; `LocalPlayerArrivedFromTeleport(gui, data)` fires once the
  client's scripts have started, then the GUI leaves CoreGui unless a
  handler moved it. Inference: scripts inside it don't run, as the guide
  says of the start place's.
- `SetTeleportSetting`'s values stay with the client across teleports;
  `GetTeleportSetting` reads them (creator-docs).
- `TeleportToSpawnByName`'s spawn name picks the arriving character's
  first SpawnLocation among those it may use; Inference: with none of
  that name, any of them.

## Lookups

`GetPlayerPlaceInstanceAsync(userId)` is server-only and waits for the
session, then returns `(currentInstance, "", placeId, jobId)` for a user
in a running server of the game, or raises `User <id> is not playing this
game` (Rook's words) for one in none.
