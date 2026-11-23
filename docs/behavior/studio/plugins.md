# Plugins

Checked against Studio 0.748 on 2026-11-23.

How Rook runs Studio plugins in a place opened for editing, for an editor
built on Rook to host. Rook draws nothing of its own here. The editor
places the plugins' dock widgets, shows their toolbar buttons and draws
the place, and plugins do the rest. Each rule is tagged Confirmed
(creator-docs), Measured, Inference or Approximation. Measured rules come
from Luau run in Studio 0.748's Edit DataModel through Studio's MCP, which
runs with about a plugin's capability but has no `plugin` of its own, so
what needs a Plugin instance (toolbars, dock widgets, activation, the
plugin's mouse, settings) stays creator-docs or Inference.

Code: the state plugins share in `rook-lua/src/studio.rs`, the members in
`rook-lua/src/api/methods/studio/`, what an editor calls in
`rook-lua/src/calls/studio.rs` and `rook-engine/src/studio.rs`, and the
dock widgets' layout in `rook-gui` (`order.rs`, `layout/pass.rs`). Tests
in `rook-engine/src/tests/studio/plugins.rs`.

## Edit mode

`DataModel::edit_place` loads a place as for playing, then puts it in
Edit mode:

- No place script starts and nothing simulates. Plugins' threads resume.
- Of RunService's frame events, `Heartbeat`, `RenderStepped` and
  `PreRender` fire, in that order, and `BindToRenderStep` functions run.
  `Stepped`, `PreSimulation`, `PostSimulation` and `PreAnimation` don't
  (Measured: none fired in half a second). Studio fired `Heartbeat` 120
  times in that half second against 30 `RenderStepped`, four a frame;
  Rook fires it once a frame (Approximation).
- StarterGui's ScreenGuis show in the view, as a player's would, while
  `StarterGui.ShowDevelopmentGui` is on (Confirmed: creator-docs
  StarterGui).
- Edits are recorded for [undo](history.md).

RunService answers by mode (Confirmed: creator-docs RunService, the table
under `IsStudio`; the Edit column Measured):

| | Live game | Edit | Run | Play, server | Play, client |
|---|---|---|---|---|---|
| `IsStudio` | false | true | true | true | true |
| `IsEdit` | false | true | false | false | false |
| `IsRunMode` | false | false | true | false | false |
| `IsRunning` | true | false | true | true | true |
| `IsServer` | server | true | true | true | false |
| `IsClient` | client | true | true | false | true |

## Loading plugins

A plugin is a Plugin instance, outside the DataModel (its `Parent` is
nil), holding what the plugin's model file holds. Its enabled Scripts
start in tree order. Each script and module inside it sees `plugin` (the
Plugin) and runs with the Plugin capability. It reaches `PluginSecurity`
members, and game scripts get "lacking capability Plugin" for them
(Confirmed: creator-docs Plugin). The capability goes with the code: a
place's ModuleScript a plugin requires runs as game code (Inference).

## Toolbars

`Plugin:CreateToolbar(name)` makes a PluginToolbar in the plugin, and
`PluginToolbar:CreateButton(id, tooltip, icon, text)` makes a
PluginToolbarButton named by its id in the toolbar. Where Studio keeps
them isn't visible to scripts, so that is Inference. The editor shows the
text (its id when the text is empty), the tooltip and the icon, and
`Click` fires when it's pressed. `SetActive` shows it pressed.

## Activation and the plugin's mouse

One plugin is activated at a time. `Activate` deactivates the last one,
whose `Deactivation` fires, and `Deactivate` ends it (Confirmed:
creator-docs Plugin:Activate). `GetMouse` gives the plugin's PluginMouse,
the same one each call.

### The plugin's mouse

The activated plugin's PluginMouse hears the view. `Move`,
`Button1Down`, `Button2Down` and the wheel fire only for input no GUI
took, so a dock widget's clicks don't reach it. `Button1Up` and
`Button2Up` always fire, so a drag ending over a widget ends. Its `Hit`,
`Target`, `UnitRay` and the rest follow the cursor as the PlayerMouse's
do. Its own `TargetFilter` isn't applied (Approximation).

## Dock widgets

`Plugin:CreateDockWidgetPluginGui(id, info)` makes a DockWidgetPluginGui
named by its id in PluginGuiService (Confirmed: creator-docs
PluginGuiService), enabled as `info.InitialEnabled` says. The editor gives
each widget a rectangle of its window (`DataModel::set_widget_rect`).
The widget lays out there as a ScreenGui does on the screen, clipped to
it, and takes the mouse there. Shown widgets draw over the place's GUI.
`AbsolutePosition` inside a widget counts from the window's top left, not
the widget's (Approximation). `GetRelativeMousePosition` counts from the
widget's. A second widget with an id already in use is an error
(Inference).

`PluginGui:BindToClose(fn)` makes the widget's close button call `fn`
instead of hiding the widget (Confirmed: creator-docs).

### DockWidgetPluginGuiInfo

`DockWidgetPluginGuiInfo.new(dock, enabled, overrideRestore, floatX,
floatY, minWidth, minHeight)` holds where the widget starts and its
sizes (Measured unless noted):

- Arguments not passed are Right, false, false and 0. One passed must
  have its type, `nil` included, or it errors
  `DockWidgetPluginGuiInfo.new first argument expects InitialDockState
  input`, `... second arg should be a boolean for InitialEnabled.`,
  `... third arg should be a boolean for
  InitialEnabledShouldOverrideRestore.`, and `... fourth argument expects
  an int for FloatingXSize.` (fifth, sixth, seventh for `FloatingYSize`,
  `MinWidth`, `MinHeight`).
- The dock is an `Enum.InitialDockState` item or its value as a number,
  truncated (2.5 is Left); not its name, another enum's item or a value
  it lacks.
- Booleans must be booleans (not `1` or a string). Sizes must be
  numbers, not numeric strings, and are truncated toward zero (12.7 is
  12, -2.7 is -2); anything outside a 32-bit integer, NaN and infinity
  included, is -2147483648.
- It has no readable member: reading any is `'MinWidth' is not a valid
  member of DockWidgetPluginGuiInfo`, writing one the same without the
  quotes.
- `tostring` gives `InitialDockState:Right InitialEnabled:0
  InitialEnabledShouldOverrideRestore:0 FloatingXSize:0 FloatingYSize:0
  MinWidth:0 MinHeight:0`, the booleans as 0 and 1.
- Two made from the same arguments are `==`.

## Settings

`Plugin:SetSetting(key, value)` keeps any value JSON can hold, per plugin
by its name, and `GetSetting` reads it back, `nil` once unset (Confirmed:
creator-docs Plugin:SetSetting). The editor loads and saves them between
runs.

## The selection

The Selection service's `Get`, `Set`, `Add` and `Remove` keep instances in
the order selected, without duplicates (Measured: `Set({a, b, a})` selects
a and b; `Set({b, a})` puts b first; `Add` appends). Only the DataModel and
what's in it can be selected: `Set` and `Add` leave out an instance
outside it (Measured). `Get` returns a new table each call. A list
argument that isn't a table errors `Expected table argument, got nil`
(naming the Lua type), and an entry that isn't an instance `Unable to
cast value to Object` (Measured).

`SelectionChanged` fires deferred, with no arguments, once for each call
that fires it (Measured):

- `Set` fires unless the selection was empty and the list is empty. It
  fires even when nothing changes: setting what's already selected, or a
  list of instances outside the DataModel on an empty selection.
- `Add` fires only when it selects something new.
- `Remove` fires on every call, even one that removes nothing.

An instance that leaves the DataModel leaves the selection at once: `Get`
right after `Parent = nil` leaves it out. `SelectionChanged` then fires
once for each selected instance that left, so a selected folder's two
selected children leaving fire it twice. Moving within the DataModel
(Workspace to ServerStorage) keeps the selection, a rename doesn't fire,
and an instance put back isn't selected again (Measured).

## ReflectionService

What Studio's Properties window and plugins build their views from
(Measured with `GetPropertiesOfClass("Part")`, `GetClass` and an unknown
class name):

- `GetPropertiesOfClass(className, filter)` returns a table for each
  property, the class's own first, then each superclass's, nearest
  first; `filter.ExcludeInherited` keeps only its own. Each holds `Name`,
  `Owner` (the class declaring it), `Display.Category` (the Properties
  window's group: Anchored is in Part, CFrame in Transform, Material in
  Appearance), `Permits.Read` and `Permits.Write` (absent when scripts
  can't), `Serialized`, and `Type` with `EngineType`, `ScriptType` and,
  for an enum, `EnumType` (`bool`/`boolean`, `CoordinateFrame`/`CFrame`,
  `Enum`/`EnumItem`). Write-only properties (`Part.shap`) aren't listed;
  hidden and unscriptable ones are, so Part's own are `Shape` and
  `shape`.
- `Permits` values are SecurityCapabilities; Rook gives the capability's
  name as a string (Approximation).
- `GetClass(name)` gives `Name`, `Superclass` and `Subclasses`, and in
  Studio also `Display.Category`, `Permits.New` and `Serialized`, which
  Rook leaves out.
- An unknown class name gives `nil` from both.

## Saving

A place saves the DataModel's `Archivable` children except Studio's own:
CoreGui, PluginGuiService, Selection, ChangeHistoryService and the
network services. That places saved by Studio have none of these is
Inference.

## Not implemented

The plugin members Rook doesn't implement are listed in
[stubs](../../guide/stubs.md#studio-plugins).
