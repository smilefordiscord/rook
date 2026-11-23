# GUI modern UI objects

Checked against Studio 0.748 on 2026-11-23.

[M8](../../roadmap/m8-gui.md) step 7: UICorner, UIStroke, UIGradient,
UIListLayout's flex and `Wraps` with UIFlexItem, UITableLayout,
UIPageLayout, CanvasGroup and Path2D. ViewportFrame and WorldModel are a
later step.

Measured in Studio 0.744 on 2026-10-22 on Baseplate. Layouts and property
reads are conformance cases (`gui/flex`, `gui/table_layout`,
`gui/page_layout`, `gui/corner_radius`), probed first in a 400 × 300 box.
Looks are from `screen_capture` shots of objects on SurfaceGuis
(`LightInfluence` 0, 50 pixels a stud) at FieldOfView 10, 1.92 screen
pixels a canvas pixel (8 for one close-up), read with System.Drawing next
to grey frames of known colors; Rook's screenshots of the same boards
(`rook-player --script`) were compared pixel by pixel. CanvasGroups and
ViewportFrames don't appear in captures at all, background included, so
CanvasGroup was looked at in screenshots the user took.

Code: drawing in `rook-gui`'s `draw/modifiers/corner.rs`, `draw/modifiers/stroke.rs`,
`draw/modifiers/gradient.rs` and `draw/objects/group.rs`, applied in `draw.rs`; shapes and
gradients in `rook-render`'s `shaders/gui/gui_paint.wgsl`, ramps in
`renderer/gui/ramps.rs`, groups' textures in `renderer/gui/groups.rs`;
layouts in `rook-gui`'s `layout/list.rs`, `layout/table.rs` and
`layout/page.rs`; UIPageLayout's methods in `rook-lua`'s
`api/luau/page_layout.luau`. Path2D's points in `rook-gui`'s `path.rs`
(their bytes in `path/codec.rs`), its curve in `path/curve.rs`, drawn in
`draw/objects/path.rs`; its methods in `rook-lua`'s
`api/methods/gui/path.rs`.

## UICorner

- The shape is computed for each screen pixel, not drawn into the canvas
  first: zoomed to 8 screen pixels a canvas pixel the curve stays smooth.
  Across a curved edge one pixel is blended; straight edges stay hard.
- `CornerRadius` is a view of the four corners' radii (`TopLeftRadius`,
  `TopRightRadius`, `BottomRightRadius`, `BottomLeftRadius`, 8 pixels by
  default): setting it sets all four, reading it reads the top left's.
  Files save the four.
- No radius goes past half the shorter side: a scale of 0.5 and an offset
  of 60 both make a 160 × 80 frame a pill. Inference: the scale is of the
  shorter side, and UIScales grow the offset.
- It rounds the background, the image and the text's box; an object with
  a UICorner draws no border (`BorderSizePixel` 6 showed nothing).
- `ClipsDescendants` still clips to the rectangle, not the rounded shape.

## UIStroke

- Border mode, or Contextual on anything but a text object, draws a band
  round the edge, over the object's background, image and text and under
  its children, in its place among siblings.
- `BorderStrokePosition` Outer (the default) lies outside the edge,
  Center across it, Inner inside.
- Without a UICorner, `LineJoinMode` Round rounds the outside corners by
  the band's width outside the edge, Bevel cuts them, Miter leaves them
  square; Inner's outside corners are square. With a UICorner, the band
  follows it: its outside radius is the corner's plus the band's width
  outside the edge, its inside the corner's.
- Its opacity is its own `Transparency`; the object's
  `BackgroundTransparency` doesn't change it (a band shows round a frame
  with `BackgroundTransparency` 1).
- `Thickness` is in pixels and UIScales widen it even at
  `StrokeSizingMode` FixedSize (a UIScale of 2 made a 4-pixel band 8);
  ScaledSize's `Thickness` is a share of the shorter side (0.1 on a
  140 × 80 frame, 8 pixels).
- A UIGradient under the UIStroke colors the band, over the band's own
  box.
