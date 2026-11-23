// SunRaysEffect (`renderer/passes/post/sun_rays.rs`), in light units at
// half the frame's size: each pixel's share of steps towards the sun that
// reach the sky, times a lobe around the sun, then streaked towards the
// sun, as Studio's sun rays and radial blur shaders draw them
// (`shaders_glsl3.pack`).

import package::post::screen::screen_corner;

struct Pass {
    // From clip space to directions from the camera.
    from_clip: mat4x4<f32>,
    // xy: the sun's place in texture coordinates; zw: one texel of the
    // texture drawn.
    sun: vec4<f32>,
    // xyz: towards the sun; w: the lobe's anisotropy.
    direction: vec4<f32>,
    // rgb: the light at the sun, in light units; w: steps.
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Pass;
@group(0) @binding(1) var marched: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var filtered: sampler;

// The main pass's near plane, in studs (reverse Z: depth is near over the
// distance, 0 for the sky; `scene.rs`'s `Camera::projection`).
const NEAR: f32 = 0.1;
// What counts as sky: Studio's shader tests its linear depth (studs times
// 0.002) against 0.999.
const SKY_STUDS: f32 = 499.5;

@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return screen_corner(i);
}

// The share of `to` (from `uv`) that stays on the screen, at most all of
// it: Studio's rays stop at the screen's edge.
fn on_screen(uv: vec2<f32>, to: vec2<f32>) -> f32 {
    var reach = 1.0;
    for (var i = 0; i < 2; i++) {
        if to[i] > 0.0 {
            reach = min(reach, (1.0 - uv[i]) / to[i]);
        } else if to[i] < 0.0 {
            reach = min(reach, -uv[i] / to[i]);
        }
    }
    return reach;
}

@fragment
fn fs_march(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy * params.sun.zw;
    let to_sun = params.sun.xy - uv;
    let steps = params.light.w;
    let step = to_sun * 0.5 * on_screen(uv, to_sun) / steps;
    // Interleaved gradient noise shifts each pixel's steps, as Studio's
    // noise texture does.
    let jitter = fract(52.9829189 * fract(dot(at.xy, vec2(0.06711056, 0.00583715))));
    let size = vec2<i32>(textureDimensions(depth));
    var sky = 0.0;
    for (var i = 0; i < i32(steps); i++) {
        let p = uv + step * (f32(i) + jitter);
        let texel = clamp(vec2<i32>(p * vec2<f32>(size)), vec2(0), size - 1);
        sky += select(0.0, 1.0, textureLoad(depth, texel, 0) <= NEAR / SKY_STUDS);
    }
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let p = params.from_clip * vec4(ndc, 1.0, 1.0);
    let toward = dot(normalize(p.xyz / p.w), params.direction.xyz);
    // A Henyey-Greenstein lobe, as Studio's shader weighs its rays, scaled
    // to 1 at the sun.
    let g = params.direction.w;
    let lobe = pow(1.0 - g, 3.0) / pow(max(1.0 + g * g - 2.0 * g * toward, 1e-6), 1.5);
    return vec4(params.light.rgb * (sky / steps) * lobe, 1.0);
}

// Six taps from here a quarter of the way towards the sun, averaged
// (Studio's `RadialBlur6FS`).
@fragment
fn fs_streak(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy * params.sun.zw;
    let to_sun = params.sun.xy - uv;
    let step = to_sun * 0.5 * on_screen(uv, to_sun) / 12.0;
    var sum = vec3(0.0);
    for (var i = 0; i < 6; i++) {
        sum += textureSampleLevel(marched, filtered, uv + step * f32(i), 0.0).rgb;
    }
    return vec4(sum / 6.0, 1.0);
}
