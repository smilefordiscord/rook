# Luau libraries

Checked against Studio 0.748 on 2026-11-23, where noted; the rest is
from the creator docs and Luau's own reference.

What the Luau Roblox runs gives scripts besides Roblox's own API (test:
`rook-lua`'s `tests/scripting/luau_libraries.rs`). Rook runs Luau 0.740
through `mlua`, with every library Roblox opens:

- `buffer`: fixed-size byte buffers (`create`, `fromstring`, `tostring`,
  `len`, the typed reads and writes, `readstring`, `copy`, `fill`);
  `typeof` gives `buffer` and `tostring` `buffer: 0x` and its address
  (Confirmed: Studio 0.748).
- `vector`: `vector.create` makes the same value as `Vector3.new`
  (Confirmed: Studio 0.748, `typeof` gives `Vector3` and they compare
  equal), with `magnitude`, `normalize`, `dot`, `cross` and the rest.
- `bit32`, `utf8`, `string.pack`/`unpack`, `table.freeze`/`isfrozen`/
  `clone`/`find`, `coroutine.close`, `math.clamp`/`sign`/`round`/`lerp`/
  `map`, string interpolation and `if` expressions.

Roblox's own additions to them (`utf8.nfcnormalize` and
`utf8.nfdnormalize`, the `task` library, `SharedTable`) are Rook's; see
[waiting](waiting.md) and [Parallel Luau](parallel-luau.md).
