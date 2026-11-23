# Post-processing

Checked against Studio 0.748 on 2026-11-23, by its shaders and the
creator docs only: nothing on this page is measured in Studio.

The PostEffects (ColorCorrectionEffect, BlurEffect, BloomEffect,
SunRaysEffect, DepthOfFieldEffect and ColorGradingEffect) change the
rendered world after it's drawn, never the GUI, and only while `Enabled`
and parented to Lighting or to `Workspace.CurrentCamera` (Confirmed:
creator-docs PostEffect). Effects under Lighting show to every player;
under the Camera, to its player alone (Confirmed: the post-processing
guide). Some draw differently or not at all at low quality levels
(Confirmed: creator-docs PostEffect, unmeasured); Rook has no quality
setting and draws every level the same.

## Where each effect sits

Studio's post shaders (the install's `shaders/shaders_glsl3.pack`, 0.748,
read 2026-11-23; Inference until measured) show one last pass
(`ImageProcess*`) that does all the composing:

1. It decodes the frame into light units (4 × value², [tone
   mapping](tone-mapping.md#the-frame-model)).
2. It adds Bloom's light (a second texture, decoded the same way) and
   SunRays' light (`FXComposition` variants).
3. It mixes in the blurred frame by a factor (`Blur` variants).
4. It runs the tone curve.
5. It multiplies the result by one 3 × 4 color matrix (its rows and an
   offset), with an optional color lookup table before it.

The matrix acts on the curve's output, the pixel values written to the
screen (a dithered variant adds its noise to the same values), so
ColorCorrection works on pixels, not on light.

## ColorCorrectionEffect

`Brightness` shifts every channel (-1 black, 1 white), `Contrast` spreads
(above 0) or draws in (below 0) the darks and lights, `Saturation` takes
the colors towards gray (-1 fully gray) or past their own, and
`TintColor` multiplies each channel (Confirmed: creator-docs). Several
compose (Confirmed: creator-docs).

How the properties make Studio's matrix isn't in its shaders. Rook's
(`ColorCorrection::matrix`, Inference):

- on sRGB pixel values *p*: (*p* − 0.5) × (1 + Contrast) + 0.5 +
  Brightness;
- then mixed with its luma (Rec. 709 weights) by 1 + Saturation;
- then times TintColor;
- several effects apply one after another, Lighting's first, then the
  Camera's, each in child order, as one matrix;
- the result is clamped to 0 to 1.

## BlurEffect

A Gaussian blur over the whole 3D view, `Size` its radius in pixels;
only one applies at a time, the one with the greatest `Size` (Confirmed:
creator-docs).

Studio's blur shaders (`Blur1FS` to `Blur7FS`, Inference) are separable
Gaussians: a center texel plus 1 to 7 pairs of neighbors, the pair 2k − 1
and 2k texels out read by one filtered tap placed between them by their
weights, the weights exp(−d² / 2σ²). They read the frame's encoded values
(the square root of light), not light, and the last pass mixes the
blurred frame with the frame by a factor, before the curve. Bloom and
SunRays are added to the frame only there, so a full blur would hide
them (Inference from the shaders: `ImageProcessBlurBloomFS` mixes the
frame with its Bloom towards the blurred frame alone). Studio also has
4 × 4 downsampling shaders, used for wide blurs (Inference).

Rook (`renderer/passes/post/blur.rs`, `shaders/post/blur.wgsl`): the same
Gaussian, σ half of `Size` in pixels at the screen's resolution
(Approximation: how Studio turns Size into σ, and whether it follows the
window's size, isn't measured). A wide blur runs on a copy of the frame
shrunk by a power of two, so σ is at most 4 texels of the copy and a
pass reads at most 8 pairs; the tone pass reads the result filtered,
replacing the frame wholly (the factor is 1 while `Size` is above 0,
Inference).

## BloomEffect

Bright colors glow: light over `Threshold` (1 only pure white, 0 every
color) blends additively with itself by `Intensity`, spread `Size`
pixels, and `Size` 0 keeps the brightening without the bleed. Several
compose (Confirmed: creator-docs). Studio's defaults: Intensity 0.4, Size
24, Threshold 0.95 (the reflection database).

Studio's bloom shaders (Inference):

- **The first copy** (`DownSampleGjoelFS`) averages nine filtered taps of
  the frame, decoded into light units, and keeps the average times
  max(*m* − *T*, 0) / (*m* + 0.001) × *K*, *m* its brightest channel:
  light fades in above the threshold rather than switching on, and a
  spot smaller than the copy's texels averages down before the threshold
  applies.
- **Smaller copies** (`DownSampleGjoelMipFS`) are the same nine-tap
  average, without the threshold.
- **Back up**, each copy is read through a 3 × 3 tent (weights 1, 2, 1
  each way, `UpSampleTentFS`) scaled by a factor and added to the next
  larger.
- **The last pass** adds the result to the frame's light before the tone
  curve (`ImageProcessBloomFS`), so a Bloom brightens before the curve
  rolls it off.

Rook (`renderer/passes/post/bloom.rs`, `shaders/post/bloom.wgsl`) runs
that chain from half the frame's size down, the copies' light summed
from the smallest up, each copy's light weighted 1 / *n* (*n* copies;
Approximation). What the properties become is Inference:

- *K* is `Intensity`.
- *T* is the light the frame holds where its encoded value (the light's
  square root, over the frame's 4 units) equals `Threshold`: 4 ×
  Threshold² units, so 1 blooms only the brightest the frame holds and 0
  everything. Studio's in-place glow (`ImageProcessLQNeonBloomFS`)
  thresholds the encoded value too.
- *n* is the number of copies whose reach (2 to the power of *n* + 1
  pixels) first covers `Size`: 4 at the default 24.
- `Size` 0 adds each pixel's own light over the threshold in the tone
  pass, with no chain.
- Each Bloom runs its own chain into one sum the tone pass adds. The
  first eight with a `Size` above 0 draw and the first four with `Size`
  0 add in place (Approximation: Studio's limits, if any, aren't
  measured; a bound keeps a frame's passes bounded).
- A setting that isn't a finite number draws as its default
  (Inference: Studio's handling isn't measured).

## SunRaysEffect

A halo of light with rays around the sun, shaped and blocked by what's
between the camera and the sun; `Intensity` its opacity, `Spread` how
wide it spreads, meant to stay within 0 to 1 (Confirmed: creator-docs).
It moves with `ClockTime` (Confirmed: the post-processing guide). Studio's
defaults: Intensity 0.25, Spread 1.

Studio's sun rays shaders (`SunRays6FS`, `9FS`, `12FS`, then
`RadialBlur3FS` to `12FS`; Inference):

- Each pixel takes 6, 9 or 12 steps from itself towards the sun's place
  on screen, covering half the way and stopping at the screen's edge,
  shifted by a noise texture.
- It counts the steps whose linear depth reads as sky (Studio's linear
  depth is studs × 0.002, sky 1, tested against 0.999: anything 499.5
  studs or further counts).
- Its light is the sun's color times that count, the intensity and a
  Henyey–Greenstein lobe in the angle between the pixel's view ray and
  the sun: (1 − *g*²) / (1 + *g*² − 2*g* cos θ)^1.5.
- A radial blur then averages taps from each pixel a quarter of the way
  towards the sun, and the last pass adds the result to the frame's light
  before the curve (`ImageProcessFXCompositionFS`).

Rook (`renderer/passes/post/sun_rays.rs`, `shaders/post/sun_rays.wgsl`)
does the same at half the frame's size, with 12 steps and 6 blur taps,
from `Lighting`'s sun direction (the moon's at night) and its light in
light units. It draws nothing while the sun is behind the camera. Its
own choices:

- The lobe is scaled to 1 at the sun, so `Intensity` is the share of the
  sun's light the rays carry there (Approximation).
- *g* is 1 − 0.2 × Spread, at most 0.99: a halo about 15 degrees wide at
  Spread 1 and under a degree at 0 (Approximation).
- Of several SunRaysEffects the one with the greatest `Intensity` draws
  (Inference: the shader takes one set of settings).

## DepthOfFieldEffect

Blurs what isn't in focus, as a camera lens does: nothing within
`InFocusRadius` studs either side of `FocusDistance` studs from the
camera, the far field beyond by `FarIntensity`, the near field before by
`NearIntensity` (Confirmed: creator-docs and its diagram). Studio's
defaults: FarIntensity 0.75, FocusDistance 0.05, InFocusRadius 10,
NearIntensity 0.75.

Studio's DOF shaders (`DOF*`, `BokehFarCoCFS`, `BokehNearCoCFS`;
Inference) split the frame into a far and a near field at reduced size:

