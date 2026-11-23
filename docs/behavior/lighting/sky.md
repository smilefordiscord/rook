# The sky over the day

Checked against Studio 0.748 on 2026-11-23.

How Studio darkens and tints the sky at dawn and dusk, and how that
reaches what the sky lights and what reflects it, measured in
`version-fee454b8e2094ba9`; and the sun and moon over it.

## The tint

From 6:30 to 17:30 the skybox draws as its images. Before and after, it
is multiplied by a tint that goes by `ClockTime` alone: captures at
`GeographicLatitude` 0 and 60 (a different sun height at each time) came
out the same, and a capture 1 s and 6 s after a change were identical, so
it doesn't fade in. The tint is about the same overhead, 30 degrees up and
at the horizon (within 0.05), so one color per time describes it.

Shares of the noon sky, per channel, read as light units (each pixel
through the inverse of the default tone curve, [tone
mapping](tone-mapping.md)), averaged over the three heights, on the
default sky:

| ClockTime | Tint (R, G, B) |
|---|---|
| 6.0 and before | 0 (the night sky) |
| 6.1 | 0.14, 0.11, 0.09 |
| 6.2 | 0.28, 0.24, 0.22 |
| 6.29 | 0.45, 0.41, 0.39 |
| 6.38 | 0.65, 0.63, 0.61 |
| 6.44 | 0.82, 0.80, 0.78 |
| 6.5 to 17.5 | 1 (byte-identical to noon) |
| 17.62 | 0.72, 0.66, 0.60 |
| 17.71 | 0.55, 0.45, 0.37 |
| 17.8 | 0.40, 0.29, 0.21 |
| 17.9 | 0.25, 0.15, 0.08 |
| 18.0 | 0.14, 0.06, 0.01 |
| 18.3 and after | 0 (the night sky) |

Dawn stays gray; dusk turns orange as it darkens. After about 18.3 and
before about 6.0 the clouds are gone and only a dark night gradient shows,
with a purple glow along the horizon and stars.

Read as squared pixels instead, the zenith and the horizon seemed to take
different shares; that was the tone curve's toe, not the tint.

## What it reaches

- **The sky's diffuse light** (`EnvironmentDiffuseScale`) takes the tint
  once: a white SmoothPlastic part lit only by it (Brightness 0, Ambient
  and OutdoorAmbient black) kept (0.49, 0.43, 0.35) of its noon light at
  6.29.
- **Reflections of the sky** take it twice. Black opaque water lit by
  nothing but its reflection kept 0.41 at 6.38 and 0.17 to 0.20 at 6.29,
  the square of the sky's 0.65 and 0.43 there; a Reflectance 1
  SmoothPlastic part kept (0.18, 0.11, 0.06) at 6.29. Studio's water and
  surface shaders (`shaders_glsl3.pack`) multiply the reflection cube's
  sky by `mix(SkyGradientBottom, SkyGradientTop, clamp(R.y × 1.588))`;
  the cube itself holds the sky as drawn, so the tint comes in once with
  it and once with the gradient.
- **The water's reflection** is also scaled by the light grid's sky
  visibility at the surface, with a tenth of the local lights' light added
  (the same shaders).
- `Brightness` doesn't change the sky or its reflection: at noon, black
  opaque water reflects the sky as strongly at Brightness 0 as at 3. Water
  that is see-through looks darker at Brightness 0 only because the unlit
  ground under it shows through.

## How Rook does it

`sky_tint` in `crates/rook-scene/src/lighting/sky.rs` holds the table and
interpolates linearly between the measured times (`Lighting::sky_tint`).
The renderer multiplies the drawn sky by it (`sky.wgsl`), the sky's diffuse
light and the covered gray by it (`sky_light`, `sky_covered` in
`lighting.wgsl`), and every reflection of the sky by its square
(`sky_reflection_tint`: parts through `sky_reflected`, water in
`water_surface`). It replaced a fitted guess that dimmed the sky by the
sun's height.

