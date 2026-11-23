# Leaving and kicks

Checked against Studio 0.748 on 2026-11-23, by reading its CoreScripts; nothing here is measured yet.

How a player leaves a game, and `Player:Kick` (in 1,237 archive files).
Rook's play session holds every client in one process
([replication](../replication.md)), so a client's connection ends there,
in `rook_engine`'s session host; what leaving does to the server's
DataModel is `rook_lua`'s `characters::leaving`.

## Player:Kick

From creator-docs (Player:Kick, Players.PlayerRemoving,
Enum.PlayerExitReason):

- `Kick(message)` disconnects the player and shows them `message`, or a
  default notice without one.
- A server's message longer than 500 bytes is dropped, and the player sees
  the default notice instead.
- In a LocalScript only the local player can be kicked. Kicking another
  player there raises `Cannot kick a non-local Player from a LocalScript`
  (Inference: the text isn't measured).

Inference, to check in Studio: the call returns at once with the player
still in Players, and the player leaves once the session handles the kick,
before the frame's changes go out. A LocalScript's kick ends the client's
own connection; the server sees it leave with reason `Unknown`.

## Leaving

- `Players.PlayerRemoving` fires with the player and an
  `Enum.PlayerExitReason` (`CreatorKick` for a kick) while the player is
  still in Players, before Players' `ChildRemoved` (creator-docs). Under
  Deferred signals its handlers run before the player is taken out of
  Players too, so they see what Immediate handlers see (Inference: what
  the creator docs promise them).
- Inference: then the character goes as it does on respawning
  (`CharacterRemoving`, then it's destroyed), `Character` is cleared, and
  the Player is parented to nil rather than destroyed, as
  `Workspace.PlayerCharacterDestroyBehavior`'s Disabled does
  (creator-docs). Rook parents it to nil whatever that property says.
- The other clients see the Player and its character go, as any removal
  replicates.
- Inference: `LoadCharacter` on a player that left raises `LoadCharacter
  can only be called when Player is in the world` and spawns nothing.

## The kicked client

- Its connection ends: it gets nothing more from the server and sends
  nothing more.
- Rook stops running the place there: scripts, physics and the clock stop,
  and the world stays drawn as it was. Hovering and resizing the window
  fire nothing; the prompt is centered on the window's new size. Inference: a Roblox client may keep
  running its LocalScripts and sounds after a kick; unchecked.

## The disconnect prompt

Roblox's `Connection` CoreScript shows it for a kick
(`Enum.ConnectionError.DisconnectLuaKick`, 267), built by its
`ErrorPrompt` module; read from Studio 0.748's
`ExtraContent/scripts/CoreScripts`:

- Title `Disconnected`; the message `You have been kicked by this
  experience or its moderators.`, or with a message `... Moderation
  message:  <message>` (two spaces), the message cut to its first 200
  bytes (`MaxKickMessageLength`); then a line `(Error Code: 267)`
  (English strings from `ExtraContent/translations`).
- One button, `Leave`, which leaves the game. Reconnecting isn't offered
  after a kick.
- The prompt takes the input (the game behind gets none) and forces the
  cursor shown.

Rook draws the prompt itself (`rook_gui`'s `draw/overlays/prompt.rs`), so
it shows with or without Roblox's CoreScripts: centered over everything,
the mouse freed, Roblox's arrow cursor, and pressing and releasing the
first mouse button on `Leave` closes the window. Approximation: the layout follows
the frames `ErrorPrompt` builds (title in Source Sans Semibold 25, message
in Source Sans 20 wrapped, a 36-pixel button, 20 pixels of padding), but
its sizes and colors come from Roblox's shared constants, which the plain
CoreScripts don't hold; Rook uses a 400-pixel-wide Flint panel, a Pumice
line and message, and a white button with Slate text. Roblox's button is a
rounded image; Rook's is a plain rectangle. A Studio capture of a kicked
client would settle the look.