- Contextual on a text object outlines the text instead, about
  `Thickness` pixels out, in place of `TextStrokeTransparency`'s outline.
  Rook draws it as the text stroke is drawn, eight copies moved its
  width each way (Inference), so it's a little rougher than Studio's.
- Inference: `BorderOffset` moves the band out by its offset in pixels
  and its scale of the shorter side.

## UIGradient

- `t` runs from 0 to 1 along `Rotation` (clockwise, 0 left to right, 90
  top to bottom) through the box's center, over the length of that line
  inside the box: `min(width / |cos|, height / |sin|)`. Fitted from five
  frames (360 × 120, 120 × 120 and 60 × 200 at 45 degrees, 360 × 100 at 30
  and 135): the slopes agree within 1%. Past the ends the end colors hold.
- `Offset` moves the gradient by that share of the box's size (0.25
  across starts the ramp a quarter of the way in).
- Colors blend in a gamma of 2: halfway from black to white reads as
  √0.5, a quarter as 0.5, exactly. `Transparency` blends in a straight
  line.
- The gradient multiplies the color and opacity of the object's
  background, image and text (a red frame with a yellow-to-cyan gradient
  goes from red to black).
- Rook's screenshots of the same frames land within 4 of Studio's values
  at every point sampled.

## Flex

UIListLayout's `HorizontalFlex` and `VerticalFlex` (whichever is along
`FillDirection` flexes along, the other across), `ItemLineAlignment` and
`Wraps`, and UIFlexItem's `FlexMode`, `GrowRatio`, `ShrinkRatio` and
`ItemLineAlignment`:

- With `Wraps`, an item starts a new line when the line, `Padding` and it
  would pass the area's length; the first item of a line always fits.
  Lines are `Padding` apart too. Each line flexes and aligns on its own.
- Room to spare along a line is shared by grow ratio; room lacking is
  taken by shrink ratio times each item's length. An item its size
  constraints stop keeps that length, and the rest is shared again among
  the others.
- Ratios: a UIFlexItem's FlexMode Grow is (grow 1, shrink 0), Shrink
  (0, 1), Fill (1, 1), Custom its `GrowRatio` and `ShrinkRatio`, None
  (0, 0), even when the list fills. A child without a UIFlexItem is (1, 1)
  when the list's flex along is Fill, (0, 0) otherwise.
- Room still spare: SpaceAround puts half a share at each end, SpaceEvenly
  a share, SpaceBetween none (one item stays at the start); otherwise the
  alignment along places the run.
- Across, a line is as thick as its thickest item. The flex across Fill
  shares the room left among the lines; the Space modes spread them (one
  line: SpaceBetween at the start, the others centered); otherwise the
  lines go where the alignment across puts them.
