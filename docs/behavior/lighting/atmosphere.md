# Atmosphere

Checked against Studio 0.748 on 2026-11-23. Only the
[properties](#its-properties) as scripts see them are measured in Studio;
the rest comes from the creator docs (Atmosphere, the "Atmospheric
effects" guide) and the form of the shaders in the install's
`shaders_glsl3.pack` (`version-8d7a1009c17e4ff8`), read for facts as the
[clean-implementation policy](../../design/policies.md#clean-implementation)
allows.

## Its properties

Measured in Studio 0.748 on 2026-11-23 (conformance case
`lighting/sky_clamps`):

- A new Atmosphere has Density 0.395, Offset 0, Haze 0, Glare 0, Color
  (200, 170, 108) and Decay (92, 60, 14).
- Density and Offset are kept within 0 to 1, Haze and Glare within 0 to
  10: a value set outside reads back at the nearer end (`inf` reads the
  top), and NaN stays NaN.
- Color isn't kept within anything: (2, -1, 0.5) reads back as it is.

Rook keeps the same ranges (`crates/rook-lua/src/api/properties/ranges.rs`).

## What the docs say

- Density: how much of parts and terrain the air hides; it doesn't touch
  the skybox directly, only how much of it shows through them.
- Offset: how light goes from the camera to the sky behind; higher draws
  far objects as silhouettes against the sky, lower blends them into it.
  Too low shows the skybox "ghosting" through objects.
- Haze: haziness above the horizon and into the distance; Color tints it.
- Glare: a glow around the sun, seen only with Haze above 0.
- Decay: the haze's hue away from the sun, falling off from Color towards
  it, seen only with Haze and Glare above 0.
- Lighting's fog properties are hidden while Lighting holds an
  Atmosphere.

## What Studio's shaders show

- **The sky** (`AdvSky`): the sky keeps
  `T = exp2(a / (0.001 + y^b))` of itself towards a direction `y` up, so
  less towards the horizon. The rest is a haze: a six-sided ambient cube
  of light times the fog color, plus the sun's light times
  `pow(0.5 + 0.5 cos θ, g) × (1 − T)` with a power `g` per channel (θ from
  the sun) times a glare strength. The sky's image is mixed towards the
  haze by `1 − T`. With Haze 0 the docs say neither the haze nor the glare
  shows, as `T = 1` (`a = 0`) would give.
- **Surfaces**: every surface shader keeps
  `exp2(z × distance + x) − w` of the surface. With Lighting's fog that's
  the fit in [tone mapping](tone-mapping.md#fog); with `w = 0` the fog's
  color is no longer FogColor but the sky's prefiltered reflection cube
  read towards the surface at level `max(y, kept) × 5`, where `y` is a
  fourth parameter. So a fogged surface takes the sky behind it, blurred
  at least by `y`: low, the sky's detail shows through (Offset's
  ghosting); high, an average (silhouettes).
- Which property becomes which parameter is set by Studio's CPU side and
  isn't in the pack.

## How Rook does it

`atmosphere.rs` in `crates/rook-scene/src/lighting/` maps the properties
onto that form; `shaders/sky/atmosphere.wgsl` draws it, for the sky
(`hazed`), the sun, moon and stars (dimmed by `T`), and every surface
through `fog_units` and `fog_light` in `shaders/lighting.wgsl` (parts,
terrain, water, particles and the world's GUI). With an Atmosphere,
Lighting's FogStart, FogEnd and FogColor aren't drawn (Inference: the docs
hide them, and the `w = 0` form has no place for them). The first
Atmosphere in Lighting counts (Inference).

The mapping, all Approximations to settle against Studio:

| Property | Rook's parameter |
|---|---|
| Density `d` (0 to 1) | `z = −d³ / 40` per stud: a surface keeps half of itself every `40 / d³` studs (about 650 at the default 0.395), `x = 0` |
| Offset (0 to 1) | the least blur `y` |
| Haze `h` (0 to 10) | `a = −0.1 h`, `b = 1`: at Haze 1 the sky keeps 93% overhead, half at 10% of the way up |
| Color | the haze's color, its sRGB value squared |
| Glare (0 to 10) | the glare's strength, times the sun's (or moon's) light |
| Decay | the glare's power per channel, `4 / Decay` (a channel Decay keeps falls off slowly) |

The ambient cube is the brightness of Rook's sky colors overhead and at
the horizon, tinted for the time of day, with the night's gradient
([sky](sky.md#the-night-sky)): the haze takes Color's hue, not the sky's,
as the docs' captures show it (a blue sky times a yellow Color would be
green).

To settle in Studio, on the Baseplate at noon and at dusk:

- white parts at known distances under Density 0.1 to 1 (`z`, and whether
  `x` is 0);
- the sky's color overhead, 10° up and at the horizon under Haze 0 to 10
  (`a`, `b`), with Color white and colored;
- a far part against a detailed skybox at Offset 0, 0.5 and 1 (`y`);
- the glow around the sun at Glare 0 to 10 with Decay white and colored;
- a place with both an Atmosphere and FogEnd set.

## Not done

- Reflections don't show the haze: Studio draws its reflection cube with
  the haze (its `AdvSky…Env` shaders), Rook's cube holds the sky without
  it.
- Clouds (M13 step 1).
