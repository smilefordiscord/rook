// DepthOfFieldEffect's circles of confusion
// (`renderer/passes/post/depth_of_field.rs`), shared by its passes and the
// tone pass.

// The main pass's near plane, in studs (reverse Z: depth is near over the
// distance, 0 for the sky; `scene.rs`'s `Camera::projection`).
const NEAR: f32 = 0.1;

// How far a depth buffer value is from the camera, in studs.
fn distance_of(depth: f32) -> f32 {
    return select(NEAR / depth, 1.0e30, depth <= 0.0);
}

// How blurred a surface `z` studs away is beyond the focus (x its
// distance, y its radius), 0 to 1: as a lens's circle of confusion, 0 at
// the focus's far edge and 1 at infinity (Approximation: Studio's circle
// for a distance isn't measured).
fn far_circle(z: f32, focus: vec2<f32>) -> f32 {
    return clamp(1.0 - (focus.x + focus.y) / z, 0.0, 1.0);
}

// The same before the focus: 0 at its near edge, 1 at half that distance.
fn near_circle(z: f32, focus: vec2<f32>) -> f32 {
    let edge = focus.x - focus.y;
    return select(0.0, clamp(edge / z - 1.0, 0.0, 1.0), edge > 0.0);
}
