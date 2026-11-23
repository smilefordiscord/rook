// BlurEffect (`renderer/passes/post/blur.rs`): a box-filtered copy of the
// frame, then a Gaussian across and down it, all in the frame's encoding,
// as Studio's blur shaders read it (`shaders_glsl3.pack`).

import package::post::screen::screen_corner;

struct Pass {
    // xy: one source texel along the pass's axis, in texture
    // coordinates; z: sigma in texels; w: tap pairs.
    step: vec4<f32>,
    // x: source texels per texel drawn, each way (the copy).
    down: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Pass;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(3) var filtered: sampler;

@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return screen_corner(i);
}

// The average of the source texels under this one.
@fragment
fn fs_down(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let d = i32(params.down.x);
    let first = vec2<i32>(at.xy) * d;
    let last = vec2<i32>(textureDimensions(source)) - 1;
    var sum = vec4(0.0);
    for (var y = 0; y < d; y++) {
        for (var x = 0; x < d; x++) {
            sum += textureLoad(source, min(first + vec2(x, y), last), 0);
        }
    }
    return sum / f32(d * d);
}

// A Gaussian along `params.step`: the center texel, then each pair of
// neighbors (2k - 1 and 2k texels out) read by one filtered tap placed
// between them by their weights, as Studio's blur shaders do.
@fragment
fn fs_blur(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = at.xy / vec2<f32>(textureDimensions(source));
    let along = params.step.xy;
    let spread = 2.0 * params.step.z * params.step.z;
    var sum = textureSampleLevel(source, filtered, uv, 0.0);
    var total = 1.0;
    for (var k = 1; k <= i32(params.step.w); k++) {
        let near = f32(2 * k - 1);
        let far = f32(2 * k);
        let a = exp(-near * near / spread);
        let b = exp(-far * far / spread);
        let offset = along * (near + b / (a + b));
        let pair = textureSampleLevel(source, filtered, uv + offset, 0.0)
            + textureSampleLevel(source, filtered, uv - offset, 0.0);
        sum += pair * (a + b);
        total += 2.0 * (a + b);
    }
    return sum / total;
}
