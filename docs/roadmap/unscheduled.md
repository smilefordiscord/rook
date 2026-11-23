# Unscheduled

Good ideas without a milestone yet, and decisions still open.
Each needs the milestones it builds on before it can start.

## Ideas

- **Studio**: `rook-studio`, a place editor (Explorer and Properties from
  the reflection database; moving, scaling and rotating parts; play
  testing; saving files Roblox Studio opens unchanged). The engine side
  is in: Edit mode, plugins with their toolbars, dock widgets and mouse,
  the Selection, undo through ChangeHistoryService, and saving
  ([plugins](../behavior/studio/plugins.md),
  [change history](../behavior/studio/history.md)); what plugins can't
  reach yet is in [stubs](../guide/stubs.md#studio-plugins). The editor
  itself remains.
- **Rook extensions**: opt-in features beyond what Roblox allows, behind
  the explicit profile from the [architecture](../design/architecture.md#extensions),
  off by default:
  - WebSockets available to any script (needs M10).
  - EditableImage and EditableMesh without platform gating.
  - Restoring embedded legacy textures that modern Roblox loads as empty.

## Undecided

