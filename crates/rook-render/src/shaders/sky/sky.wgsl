// The skybox: a full-screen triangle per image (the instance is the image),
// at the far plane.

import package::common::{g, sky_dir};
import package::tone::{albedo_units, frame, light_units3, srgb_to_linear};
import package::images::{sample_sky, sky_uv};

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
    @location(1) @interpolate(flat) face: u32,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32, @builtin(instance_index) face: u32) -> SkyOut {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: SkyOut;
    out.clip = vec4(xy, 0.0, 1.0);
    out.ndc = xy;
    out.face = face;
    return out;
}

// One skybox image: the pixels whose direction is closest to its face's,
// mapped straight onto the cube face (unlit and unfogged), tinted for the
// time of day (`g.sky_tint`) and scaled by the exposure in light units, as
// Studio's shaders scale everything.
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let p = g.sky_from_clip * vec4(in.ndc, 1.0, 1.0);
    let d = normalize(p.xyz / p.w);
    let along = dot(d, g.sky_faces[in.face * 3u].xyz);
    let texel = sample_sky(in.face, sky_uv(in.face, d));
    // In the skybox's own axes, as its faces are turned.
    let a = abs(sky_dir(d));
    if along < max(a.x, max(a.y, a.z)) - 1e-6 {
        discard;
    }
    return vec4(frame(light_units3(texel.rgb) * g.fog_range.z * g.sky_tint.rgb), 1.0);
}

// The plain sky, before the skybox's images are up (or without any): its
// color at the horizon over the whole screen.
@fragment
fn fs_plain_sky(in: SkyOut) -> @location(0) vec4<f32> {
    return vec4(frame(light_units3(srgb_to_linear(g.sky_horizon.rgb)) * g.fog_range.z * g.sky_tint.rgb), 1.0);
}

struct DiscOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) body: u32,
};

// The sun (instance 0) or the moon (1): a square facing the camera at
// infinity, spanning its angular size, at the far plane as Studio's
// `AdvSun` puts it (z = 0, reverse Z here).
@vertex
fn vs_disc(@builtin(vertex_index) i: u32, @builtin(instance_index) body: u32) -> DiscOut {
    let corners = array(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    let corner = corners[i];
    let at = g.discs[body * 2u];
    let c = at.xyz;
    let side = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(c.y) > 0.99);
    let right = normalize(cross(side, c));
    let up = cross(c, right);
    var out: DiscOut;
    // A direction, not a point: the camera's place doesn't move it.
    out.clip = g.view_proj * vec4(c + (corner.x * right + corner.y * up) * at.w, 0.0);
    out.clip.z = 0.0;
    out.uv = vec2(0.5 + 0.5 * corner.x, 0.5 - 0.5 * corner.y);
    out.body = body;
    return out;
}

// Rook's own disc, for an image that won't load: white to 0.3 of the way
// to the quad's edge, then fading out by 0.5.
fn own_disc(uv: vec2<f32>) -> f32 {
    return 1.0 - smoothstep(0.3, 0.5, length(uv - 0.5));
}

// Added over the sky, in the frame's encoding as Studio's sun and stars
// blend: the image's sRGB color squared (as Studio's shaders take a
// texture) times the body's light, then the exposure.
@fragment
fn fs_disc(in: DiscOut) -> @location(0) vec4<f32> {
    let look = g.discs[in.body * 2u + 1u];
    // Sampled first: a texture read must be in uniform control flow.
    let image = albedo_units(sample_sky(6u + in.body, in.uv).rgb);
    let shade = select(image, vec3(own_disc(in.uv)), look.w > 1.5);
    return vec4(frame(shade * look.rgb * g.fog_range.z), 0.0);
}