- In its line, an item sits by its UIFlexItem's `ItemLineAlignment`, or
  the list's when that's Automatic: Start, Center, End or Stretch (to the
  line's thickness). Automatic stretches when the flex across is Fill and
  aligns by the alignment across otherwise, which without flex is the
  old rule.
- `AbsoluteContentSize` is the longest line (the room spreading adds left
  out, grown items counted) by the lines across.
- Children of an item flex changed lay out in its new size.
- An item flex changed the length of that grows across the list by
  `AutomaticSize` is measured again at its new length: a 0 × 0 item
  growing on Y and filling the rest of a 300-wide row beside a 100 × 50
  frame, holding a square as wide as it (`SizeConstraint` RelativeXX), was
  200 × 200, and the line, `AbsoluteContentSize` (300, 200), a row growing
  on Y (200 tall) and the alignment across (the frame centered at 75)
  followed (Studio 0.748, conformance `gui/flex_automatic`).

## UITableLayout

- Its children are rows and their children the cells, each list in
  `SortOrder`. A column is as wide as its widest cell and a row as tall as
  its tallest; every cell takes its column's width and its row's height.
- `Padding`'s X lies between columns and its Y between rows. A row is as
  wide as the table (`Padding` included) and as tall as its row.
- `FillEmptySpaceColumns` and `FillEmptySpaceRows` stretch the columns or
  rows in proportion to their sizes to fill the area. The table is aligned
  as a whole.
- `MajorAxis` ColumnMajor, or `FillDirection` Horizontal, makes the
  children columns.
- A cell's scale size is of the table's area, not its row's.
- Studio quirk not copied: changing a live table's `MajorAxis` used sizes
  from the layout before.

## UIPageLayout

- Each page has a slot as big as the area along `FillDirection`,
  `Padding` apart (its scale of the area's length), and sits in it by the
  alignments; the current page's slot is in view. Hidden pages take no
  slot and keep their place. `AbsoluteContentSize` is the slots along;
  across it's the area (Inference: the measured pages filled it).
- With no `CurrentPage`, the layout sets it to its first page.
- `JumpTo`, `JumpToIndex`, `Next` and `Previous` set `CurrentPage` at
  once. With `Animated`, the view eases there over `TweenTime` by
  `EasingStyle` and `EasingDirection` (Back Out, the default, overshoots a
  400-pixel page by 40), then `Stopped` fires with the page; without it
  the view moves at once and nothing fires.
- `Next` on the last page and `Previous` on the first do nothing unless
  `Circular`, which goes round. Animated, the view ends on the page;
  without `Animated` it stays past the end (a step back from the first
  page leaves the view one page before it, the last page far off to the
  right) until the next jump. An index out of range does nothing.
- `PageEnter` and `PageLeave` fired only when the layout's parent clips,
  in an order not worked out yet; Rook doesn't fire them. The wheel, touch
  and gamepad don't turn pages in Rook yet.

## CanvasGroup

- Rook draws the group's background and what it holds into a texture of
  the group's size in pixels as drawn, then lays it over its place,
  turned with it, faded by `GroupTransparency`, tinted by `GroupColor3`,
  cut by its UICorner and colored by its UIGradient; its UIStroke goes
  over the texture. What it holds is clipped to it. This follows the
  creator docs and Studio's flags (`FFlagCanvasGroupRenderStrokeOnTop`);
  the look is Inference.
- A group that holds nothing that draws (an empty CanvasGroup with no
  background, or one whose children all moved out) draws nothing, as in
  Studio: DB2's chest opening fades its overlay in inside a CanvasGroup,
  then moves the overlay out of it. Rook used to keep showing the
  group's last texture, so the dropping chest stayed frozen on screen
  behind the reel (`an_emptied_group_draws_nothing` in `rook-render`).
- Studio 0.744 on this machine drew every CanvasGroup piece by piece
  instead, in edit mode and in a playtest alike: children unclipped,
  `GroupTransparency`, `GroupColor3` and the UICorner and UIGradient
  reaching only the group's own background, the rest as plain frames.
  Studio has a flag for drawing groups one by one when it runs out of
  texture memory (`DFFlagCanvasGroupIndividualDrawWhenOOM`, off), so this
  looks like a fallback, not how players see it; why it fell back here
  isn't known.

## Path2D

From the creator docs (Path2D, Path2DControlPoint and the 2D paths
guide) as of 2026-11-22, and measured in Studio 0.748 on 2026-11-23 by
the conformance case `gui/path2d` (a server script, the path under a
400 × 300 Frame in a ScreenGui that isn't shown), which Rook passes.
What isn't marked Confirmed is Inference. What Studio draws is still to
be compared, under [Open questions](#open-questions).

### Path2DControlPoint

- `Path2DControlPoint.new()`, `.new(position)` or `.new(position,
  leftTangent, rightTangent)`: any other count of arguments (a method
  call's too) raises `Trying to create a Path2DControlPoint with wrong
  number of arguments` before the arguments are looked at, and each one
  given must be a UDim2, nil included: `invalid argument #1 to 'new'
  (UDim2 expected, got nil)` (Confirmed). The `Path2DControlPoint` table
  is read-only (Confirmed).
- `tostring` gives `Path2DControlPoint { Position = {0.5, 10}, {0.25,
  -20}, LeftTangent = {0, -5}, {0, 0}, RightTangent = {0.100000001, 3},
  {0, 4} }` (Confirmed). Points with equal members are `==`; comparing,
  joining and arithmetic raise as for other datatypes (Confirmed).
- Assigning `Position` or a tangent doesn't raise (Confirmed); Rook
  changes that value (Inference).

### Points and the curve

- A point's `Position` is a UDim2 in the path's parent: its scale of the
  parent's size, plus its offset in pixels. The tangents are UDim2s too,
  added to it. Rook places the points in the parent's box as laid out,
  turned and scaled with it (Inference: the box, not a ScrollingFrame's
  canvas or the area inside a UIPadding).
- Each pair of neighboring points makes a segment (Confirmed): a
  straight line when both tangents between them (the first's
  `RightTangent`, the second's `LeftTangent`) are zero; a quadratic
  Bézier through the one handle when only one is (at the zero end it
  leaves towards that handle); otherwise a cubic Bézier from the first
  `Position`, through it plus its `RightTangent` and the second's
  `Position` plus its `LeftTangent`, to the second's `Position`. With
  `Closed` and 3 points or more, the last point joins the first the same
  way (creator-docs).
- Scripts set its points with `SetControlPoints`, `InsertControlPoint`,
  `UpdateControlPoint` and `RemoveControlPoint`, and read them with
  `GetControlPoints` (a new table each call) and `GetControlPoint`;
  indexes start at 1. Each setter fires `ControlPointChanged`, with no
  arguments, once it has finished; one that raises fires nothing, even
  where it changed the points first (Confirmed). Its other properties
  don't fire it.
- A path has a parent when it's in a GUI object or LayerCollector (Rook
  looks past Folders, Inference). Without one, `InsertControlPoint`,
  `UpdateControlPoint`, `GetLength` and `GetBoundingRect` raise
  `Attempting to use Path2D with invalid parent` (Confirmed), the
  setters changing nothing; `SetControlPoints`, `RemoveControlPoint` and
  `GetControlPoint` work. Rook raises it from the position and tangent
  queries too, after their own check (Inference).
- Studio keeps the points twice, as the path's own and in a spline that
  places them, and the two can disagree (Confirmed):
  `SetControlPoints` leaves the spline empty, and `InsertControlPoint`
  and `RemoveControlPoint` change the points, then raise from the spline
  when it hasn't that place (`Spline inserting control point at index 1
  access out of bounds`, its index counting from 0), keeping the points
  as changed. So straight after `SetControlPoints`, inserting past the
  first point or removing raises. Rook builds the spline again when the
  path's tree is laid out for a frame or a curve query reads it
  (Inference), and `UpdateControlPoint` doesn't consult it (Inference).
- The points are a property of the path, as in Roblox: the hidden
  `PropertiesSerialize`, a binary blob scripts can't read or set, which
  files save and which replicates. So `Clone` copies a path's points
  (Confirmed, with `Closed` and `Thickness`), a path the server sets up
  reaches clients with its points, and the server's later changes follow
  it. Roblox's layout of the blob isn't documented, so Rook writes a
  layout of its own: Studio can't read the points of a path Rook saved,
  and a path Studio saved with points starts with none in Rook. Its
  default, 4 zero bytes, is no points in both. Changing the points
  fires no `Changed` that a script sees (Confirmed).

### Setters' arguments

All Confirmed:

- An index is cast to an `int`: a number or numeric string, rounded
  (1.5 is 2, 3.9 is 4); NaN and anything past an `int32` are
  -2147483648; `"a"` raises `Unable to cast string to int`, nil
  `Argument 1 missing or nil`. An index outside the points raises
  `Path2D:GetControlPoint() at index 4 access out of bounds`, with the
  method's name and the index as cast; `InsertControlPoint` takes 1 to
  one past the last.
- A point must be a Path2DControlPoint: `Unable to cast UDim2 to
  Path2DControlPoint`, `Argument 2 missing or nil`.
- `SetControlPoints` takes an Array: nil is `Argument 1 missing or nil`,
  a string `Unable to cast string to Array`, a table with keys but no
  first item `Unable to cast Dictionary to Array`, none of which change
  the points. Otherwise it empties the points and adds the items in
  turn, nils in holes included, raising `Variant cast failed` at the
  first that isn't a point, with those before it kept.
- `GetMaxControlPoints()` is 100 (creator-docs). `SetControlPoints`
  with more keeps the first 100 and raises `Path2D:SetControlPoints()
  only supports 100 control points.`; `InsertControlPoint` on a full
  path raises the same with its own name.

### Queries

All Confirmed but where marked:

- `GetPositionOnCurve(t)` and `GetTangentOnCurve(t)` read the curve at
  `t` from 0 to 1 by its parameter: each segment takes an equal share of
  `t`, a point where two meet belonging to the later one. The
  `ArcLength` versions read it at `t` of its length, by the segments'
  true arc lengths, a point where two meet belonging to the earlier one.
  `t` is a `float`, held to 0 to 1, NaN reading as 0; nil is
  `Argument 1 missing or nil`.
- With no segments they raise `Path2D:GetPositionOnCurve() attempting
  to query with zero segments` (with the method's name).
- A position is a UDim2 in scale alone: where the curve is in the
  parent's pixels over the parent's size (a quarter of the way along a
  100-pixel line in a 400-wide parent is `{0.0625, 0}`).
- A tangent is the segment's derivative by its own parameter, in the
  parent's pixels: `(100, 0)` all along a 100-pixel line, 3 times the
  handle at a cubic's end, twice the handle at a quadratic's zero end,
  and `(0, 0)` along a segment of no length.
- `GetLength()` is the curve's length in the parent's pixels (Inference:
  0 with no segments): infinite through a point at an infinite place,
  NaN through one at NaN.
- `GetBoundingRect()` is the box round the points' positions, not the
  curve between them, in the pixels `AbsolutePosition` is in, grown by
  half `Thickness` and half a pixel each way (by 1 at 1, 5.5 at 10), and
  not at all at `Thickness` 0.
- Studio finds the place a share of the length along a curve to within
  about a millionth of its parameter, so the third decimal of a tangent
  there can differ from the exact place's (the case prints those to two
  places); Rook's is the exact place, within kurbo's error bound.

### Drawing

- A visible path draws a line `Thickness` pixels wide (1 by default,
  kept within 0 to 100, NaN kept as it is, Confirmed) along its curve in `Color3` (black by default),
  among its parent's children by `ZIndex`, as a GUI object would, and
  clipped as they are. It takes no input (Inference).
- A UIGradient under it colors the line in place of `Color3`
  (Inference: over the line's bounding box). UIScales widen the line, as
  they do a UIStroke's (Inference).
- `Transparency` isn't scriptable (Confirmed: `The current thread cannot
  read 'Transparency' (lacking capability RobloxScript)`); a file can save one, which Rook draws
  (Inference). A translucent line is drawn whole and then faded, so its
  joints don't show darker.
- Rook draws the line with round joins and round ends, its edge smoothed
  over a pixel, the curve kept within a quarter of a pixel (a sixteenth
  on a SurfaceGui or BillboardGui, seen up close); a curve over a million
  times that across is kept within that share of its size instead, so it
  draws a bounded number of lines. Approximation: Studio's joins, ends,
  smoothing and curve steps aren't compared with Rook's. Inference: a
  path with a point or tangent at an infinite or NaN place draws
  nothing.

### Open questions

To check side by side in Studio:

- the line's shape: its joins at sharp corners, its ends, how its edge
  is smoothed and how finely the curve is stepped, at thin and thick
  widths;
- when Studio builds the spline again after `SetControlPoints`, whether
  `UpdateControlPoint` raises from it as inserting and removing do, and
  whether the position and tangent queries need a parent;
- whether a tangent counts as zero by its UDim2 or by the pixels it
  comes to, and which pixels the queries count in under a UIScale or a
  turned parent;
- points in a ScrollingFrame or under a UIPadding, a UIScale's effect on
  the width, a UIGradient's box, and whether a path stops drawing past a
  SurfaceGui's edge;
- what a 0 `Thickness` or a single point draws, and whether a path takes
  input;
- the layout of `PropertiesSerialize`, from a place Studio saves with a
  path in it, and `ControlPointChanged` on a client when the server
  changes the points.
