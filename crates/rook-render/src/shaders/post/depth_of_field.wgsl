// DepthOfFieldEffect (`renderer/passes/post/depth_of_field.rs`), at half
// the frame's size in light units: the frame with each texel's far
// circle, the far field gathered by each pixel's own circle, and the near
// field gathered outwards, a tap counting where its own circle reaches the
// pixel, as Studio's bokeh shaders count it (`shaders_glsl3.pack`).

import package::post::focus::{distance_of, far_circle, near_circle};
import package::post::screen::screen_corner;
import package::tone::FRAME_LIGHT;

struct Pass {
    // x: FocusDistance; y: InFocusRadius, in studs; z: the far field's
    // radius and w: the near field's, at a circle of 1, as shares of the
    // screen's height.
    focus: vec4<f32>,
    // xy: one texel of the texture drawn; z: the screen's height over
    // its width.
    texel: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Pass;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_2d;

const TAPS: i32 = 24;
// The golden angle, in radians, which spreads the taps evenly over a disk.
const GOLDEN: f32 = 2.39996323;

@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return screen_corner(i);
}

// How far the surface drawn at `uv` is from the camera, in studs.
fn distance_at(uv: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(depth));
    let texel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), size - 1);
    return distance_of(textureLoad(depth, texel, 0));
}

// The field texel at `uv`, unfiltered: filtering would mix a sharp
// surface's light into the blurred one behind it.
fn field_at(uv: vec2<f32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let texel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), size - 1);
    return textureLoad(source, texel, 0);
}

// The `i`th tap's place in a unit disk.
fn tap(i: i32) -> vec2<f32> {
    let r = sqrt((f32(i) + 0.5) / f32(TAPS));
    let a = f32(i) * GOLDEN;
    return r * vec2(cos(a), sin(a));
}

// One frame texel of the two by two under this one, with its own
// circle, so its light and its depth agree.
@fragment
fn fs_field(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(at.xy) * 2;
    let v = textureLoad(source, texel, 0).rgb;
    let z = distance_of(textureLoad(depth, texel, 0));
    return vec4(v * v * FRAME_LIGHT, far_circle(z, params.focus.xy));
}

// The far field: taps over the pixel's own circle, each counting, by its
// own circle, if that reaches back to the pixel, so in-focus things in
// front don't bleed into it. Weighted by the pixel's own circle, so the
// tone pass, filtering it, takes the blurred background alone.
@fragment
fn fs_far(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy * params.texel.xy;
    let center = field_at(uv);
    let own = center.a;
    let scale = vec2(params.texel.z, 1.0) * own * params.focus.z;
    var sum = center.rgb * own;
    var total = own;
    for (var i = 0; i < TAPS; i++) {
        let o = tap(i);
        let s = field_at(uv + o * scale);
        if s.a >= length(o) * own {
            sum += s.rgb * s.a;
            total += s.a;
        }
    }
    return vec4(select(center.rgb, sum / total, total > 0.0) * own, own);
}

// The near field: taps over the widest circle, each counting if its own
// near circle reaches the pixel; how many do is how much of the pixel the
// near field covers.
@fragment
fn fs_near(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy * params.texel.xy;
    let scale = vec2(params.texel.z, 1.0) * params.focus.w;
    var sum = vec3(0.0);
    var total = 0.0;
    for (var i = 0; i < TAPS; i++) {
        let o = tap(i);
        let p = uv + o * scale;
        let reach = near_circle(distance_at(p), params.focus.xy);
        if reach >= length(o) && reach > 0.0 {
            sum += field_at(p).rgb * reach;
            total += reach;
        }
    }
    let own = field_at(uv).rgb;
    return vec4(select(own, sum / total, total > 0.0), total / f32(TAPS));
}
