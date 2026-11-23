# M13 Lighting and post-processing

Make places look the way Studio shows them today, at both ends of its
graphics quality: the default (`EditQualityLevel` Automatic, what
archive places are mostly seen at) and the top (Level21, with
`PrioritizeLightingQuality` on). Post effects in the corpus: Bloom 49
files, ColorCorrection 44, SunRays 20, Blur 13, DepthOfField 11;
Atmosphere 16, Clouds 8.

Rook has no quality setting today and draws Automatic everywhere
([user settings](../behavior/user-settings.md): Graphics Quality is
saved with nothing behind it). Step 4 adds it; steps 1 to 3 measure
what they build at both Automatic and Level21, so nothing has to be
measured twice.

1. **The sun, moon and stars**, `SkyboxOrientation`, Atmosphere and
   Clouds.
   - *Scope*: the night sky Rook draws black today (Studio's dark
     gradient, its purple horizon glow and the stars,
     [sky](../behavior/lighting/sky.md#not-done)); `CelestialBodiesShown`,
     `StarCount`, `SunTextureId`, `MoonTextureId`, `SunAngularSize`,
     `MoonAngularSize`; Atmosphere's `Density`, `Offset`, `Color`,
     `Decay`, `Glare` and `Haze` over parts, terrain and the sky; Clouds'
     `Cover`, `Density` and `Color` and their drift with `GlobalWind`.
     Out: fog's existing rules ([tone mapping](../behavior/lighting/tone-mapping.md#fog)).
   - *Studio questions*: whether the sun disc, the stars, Atmosphere and
     Clouds draw differently at Level21 than at Automatic (detail, steps
     through the haze, cloud shadows on the ground); how Atmosphere and
     the old fog combine when a place has both.
   - *Design* (2026-11-23), one per part, in the order they land. Where
     Studio's rule is in the install's shader pack (`shaders_glsl3.pack`:
     `AdvSun`, `AdvStars`, `AdvSky`, `Sky`, `Stars` and the surface
     shaders' fog), Rook takes its form; what the CPU feeds those shaders
     isn't in the pack, so the constants are Approximations until Studio
     captures settle them ([sky](../behavior/lighting/sky.md)).
     - *The sun and moon*: engines draw celestial bodies as camera-facing
       quads at infinity in the sky pass (Unity's procedural skybox sun,
       Unreal's `SkyAtmosphere` disc, Godot's `sky` shader suns), not as
       geometry in the world: they can't be occluded by distance, need no
       depth and cost two quads. Studio's `AdvSun` draws a textured quad at
       the far plane (z forced to 0) and blends it over the sky; Rook does
       the same in its sky pass: two instances of one quad pipeline, each
       a direction from the clock and latitude Rook already turns into the
       sun's light (one owner, `sun_direction`), a size from
       `SunAngularSize`/`MoonAngularSize` and its image from the image
       table (or bound per draw without one). `SkyboxOrientation` turns
       only the skybox (creator docs: celestial bodies don't turn): the
       six faces' axes in the globals turn, and lookups into the
       reflection cube turn the same way, as Studio's surface shaders turn
       theirs (`SkyboxRotation`), so the cube is never rebuilt for it.
     - *Stars and the night sky*: engines draw stars from a seeded point
       set (Unity's and Godot's procedural skies hash a direction per
       star; offline renderers bake a cube texture). A cube texture
       costs memory and resolution; Studio's `AdvStars` draws points two
       pixels across, so the count is its `StarCount`. Rook makes each
       star's direction and brightness from a hash of its index in the
       vertex shader: one instanced draw, no buffer, turned about the
       sun's axis with the clock. The night gradient and horizon glow are
       a term of the sky shader that fades in as the time of day's tint
       fades out.
     - *Atmosphere*: physically based engines march or precompute
       scattering (Hillaire 2020's sky-atmosphere LUTs in Unreal's
       `SkyAtmosphere`; Bruneton 2008), with fog as a separate height
       fog (Unreal's `ExponentialHeightFog`). Roblox's model is smaller,
       and its shaders show its form: the sky is mixed towards an
       ambient-cube haze lit by the sky, plus a glare lobe around the sun
       with an exponent per channel, by a transmittance
       `exp2(a / (0.001 + y^b))` that falls to the horizon; surfaces keep
       `exp2(z × distance)` of themselves and take the sky behind them,
       read from the prefiltered reflection cube at a blur level of at
       least `Offset`, in place of `FogColor` (so an Atmosphere replaces
       Lighting's fog, whose properties Studio hides). Rook takes that
       form rather than a scattering simulation: it is what Studio draws,
       and it costs a few terms per pixel and one cube read where fog
       already reads its color. One owner maps the six properties onto
       the form's parameters (`rook-scene`), and the shaders read them
       from the globals.
     - *Clouds*: engines raymarch a 2D coverage map with 3D noise
       (Guerrilla's Nubis in Horizon Zero Dawn; Unreal's volumetric
       clouds), at reduced resolution with temporal reprojection; a
       cheaper engine draws a scrolling 2D layer. Studio's own pack
       shows the volumetric kind (`CloudsDist`, `CloudsWind`,
       `CloudsReprojectFilter`, the install's `sky/clouds.dds`,
       `cloudDetail3D.dds`). Rook will raymarch the same way at quarter
       resolution with reprojection, wind advancing the coverage by
       `GlobalWind`, the Clouds' properties read like the Sky's. It
       lands after the three above.
     - *Changes*: everything here is Lighting and its children, so it
       flows from the change journal as the sky does: a property of
       Lighting, a Sky or an Atmosphere marks the lighting changed, and
       the scene reads the Lighting service again (a handful of
       properties, never the place). A sun or moon image arriving marks it
       too, and so does a Clouds in a Terrain, which the scene reads with
       Lighting. Per frame: two quads, `StarCount` tiny quads, and a few terms
       in the sky and fog; nothing scales with the place.
   - *So far*: all four parts are built in Studio's forms
     ([sky](../behavior/lighting/sky.md),
     [Atmosphere](../behavior/lighting/atmosphere.md),
     [clouds](../behavior/lighting/clouds.md)). The clouds take all four
     of Studio's passes, temporal reprojection included (its blend of up
     to seven frames at where each cloud was), with the install's cloud
     textures or Rook's own, and nothing drawn or made without Clouds.
     Left: the Studio captures that set what the CPU feeds the shaders
     (every Atmosphere and Clouds mapping, the sun and moon's brightness,
     the night sky's colors), clouds' shadows on the ground, and the sky's
     extras in reflections.
   - *Done when*: day, dusk and night captures of the Baseplate, with and
     without an Atmosphere and Clouds, match Studio at Automatic and at
     Level21 by the [Match Studio](README.md#match-studio) bar.
2. **Post-processing** on M7's HDR target: Bloom, ColorCorrection,
   SunRays, Blur, DepthOfField, and Neon's glow.
   - *Scope*: each effect's properties and `Enabled`; several of one kind
     at once (whether they stack or one wins); effects under the Camera
     as well as under Lighting; the order they apply in against tone
     mapping. Neon glows at high quality levels
     ([antialiasing](../behavior/rendering/antialiasing.md) notes it
     blurs edges there), so whether Neon's glow is Bloom or its own pass
     belongs here.
   - *Studio questions*: which effects change or turn off by quality
     level (DepthOfField and SunRays in particular), and the level each
     one starts at; Neon's glow with no Bloom in Lighting, at Automatic
     and at Level21.
   - *Done when*: one capture per effect at a few settings each, at
     Automatic and Level21, matches Studio.
3. **Studio's one lighting system** (`Lighting.Technology` is gone):
   `LightingStyle`, `PrioritizeLightingQuality`, `ShadowSoftness`,
   `EnvironmentDiffuseScale` and `EnvironmentSpecularScale` for M4's
   PBR materials.
   - *Scope*: sun shadows both ways
     ([sun](../behavior/lighting/sun.md#shadows)): with
     `PrioritizeLightingQuality` on, Studio's sharper edges and reach of
     about 580 studs where Rook's one 2048² cascade reaches 300; with it
     off, the light voxels' blocker fit (a 4-stud slab 20 studs up
     darkens about 133 levels in Rook against 50 in Studio) and parts'
     real shapes in the voxels. Also the sky light blocked by parts'
     shapes instead of their boxes, the indoor reflection cube and the
     reflections' fade with distance, and terrain lit as Studio lights
     it (the stubs rows naming this step).
   - *Studio questions*: how shadow reach, cascades, edge sharpness and
     the light voxel grid change with the quality level; whether
     `PrioritizeLightingQuality` does anything at low levels; what
     `LightingStyle` Realistic changes against Soft.
   - *Done when*: reference places and the sun shadow test scenes match
     Studio with `PrioritizeLightingQuality` off at Automatic and on at
     Level21, and no stubs row names this step.
4. **Graphics quality**: a quality setting that follows Studio's
   levels, set from the in-game Graphics Quality slider, and everything
   it switches.
   - *Scope*: the levels from Automatic to Level21 and the slider's 1 to
     10 onto them. What is known to switch: antialiasing (none below
     Level17, 2× MSAA at Level17, 4× from Level18,
     [antialiasing](../behavior/rendering/antialiasing.md#for-rook));
     water's screen-space reflections from Level16
     ([water](../behavior/terrain/water.md#reflections-at-high-quality));
     Glass's full shader at Level21, with refraction and a sharp round
     glint ([materials](../behavior/rendering/materials.md)). Plus
     whatever steps 1 to 3 find switching. Out: view distance and how far
     small parts are culled at lower quality
     ([file meshes](../behavior/meshes/file-meshes.md); M21 step 2).
   - *Studio questions*: how the slider's 1 to 10 map onto the levels;
     levels 11 to 15 and 20, assumed from their neighbors; what else
     changes between Level16 and Level21 (texture and shadow
     resolution, particle counts, grass reach), by side-by-side
     captures of one scene at each level.
   - *Done when*: the Baseplate test scenes and reference places match
     Studio at Automatic, Level10, Level16 and Level21, and the slider
     moves Rook between them while playing, without reloading the place.
5. **Match Studio at Level21**: close every difference left in what
   steps 1 to 4 built at the top quality level.
   - *Scope*: the Approximations in the lighting, sky, materials and
     water pages that only show at high quality; reflections, shadows
     and post effects together in one frame (their order and how they
     interact).
   - *Evidence*: the [stubs](../guide/stubs.md) rows naming this
     milestone, and open Approximations and Inferences on those pages.
   - *Done when*: screenshots of reference places at Level21 with
     `PrioritizeLightingQuality` on match Studio's by the
     [Match Studio](README.md#match-studio) bar, as they do at
     Automatic, and no stubs row names this milestone.
