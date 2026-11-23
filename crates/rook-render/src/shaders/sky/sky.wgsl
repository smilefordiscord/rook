// The skybox: a full-screen triangle per image (the instance is the image),
// at the far plane.

import package::common::{g, sky_dir};
import package::tone::{albedo_units, frame, light_units3, srgb_to_linear};
import package::images::{sample_sky, sky_uv};
import package::sky::night::night_sky;
import package::sky::atmosphere::{haze_kept, hazed};

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

// The direction from the camera through a point of the screen.
fn sky_direction(ndc: vec2<f32>) -> vec3<f32> {
    let p = g.sky_from_clip * vec4(ndc, 1.0, 1.0);
    return normalize(p.xyz / p.w);
}

// One skybox image: the pixels whose direction is closest to its face's,
// mapped straight onto the cube face (unlit and unfogged), tinted for the
// time of day (`g.sky_tint`) over the night's gradient, through the
// Atmosphere's haze, and scaled by the exposure in light units, as Studio's
// shaders scale everything.
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let d = sky_direction(in.ndc);
    let along = dot(d, g.sky_faces[in.face * 3u].xyz);
    let texel = sample_sky(in.face, sky_uv(in.face, d));
    // In the skybox's own axes, as its faces are turned.
    let a = abs(sky_dir(d));
    if along < max(a.x, max(a.y, a.z)) - 1e-6 {
        discard;
    }
    let units = hazed(d, light_units3(texel.rgb) * g.sky_tint.rgb + night_sky(d));
    return vec4(frame(units * g.fog_range.z), 1.0);
}

// The plain sky, before the skybox's images are up (or without any): its
// color at the horizon over the whole screen, with the night's gradient.
@fragment
fn fs_plain_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let d = sky_direction(in.ndc);
    let units = hazed(d, light_units3(srgb_to_linear(g.sky_horizon.rgb)) * g.sky_tint.rgb + night_sky(d));
    return vec4(frame(units * g.fog_range.z), 1.0);
}

struct DiscOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) body: u32,
    // The share the Atmosphere's haze leaves of it.
    @location(2) @interpolate(flat) kept: f32,
};

// The sun (instance 0) or the moon (1): a square facing the camera at
// infinity, spanning its angular size, at the far plane as Studio's
// `AdvSun` puts it (z = 0, reverse Z here).
@vertex
fn vs_disc(@builtin(vertex_index) i: u32, @builtin(instance_index) body: u32) -> DiscOut {
    let corner = quad_corner(i);
    let at = g.discs[body * 2u];
    var out: DiscOut;
    out.clip = facing(at.xyz, at.w, corner);
    out.uv = vec2(0.5 + 0.5 * corner.x, 0.5 - 0.5 * corner.y);
    out.body = body;
    out.kept = haze_kept(at.xyz);
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
    return vec4(frame(shade * look.rgb * in.kept * g.fog_range.z), 0.0);
}

// Corner `i` of a square, two triangles.
fn quad_corner(i: u32) -> vec2<f32> {
    var corners = array(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    return corners[i];
}

// A corner of a square facing the camera around direction `c` (unit),
// reaching `half` (the tangent of its half-angle) from it, at the far
// plane: a direction, not a point, so the camera's place doesn't move it.
fn facing(c: vec3<f32>, half: f32, corner: vec2<f32>) -> vec4<f32> {
    let side = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(c.y) > 0.99);
    let right = normalize(cross(side, c));
    let up = cross(c, right);
    var clip = g.view_proj * vec4(c + (corner.x * right + corner.y * up) * half, 0.0);
    clip.z = 0.0;
    return clip;
}

// PCG's hash (Jarzynski and Olano 2020): the stars' directions and
// brightnesses come from their index, the same every frame.
fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn unit(v: u32) -> f32 {
    return f32(v) / 4294967295.0;
}

struct StarOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) light: f32,
};

// Star `star`: a direction spread evenly over the sphere, turned about Z
// with the time of day as the sun turns, as a square two pixels across
// (Studio's `AdvStars` draws two-pixel points), and a brightness from a
// tenth of the brightest up (Approximation: Studio's stars' colors are
// vertex data the pack doesn't hold).
@vertex
fn vs_star(@builtin(vertex_index) i: u32, @builtin(instance_index) star: u32) -> StarOut {
    let a = pcg(star);
    let b = pcg(a);
    let z = unit(a) * 2.0 - 1.0;
    let around = unit(b) * 6.2831853 + g.stars.y;
    let ring = sqrt(max(1.0 - z * z, 0.0));
    let d = vec3(ring * cos(around), ring * sin(around), z);
    var out: StarOut;
    // A pixel spans `fog_range.w` studs one stud away.
    out.clip = facing(d, g.fog_range.w, quad_corner(i));
    out.light = g.stars.z * mix(0.1, 1.0, unit(pcg(b))) * haze_kept(d);
    return out;
}

// Added over the sky, as the sun is.
@fragment
fn fs_star(in: StarOut) -> @location(0) vec4<f32> {
    return vec4(frame(vec3(in.light * g.fog_range.z)), 0.0);
}