- Each pixel gathers 45 taps (27 in a cheaper form) over a disk whose
  radius is its circle of confusion squared times a setting, × 0.014 of
  the screen's height.
- A far tap counts only where its own circle reaches back to the pixel,
  weighted by its circle; near taps count by their own circles, so the
  near field's blur spreads over what's behind it.
- The fields are resolved back into the frame before the bloom and the
  last pass read it.

The `shaders_glsl3.pack` forms that compute the circles from depth are
stubs (they return 1), so how a distance becomes a circle isn't in them.

Rook (`renderer/passes/post/depth_of_field.rs`,
`shaders/post/depth_of_field.wgsl`, `shaders/post/focus.wgsl`), at half
the frame's size:

- A first pass keeps one frame texel of each two by two, in light units,
  with its far circle.
- The far gather takes 24 taps over the pixel's own circle, each counting
  by Studio's rule.
- The near gather takes 24 taps over the widest near circle, each
  counting where its own circle reaches the pixel; how many count is how
  much the near field covers the pixel.
- The tone pass mixes the far field in by each pixel's own circle,
  worked out from the full-size depth buffer so sharp things keep sharp
  edges against a blurred background, then the near field over it.

Its own choices:

- The circles are a thin lens's (Approximation): beyond the focus 1 −
  (FocusDistance + InFocusRadius) / distance, 0 at the focus's far edge
  and 1 at infinity (the sky blurs fully); before it (FocusDistance −
  InFocusRadius) / distance − 1, 0 at the near edge and 1 at half that
  distance.
