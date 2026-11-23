# Teleports

Checked against Studio 0.748 on 2026-11-23.

How TeleportService moves players between the places of a game, and what
they carry. Sources: creator-docs TeleportService, TeleportOptions,
TeleportAsyncResult, `Enum.TeleportResult`, `Enum.TeleportState`,
Player:GetJoinData and the "Teleport between places" guide, and Studio
0.748's playtest of an unpublished place, on its server and its client
(conformance cases `scripting/teleport_args` and
`scripting/client_teleport`). What the playtest showed is marked
Confirmed. The design is M14 step 4's on the
[roadmap](../../roadmap/m14-scripting-completeness.md).

Studio refuses to teleport from an unpublished place, so the playtest
shows how each method takes its arguments and refuses, never a teleport
that goes. Its Test mode of a published game teleports with
`TeleportAsync` only: the destination opens as a new test server, `JobId`
and `PrivateServerId` stay empty, and the legacy methods and
`GetPlayerPlaceInstanceAsync` don't work. Rook runs places as a live game
does, so past the checks it follows the docs' live behavior for all of
them. Where neither gives the words or order, Rook's are marked
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

- Every method casts its arguments first, last argument first (Confirmed
  for `TeleportAsync`, whose call without arguments is `Argument 2 missing
  or nil`; Inference for the others), as Roblox's engine methods cast:
  a place id that isn't a number is `Unable to cast string to int64`, a
  user id `Unable to cast string to User`, a missing one `Argument 1
  missing or nil`. A players list must be an array (`{}` included):
  anything else, a Player not in a table or a dictionary, is `Unable to
  cast value to Objects`, and an item that isn't an Instance `Unable to
  cast value to Object`. An options, player or loading-screen argument
  that isn't an Instance is `Unable to cast value to Object`; nil is
  none. A client is refused after the casts: a client's
  `ReserveServerAsync("abc")` is the cast error.
- `TeleportAsync(placeId, players, options)` is server-only: a client's
  call is `TeleportUnknown must be called from a Server` (Confirmed). Then
  options setting more than one of `ReservedServerId`, `VipServerId`,
  `ReservedServerAccessCode`, `ShouldReserveServer` and
  `ServerInstanceId` raise `Can only specify one of ReservedServerId,
  VipServerId, ReservedServerAccessCode, ShouldReserveServer, or
  ServerInstanceId.`, and then a place id of 0 or below `Invalid
  placeId.` (Confirmed, in that order: a Part as options, an empty list,
  Workspace in it and 51 players all give `Invalid placeId.` for place
  0). Unpublished, a place is 0, so a teleport to its own place raises
  there. After the place id, with the docs' names for each failure
  (Inference: their words and order): `TeleportOptions not of correct
  type`, `Players empty`, `List of players instances is incorrect`
  (anything but Players in it), and Rook's `Cannot teleport more than 50
  players at once` (the limit is the docs').
- It then yields until the session has routed the teleport, at the end of
  the server's frame, and returns a `TeleportAsyncResult` when options were
  given (its `PrivateServerId` for a reserved destination, and
  `ReservedServerAccessCode` only for a legacy access-code teleport), or
  nothing without them (creator-docs). Inference: the call returns before
  the players leave.
- The legacy methods return at once. `Teleport`, `TeleportToSpawnByName`
  and `TeleportToPlaceInstance` move one player. On a server, a call
  without a player returns doing nothing, before the place is checked
  (Confirmed for `Teleport(0)`; Inference: it moves no one); then a place
  id of 0 or below is `Invalid placeId.` (Confirmed), and a player that
  isn't a Player `<method>: player argument must be a Player object`
  (Inference). On a client they move the local player, the only one it
  may name: a place id of 0 or below is `Cannot teleport to invalid place
  id. Aborting teleport.` (Confirmed for `Teleport`, checked before the
  player; Inference for the other two), and another player the same
  `player argument` error. `TeleportToPrivateServer` and
  `TeleportPartyAsync` are server-only (a client's are
  `TeleportToPrivateServer can only be called from the server` and
  `TeleportPartyAsync must be called from a Server`, Confirmed), check the
  place id as `TeleportAsync` does (Confirmed), then their players.
  `TeleportPartyAsync` waits and returns the destination's `JobId`. A
  LocalScript's teleport goes to its server, which carries it out as its
  own.
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
  `PrivateServerId`; nothing starts until a teleport uses the code.
  Inference: they check the place id as a teleport does. Rook's
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
`GameNotFound` (a place above 0 not in the universe, as every one is
without a `universe.toml`), `GameEnded`, `GameFull`,
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
  data is the server's. Confirmed: the playtest player's join data is an
  empty table on its server and its client, a new one each call.
- `TeleportOptions:SetTeleportData` keeps a copy made as a bindable's
  arguments are ([bindables](bindables.md), Confirmed): a value that
  isn't a table is kept as it is, functions and Instances included; a
  table is copied, keeping only an array's array part
  (`{1, 2, x = 3}` reads `{1, 2}`); a boolean key raises `Invalid table
  key type used` and a table that holds itself `tables cannot be cyclic`.
  `GetTeleportData` returns a new copy each time, nil before any is set.
  Nil, or nothing, is `Argument 1 missing or nil`. TeleportOptions'
  defaults, and their properties' casts, are Studio's (Confirmed): empty
  strings and `ShouldReserveServer` false, `ServerInstanceId = 5` reads
  `"5"`, `ServerInstanceId = nil` raises.
- The teleport sends the data (and the legacy methods' `teleportData`,
  copied the same way) as remote arguments are: instances are removed
  with an error in the output, as are functions (creator-docs). The
  arriving client reads it with `GetLocalPlayerTeleportData` (nil on a
  server and on a client that didn't arrive by teleport, Confirmed).
- `SetTeleportGui` takes any Instance, whatever its class (a Part, a
  Frame, a BillboardGui), on a server or a client, and nil, which clears
  it (Confirmed: none raise; Inference: nil clears). A value that isn't an
  Instance is `Unable to cast value to Object`, and nothing at all
  `Argument 1 missing or nil` (Confirmed). The creator docs call it the
  loading ScreenGui; whatever was set is what goes.
- `SetTeleportGui`'s ScreenGui (or a legacy method's
  `customLoadingScreen` on a client) goes with the player: copied with its
  descendants, references outside it cleared (creator-docs), put in the new
  client's CoreGui before its scripts start. `GetArrivingTeleportGui`
  returns it; `LocalPlayerArrivedFromTeleport(gui, data)` fires once the
  client's scripts have started, then the GUI leaves CoreGui unless a
  handler moved it. Inference: scripts inside it don't run, as the guide
  says of the start place's.
- `SetTeleportSetting`'s values stay with the client across teleports;
  `GetTeleportSetting` reads them (creator-docs), nil for a key never set.
  Both work on a server too; a value is copied as teleport data is, and a
  nil value is `Argument 2 missing or nil` (Confirmed).
- `TeleportToSpawnByName`'s spawn name picks the arriving character's
  first SpawnLocation among those it may use; Inference: with none of
  that name, any of them.

## Lookups

`GetPlayerPlaceInstanceAsync(userId)` is server-only and waits for the
session, then returns `(currentInstance, "", placeId, jobId)` for a user
in a running server of the game, or raises `User <id> is not playing this
game` (Rook's words) for one in none.
