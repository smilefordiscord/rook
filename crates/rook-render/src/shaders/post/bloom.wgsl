// BloomEffect's chain (`renderer/passes/post/bloom.rs`), in light units:
// the frame's light over the threshold into a half-size copy, each smaller
// copy the average of the one before, and back up by a tent, as Studio's
// bloom shaders draw it (`shaders_glsl3.pack`).

import package::post::screen::screen_corner;
import package::tone::FRAME_LIGHT;

struct Pass {
    // xy: one texel of the texture read, zw: one texel of the texture
    // drawn, in texture coordinates.
    texels: vec4<f32>,
    // x: the threshold in light units; y: the intensity; z: what the
    // tent of `first` is scaled by; w: what `second` is scaled by.
    weights: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Pass;
@group(0) @binding(1) var first: texture_2d<f32>;
@group(0) @binding(2) var second: texture_2d<f32>;
@group(0) @binding(3) var filtered: sampler;

@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return screen_corner(i);
}

// `first` filtered at this texel's middle and the eight one texel of
// `first` around it, each through `decode`, averaged: Studio's 3 × 3
// downsample.
fn average(uv: vec2<f32>, decode: bool) -> vec3<f32> {
    var sum = vec3(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let at = uv + vec2(f32(x), f32(y)) * params.texels.xy;
            let v = textureSampleLevel(first, filtered, at, 0.0).rgb;
            sum += select(v, v * v * FRAME_LIGHT, decode);
        }
    }
    return sum / 9.0;
}

// The frame's light over the threshold: each texel scaled by how far its
// brightest channel is over it (Studio's `DownSampleGjoelFS`), times the
// intensity.
@fragment
fn fs_extract(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let light = average(at.xy * params.texels.zw, true);
    let brightest = max(light.r, max(light.g, light.b));
    let over = max(brightest - params.weights.x, 0.0) / (brightest + 0.001);
    return vec4(min(light * over * params.weights.y, vec3(FRAME_LIGHT)), 1.0);
}

@fragment
fn fs_down(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4(average(at.xy * params.texels.zw, false), 1.0);
}

// A 3 × 3 tent of `first` (Studio's `UpSampleTentFS`) plus `second` at
// this texel, each scaled; the pass adds it to what's drawn.
@fragment
fn fs_up(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy * params.texels.zw;
    var tent = vec3(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let weight = f32((2 - abs(x)) * (2 - abs(y)));
            let tap = uv + vec2(f32(x), f32(y)) * params.texels.xy;
            tent += textureSampleLevel(first, filtered, tap, 0.0).rgb * weight;
        }
    }
    let own = textureSampleLevel(second, filtered, uv, 0.0).rgb;
    return vec4(tent / 16.0 * params.weights.z + own * params.weights.w, 1.0);
}