The drawn sky also takes `ExposureCompensation` in light units, as Studio's
shaders scale everything; scaling it before the light units made bright
sky channels jump past the curve's top, a saturated blue at exposure 0.5.
The water's sky is in light units as the reflection cube holds it (the
image's sRGB color squared), not through the curve's inverse.

Against Studio at 6.29, exposure 0.5, Brightness 0 (a pool beside a
wall): the sky lands within 9 levels at the horizon, and overhead within
4 in red and green but 22 too blue. The water was brighter than Studio's
(11, 26, 35 against 7, 12, 16) until see-through water let through what
Studio's does and only its nearest surface drew ([water](../terrain/water.md),
2026-11-05): black see-through water seen low over black ground now
lands within 2 levels far off and 10 to 30% above Studio's light near the
camera.

## A sky that won't load

Measured in Studio 0.745 on 2026-11-06, Baseplate, ClockTime 14. A Sky with
five good night images and `SkyboxFt` set to an id that isn't an image
(`rbxassetid://1`) drew the default sky on every side, including the sides
with good images. Setting that one image to a good id drew the Sky's night
images on every side, so a Sky is all or nothing. Rook draws the default
sky without a Sky, until all six of a Sky's images are there, and when one
won't load. Its own plain sky is left for having no default sky at all: no
Roblox packages or install.

A Sky with an empty image is not settled: one captured after the images had
loaded before showed no hole, which may be Studio's cache. Rook treats it as
an image that won't load (Inference). Studio's sky while images load isn't
measured; that it's the default sky follows from the failed case (Inference).

## The Sky's properties

Measured in Studio 0.748 on 2026-11-23 (conformance case
`lighting/sky_clamps`), as scripts see them:

- A new Sky has `CelestialBodiesShown` on, `SunAngularSize` 21,
  `MoonAngularSize` 11, `StarCount` 3,000 and `SkyboxOrientation`
  (0, 0, 0); `SunTextureId` `rbxasset://sky/sun.jpg`, `MoonTextureId`
  `rbxasset://sky/moon.jpg`, and the six skybox images
  `rbxasset://textures/sky/sky512_bk.tex` and so on. Each `Content`
  property (`SunTextureContent`, `SkyboxBackContent`, ...) is a Uri of
  its image's id, and an empty id reads as a `None` Content.
- `SunAngularSize` and `MoonAngularSize` are kept within 0 to 60: a value
  set outside reads back at the nearer end (-1 reads 0, 70 and `inf`
  read 60), and NaN stays NaN.
- `StarCount` is a whole number kept within 0 to 5,000: 2.5 reads 2,
  1e9 reads 5,000, and NaN reads 0.
- `SkyboxOrientation` is kept as it's set, angles past 360 and NaN
  included.

Rook keeps the same ranges (`crates/rook-lua/src/api/properties/ranges.rs`).

## The sun and moon

From the creator docs (Sky, the Skyboxes guide) and the install's files,
read on 2026-11-23; the properties' ranges are [measured](#the-skys-properties).

- A Sky's `CelestialBodiesShown` off hides the sun, moon and stars;
  `SunAngularSize` and `MoonAngularSize` (degrees) are within 0 to 60
  (Confirmed), and 0 hides that body alone (Confirmed for the sun by the
  creator docs; Inference for the moon, whose page says nothing of it).
  Their images are `SunTextureId` and `MoonTextureId`, by default
  the install's `content/sky/sun.jpg` and `moon.jpg` (128 × 128 JPEGs, a
  white disc with a glow, and the moon, on black).
- They rise and set with the time of day (creator docs); the sun's disc
  sits where `Lighting:GetSunDirection()` points ([the sun](sun.md)) and
  the moon's opposite the sun's daily circle, as their light does
  (Inference).
- `SkyboxOrientation` turns the skybox, by Y first, then X, then Z; the
  sun, moon and stars don't turn with it (Confirmed: creator docs). Which
  way a positive angle turns is an Inference (as CFrame angles do).
  Studio's surface shaders turn their reads of the sky's reflection cube
  by the same turn (`SkyboxRotation` in `shaders_glsl3.pack`).
