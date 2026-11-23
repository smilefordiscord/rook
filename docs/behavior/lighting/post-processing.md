# Post-processing

Checked against Studio 0.748 on 2026-11-23, by its shaders and the creator docs only: nothing on this page is measured in Studio yet.

The PostEffects (ColorCorrectionEffect, BlurEffect, BloomEffect,
SunRaysEffect, DepthOfFieldEffect and ColorGradingEffect) change the
rendered world after it's drawn, never the GUI, and only while `Enabled`
and parented to Lighting or to `Workspace.CurrentCamera` (Confirmed:
creator-docs PostEffect). Effects under Lighting show to every player;
under the Camera, to its player alone (Confirmed: the post-processing
guide). Some draw differently or not at all at low quality levels
(Confirmed: creator-docs PostEffect, unmeasured); Rook draws every level
the same until it has a quality setting (M13 step 4).

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
enabled.

## What Studio must settle

- ColorCorrection: Rec. 709 or other luma weights; the order of
  brightness, contrast, saturation and tint; contrast's pivot; how two
  effects combine (a gray ramp and a color chart under each property
  alone, then two effects at once).
- Which of several ColorGradingEffects picks the curve.
