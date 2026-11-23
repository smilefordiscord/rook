// Lighting's Atmosphere, in the form Studio's shaders give it
// (`shaders_glsl3.pack`: `AdvSky`, and the surface shaders' fog;
// docs/behavior/lighting/atmosphere.md). The sky keeps a share of itself
// that falls towards the horizon, the rest becoming a haze: the sky's
// brightness times the Atmosphere's color, plus a glare around the sun whose
// falloff differs per channel. Surfaces keep a share that falls with
// distance and take the hazed sky behind them, read from the reflection
// cube at least as blurred as `Offset` says. What the six properties
// become is `rook-scene`'s (`atmosphere.rs`).

import package::common::{g, sky_cube, image_sampler, sky_dir};
import package::tone::light_units3;
import package::images::plain_sky_toward;
import package::sky::night::night_sky;

fn has_atmosphere() -> bool {
    return g.atmosphere_color.w > 0.0;
}

// The share of the sky towards `d` that the haze leaves: `AdvSky`'s
// exp2(a / (0.001 + y^b)), with b = 1 here. 1 without an Atmosphere.
fn haze_kept(d: vec3<f32>) -> f32 {
    if !has_atmosphere() {
        return 1.0;
    }
    return clamp(exp2(g.atmosphere.z / (0.001 + max(d.y, 1e-5))), 0.0, 1.0);
}

// `sky` (light units, before the exposure) towards `d` as the haze leaves
// it.
fn hazed(d: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    if !has_atmosphere() {
        return sky;
    }
    let kept = haze_kept(d);
    // The light around: Studio's is a six-sided ambient cube the CPU
    // fills; the brightness of Rook's sky colors overhead and at the
    // horizon stands for it, so the haze takes Color's hue rather than
    // the sky's (Approximation).
    let sky_light = light_units3(plain_sky_toward(d)) * g.sky_tint.rgb + night_sky(d);
    let around = dot(sky_light, vec3(0.2126, 0.7152, 0.0722));
    // Kept from rounding below 0 opposite the sun: `pow` of a negative
    // number is undefined.
    let toward_sun = max(0.5 + 0.5 * dot(d, g.sun_dir.xyz), 0.0);
    let lobe = clamp(pow(vec3(toward_sun), g.atmosphere_glare.xyz) * (1.0 - kept), vec3(0.0), vec3(1.0));
    let haze = around * g.atmosphere_color.rgb + g.sun_color.rgb * lobe * g.atmosphere.w;
    return mix(haze, sky, kept);
}

// The share of a surface at `world` the Atmosphere leaves.
fn atmosphere_kept(world: vec3<f32>) -> f32 {
    let dist = distance(world, g.camera_pos.xyz);
    return clamp(exp2(-g.atmosphere.x * dist), 0.0, 1.0);
}

// What the Atmosphere fogs a surface at `world` towards, keeping `kept` of
// it: the hazed sky behind it, from the reflection cube at level
// max(blur, kept) × 5 (Studio's surface shaders read their cube so).
fn atmosphere_fog(world: vec3<f32>, kept: f32) -> vec3<f32> {
    let d = normalize(world - g.camera_pos.xyz);
    let level = max(g.atmosphere.y, kept) * 5.0;
    let sky = textureSampleLevel(sky_cube, image_sampler, sky_dir(d), level).rgb * g.sky_tint.rgb;
    return hazed(d, sky + night_sky(d));
}