- A circle of 1 blurs across a radius of the intensity times 0.02 of the
  screen's height (Approximation).
- It's mixed in by the tone pass, before Bloom and SunRays add their
  light, where Studio resolves it into the frame before its bloom chain
  and its blur read it, so a Bloom's glow and a Blur come from the sharp
  frame (Approximation).
- Of several DepthOfFieldEffects the first enabled draws (Inference).

## Neon's glow

Neon glows at high quality levels with no BloomEffect anywhere
([antialiasing](../rendering/antialiasing.md) notes it blurs edges
there). Studio's shaders put that glow through Bloom's machinery: its
tone pass has `LQNeon` forms that add a glow from the frame's brightest
channel over a threshold, and a temporal glow downsample
(`DownSampleGlowTemporal8x8FS`) thresholds the frame as the bloom chain's
first copy does (Inference). So it belongs with Bloom, run with the
engine's own threshold and intensity at the levels that turn it on. Rook
doesn't draw it: those settings and levels need Studio captures (Neon
squares on black with no Bloom, at Automatic and Level21), and Rook has
no quality levels.

## ColorGradingEffect

Its `TonemapperPreset` picks the tone curve ([tone
mapping](tone-mapping.md)). Rook takes the first enabled one, under
Lighting or the Camera (Inference: one curve draws; which of several
isn't measured).

## How Rook does it

`rook-scene` reads the enabled effects into `Scene::post`
(`lighting/post.rs`) from Lighting's children, then the current Camera's,
only when the journal says one of them changed, came or went, or
`Workspace.CurrentCamera` changed (`RenderChanges::post_effects`). The
tone pass (`renderer/passes/tonemap.rs`, `shaders/tonemap.wgsl`) is
Studio's last pass: ColorCorrection is its matrix, after the curve,
before Highlights and the GUI, and costs nothing when no effect is
enabled. The effects that need passes of their own run them between the
main pass and the tone pass (`renderer/passes/post.rs`), only while
enabled, into textures made the first frame they're needed and again
when the window's size changes; the tone pass reads them, or a black
texel when they're off.

## What Studio must settle

- ColorCorrection: Rec. 709 or other luma weights; the order of
  brightness, contrast, saturation and tint; contrast's pivot; how two
  effects combine (a gray ramp and a color chart under each property
  alone, then two effects at once).
- Blur: its width for a `Size` (a lone white line a pixel wide at
  several Sizes), whether it follows the window's size (the same at two
  window sizes), the mix factor at small Sizes, and whether Bloom and
  SunRays show through a Blur.
- Bloom: what `Threshold` means in light (Neon squares of several
  grays on black at several Thresholds), how far the glow reaches for a
  `Size` and how it falls off (one Neon square at several Sizes), how
  `Intensity` scales it, and whether two Blooms add or one wins.
- Neon's glow with no Bloom, at Automatic and Level21: whether it shows,
  its reach and strength.
- SunRays: the halo's width for each `Spread` and its brightness for each
  `Intensity` (the open sky around the sun at several settings), how it
  falls off with the sun off screen or behind the camera, whether it
  comes from the moon at night, and what two effects do together.
- DepthOfField: the blur's radius against distance for each property
  (a row of checkered boards receding from the camera at several
  settings), whether the sky blurs, how its intensities scale (and
  whether they stop at 1), and which of two effects draws.
- Which of several ColorGradingEffects picks the curve.
