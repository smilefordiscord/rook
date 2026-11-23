// The night sky's gradient: dark overhead with a glow along the horizon,
// showing as the skybox's tint fades (docs/behavior/lighting/sky.md,
// "The night sky"). Not turned by SkyboxOrientation: it isn't a skybox
// image.

import package::common::g;
import package::tone::{light_units3, srgb_to_linear};

// How far above the horizon the glow reaches: it falls to 1/e at this
// height (the sine of the elevation; Approximation).
const GLOW_HEIGHT: f32 = 0.15;

// The gradient's light units towards `d`, before the exposure: the horizon
// color at and below the horizon, the top color overhead.
fn night_sky(d: vec3<f32>) -> vec3<f32> {
    let shown = g.night_top.w;
    if shown <= 0.0 {
        return vec3(0.0);
    }
    let glow = exp(-max(d.y, 0.0) / GLOW_HEIGHT);
    let look = mix(g.night_top.rgb, g.night_horizon.rgb, glow);
    return light_units3(srgb_to_linear(look)) * shown;
}
