// What every post pass shares (`renderer/passes/post.rs`): one triangle
// over the whole target.

// The `i`th corner of a triangle covering clip space.
fn screen_corner(i: u32) -> vec4<f32> {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    return vec4(xy, 0.0, 1.0);
}