- Studio's `AdvSun` shader draws each body as a textured quad at the far
  plane: the image times a color (mixed between two by the quad's
  height), squared, times a constant power of two, then the exposure.
  The quad and the image's black corners are added over the sky. The
  constant and the colors aren't in the pack.

Rook (`celestial.rs` in `crates/rook-scene/src/lighting/`, `vs_disc` and
`fs_disc` in `shaders/sky/sky.wgsl`) draws each as a square facing the
camera that spans its angular size, the image's sRGB color squared times
its light, added over the sky. Approximations until Studio is measured:
the sun at 4 light units at its image's white, so its middle is white,
and white down to the horizon though its light turns yellow there (a
Studio 0.748 screenshot at ClockTime 17.8, 2026-11-23, shows a white
disc with a pale cyan-white halo); the sun and the moon (at 1 unit) are
faded in over the last degrees above the horizon as the moon's light is. A Sky's own image that won't load gives the
default one, as a skybox image does (Inference); without a Roblox install
the defaults don't load either, and Rook draws its own soft white disc;
an empty image draws nothing (Inference). A place without a Sky shows a new Sky's sun and
moon (Inference). The reflection cube keeps the sky unturned and every
read of it turns by `SkyboxOrientation`, so turning the sky never builds
the cube again.

To settle in Studio: the discs' brightness at noon, at dusk and through
the horizon; whether a positive `SkyboxOrientation` angle turns
as Rook's; whether a place without a Sky shows the sun.

## The night sky

Observed in Studio 0.745 with the tint above: where the tint reaches 0 the
skybox's images are gone and a dark gradient shows, with a purple glow
along the horizon, and stars. Its colors weren't measured.

Studio's install has the pieces: `Sky`, `SkyLinearVColor` and their `Adv`
variants draw a mesh with vertex colors times a gradient by height (mixed
between two colors over the first 1,700 studs up), so the gradient is
geometry colored per vertex; `Stars` and `AdvStars` draw points two pixels
across, each its own color, the `Adv` one added over the sky. The stars'
colors and the gradient's are data the CPU feeds them, not in the pack.
`StarCount` sets how many there are, and only with `CelestialBodiesShown`
on (Confirmed: creator docs; 0 to 5,000, default 3,000, [measured](#the-skys-properties)).

Rook (`night_sky` in `crates/rook-scene/src/lighting/sky.rs`,
`shaders/sky/night.wgsl`, `vs_star` in `shaders/sky/sky.wgsl`):

- The gradient shows as the tint fades out, none while its brightest
  channel is above 0.25 and all of it at 0 (smoothly between), under
  whatever is left of the skybox, and the stars with it: from sRGB (0.02, 0.03, 0.08)
  overhead to (0.17, 0.12, 0.25) at and below the horizon, the glow falling
  to 1/e by 0.15 of the way up (the sine of the elevation). Approximation:
  a look to match the description until a capture of the default sky at
  midnight sets the colors and the glow's height.
- `StarCount` stars, each one pixel at a direction spread evenly over the
  sky and a brightness from a twentieth of the brightest up, most of them
  faint (the square of an even spread), both made from its index, so they
  stay put from frame to frame. The
  field turns with the time of day about the axis the sun turns about
  (Inference), and doesn't turn with `SkyboxOrientation` (Confirmed:
  creator docs). The brightest is 0.15 light units at night, faded in
  with the gradient (Approximation, set against a Studio 0.748 screenshot
  of the default sky at midnight, 2026-11-23: stars a pixel across, most
  of them faint; Rook's now land within about 10 levels of Studio's
  middle and brightest stars, at about the same number on screen). A file's `StarCount` over 5,000 draws 5,000
  (Inference: a script's reads back as 5,000).

To settle in Studio: the gradient's colors at midnight overhead, 10° up and
at the horizon; when it shows against the tint between 18.0 and 18.3; the
stars' colors and whether they turn with the clock.

## Not done

- The sun, moon, stars and night gradient in reflections (Studio's
  reflection cube may hold them; unmeasured). Rook's reflections of the
  sky go black at night.
- 18.0 to 18.3 wasn't sampled between its ends.
