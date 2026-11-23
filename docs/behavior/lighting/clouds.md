# Clouds

Checked against Studio 0.748 on 2026-11-23. Only the
[properties](#its-properties) as scripts see them are measured in Studio,
and the look compared with Studio screenshots
([against Studio](#against-studio)); the rest comes from the creator docs (Clouds, the "Dynamic clouds" and
"Global wind" guides), the form of the cloud shaders in the install's
`shaders_glsl3.pack` and the textures beside them in `content/sky/`
(`version-8d7a1009c17e4ff8`), read for facts as the
[clean-implementation policy](../../design/policies.md#clean-implementation)
allows.

## Its properties

Measured in Studio 0.748 on 2026-11-23 (conformance case
`lighting/clouds_clamps`):

- A new Clouds has Cover 0.5, Density 0.7, Color (1, 1, 1) and `Enabled`
  on.
- Cover and Density are kept within 0 to 1: a value set outside reads
  back at the nearer end (`inf` reads 1), and NaN stays NaN.
- A Clouds may be parented anywhere: a Folder, Workspace, Terrain,
  Lighting and a Part all take one, and it stays in Terrain.

Rook keeps the same ranges (`crates/rook-lua/src/api/properties/ranges.rs`),
and draws a NaN Cover, Density or Color channel as 0 (Inference: how Studio
draws one isn't measured).

## What the docs say

- Clouds draw only under Terrain; a Clouds anywhere else draws nothing
  (it may still be [parented](#its-properties) anywhere).
  `Enabled` turns one off, for switching between several in a place.
- Cover, 0 to 1 (Confirmed): how much of the sky's cloud layer the clouds span, from
  sparse to full.
- Density, 0 to 1 (Confirmed): how thick each cloud is, mostly its transparency; low
  is light and see-through, high heavy and dark.
- Color: the clouds' material color; Lighting and Atmosphere also change
  how they look.
- `Workspace.GlobalWind` sets their direction and speed of drift.

## What Studio's shaders show

Four passes, each in the pack:

- **The layer.** A shell from 973 to 975.5 km from a planet's center 971 km
  below the origin: 2 to 4.5 km up, curving down to the horizon. The
  camera's position counts in kilometers (studs times 0.00028). Rays below
  the horizon draw no cloud.
- **The field.** A 3D texture (the install's `sky/clouds.dds`, 256 × 256 ×
  32, one byte a texel) read across every 32 km and up through the layer,
  pushed along a 2D flow (`sky/cloudAdvection.dds`) and lowered by two
  octaves of a 3D detail noise (`sky/cloudDetail3D.dds`, whose values only
  reach half way, so detail only adds cloud). Wherever the field is below
  Cover there is cloud, as thick as 50 × Density × how far below it is.
  Density thins out between 6,000 and 7,000 studs up. The field's values:
  a tenth of it is below 0.435, half below 0.573, nearly all below 0.85.
- **A shadow map.** One tile of the field, each texel's column marched 13
  steps towards the sun (no lower than 0.3 up) through the layer, keeping
  how much cloud lies on the way and where it starts and ends.
- **The march.** A fixed number of steps along each ray, from a fixed share
  of the way through the layer, summed front to back with Beer's law. Each
  step's light is the light around (two ambient cubes the CPU fills,
  lifted by a gray that brightens up the layer and the sky gradient, no
  less than the ambient terms) plus the sun's: three octaves of its light
  through the cloud the shadow map puts between the step and the sun, by a
  48-entry phase function strongly forward, plus light from below. All
  times Color. The sun's light through thin edges is added after. With an
  Atmosphere the clouds fade towards its hazed sky by the distance to the
  layer.
- **The blend.** At a quarter of the frame's pixels: the march softened by
  a 3 × 3 tent and blended with up to seven last frames' clouds at where
  each pixel's cloud was then, unless that cloud was at another distance,
  the light changed much or the history was reset. Clouds from 227 to
  238 km away fade out.
- **Over the sky.** Through the tent again, in the frame's encoding with
  their cover squared, the thinnest (cover 0.12 or less) left out, behind
  everything in the scene.
- **The sun's cover** (`CloudsSunCoverFS`). Into a single texel: the
  shadow map's cloud per step over the point where the sun's ray from the
  camera leaves the layer, averaged over the middle and 27 taps on two
  rings around it, kept as `max(0, 1 − k × average²)`. A one-texel target
  is something the CPU reads back, so the sun's light Studio lights with
  is scaled by it (Inference).

What the CPU feeds them (the cover threshold made from Cover, the step
count, the detail's strength and size, the drift and its clock, the
ambient cubes, the sun cover's `k` and ring radius) isn't in the pack.

## How Rook does it

`clouds.rs` in `crates/rook-scene/src/lighting/` reads the first enabled
Clouds in Workspace's Terrain (Inference: which of several draws), with
the Lighting service: a change to a Clouds in a Terrain marks the
lighting changed in the journal. Color is taken as its sRGB value squared,
as Studio's shaders take colors. `crates/rook-render/src/lighting/clouds.rs`
and `shaders/sky/clouds.wgsl` draw the four passes in the form above,
before the main pass, and the clouds over the sky after the stars, sun and
moon and before the parts. Without Clouds, or at Cover 0, nothing is made
or drawn (Inference: the field is a distance, never below 0, so nothing is
below Cover 0).

- **Cover.** The shaders' threshold is Cover squared (Approximation). The
  field's values put it there: in Studio 0.748 screenshots Cover 0.5
  shows no cloud from the ground and 0.9 an overcast; the creator docs'
  pictures show a few clouds at 0.65 and a mostly covered sky at 0.8.
  Cover itself as the threshold covered a third of the sky at 0.5.
- **The sun's cover.** Worked out on the CPU each frame
  (`lighting/clouds/sun_cover.rs`) from the field alone, without the
  detail and the flow, over rings 0.5 km across; it scales the sun's (or
  moon's) light for everything the renderer lights, the clouds included.
  `k` is 20 (Approximation: set so the ground under Cover 0.9, Density
  0.7 at noon lands within a few levels of Studio's screenshot).
- **Textures.** The install's three, loaded at startup. Without them,
  Rook's own, made once the first time clouds are drawn: a field of
  tiling cells and value noise whose values are spread as the install's
  (so a Cover covers as much sky with either), a detail noise spread as
  the install's, and a flow. Their shapes are Rook's (Approximation).
- **The blue noise.** Studio's march reads it at each pixel's coordinate
  in texels as if they were normalized, the same texel for every pixel;
  Rook takes its average, 0.5 (Inference).
- **Drift.** The field moves with `GlobalWind` at the wind's own speed, a
  stud a second moving it 0.28 m a second at the layer's scale
  (Inference), and changes shape with how far it went (Approximation).
- **The light around.** Rook's sky light (`sky_light`, with
  `EnvironmentDiffuseScale` and the time of day's tint) and the squares of
  Ambient and OutdoorAmbient stand for the ambient cubes; the sky gradient
  is the time of day's tint (Approximation).
- **Fog.** With an Atmosphere, as above with its falloff and blur; with
  Lighting's fog, none (Inference).
- 20 steps, the detail at strength 0.12 and 2 km across, a 128 × 128
  shadow map (Approximation).

Against the frame without clouds, a 1920 × 1080 frame of the test place
took about 0.5 ms longer to draw and read back.

## Against Studio

Compared with Studio 0.748 screenshots on 2026-11-23 (the Baseplate with
a red tower, Cover 0.5 and 0.9 at Density 0.7, ClockTime 14 and 17.7;
the cameras differ, so looks rather than pixels):

- Cover 0.5: Studio shows only the skybox's own painted clouds, at noon
  and at dusk. Rook filled most of the sky with thick clouds; with Cover
  squared it shows a few small ones low down.
- Cover 0.9 at noon: both overcast. The ground under it went from
  (120, 134, 151) to (99, 117, 137) in Studio; Rook's didn't change until
  the sun's cover, and now lands at (101, 122, 142). Studio's overcast is
  a gray-lavender (156, 170, 193); Rook's is lighter and greener
  (166, 193, 206): the light around (Rook's sky colors standing for
  Studio's ambient cubes) is the likely cause, not settled.
- At dusk the few clouds Rook draws at Cover 0.5 are cream, lit by the
  sun's yellow light; Studio's dusk shot has no cloud of its own to
  compare.

To settle in Studio, on the Baseplate at noon, dusk and midnight, with and
without an Atmosphere:

- the sky overhead, 30° up and 10° up at Cover 0.5 and 0.9, Density 0.1,
  0.7 and 1, Color white and colored (the light around, the sun's share,
  the step count);
- the same spot over 10 seconds under `GlobalWind` (10, 0, 0) and with no
  wind (the drift's speed and whether the clouds change shape without
  wind);
- Cover 0 (whether any cloud shows);
- the ground under Cover 0.5 to 1 at several Densities and sun heights
  (the sun cover's `k`, and whether it follows the shadow map's form);
- whether any of this changes at Level21.

## Not done

- Clouds' shadows on the ground place by place: the sun's cover dims the
  whole world at once, as Studio's single texel does; Studio's pack also
  has a cloud shadow shader for the ground, not read yet.
- Clouds in reflections: Rook's reflection cube holds the sky without them.
