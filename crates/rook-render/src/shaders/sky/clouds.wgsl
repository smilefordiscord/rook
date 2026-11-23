// Terrain's Clouds (docs/behavior/lighting/clouds.md), in the form of
// Studio's cloud shaders (`shaders_glsl3.pack`, read for facts): a layer of
// cloud from 2 to 4.5 km up a planet's curve, read from a field repeating
// every 32 km that is pushed along by a flow and roughened by detail
// noise, cloud wherever it's below Cover. Four passes: a shadow map of the
// layer from the sun, the march along each ray through the layer at a
// quarter of the frame's pixels, a blend with last frames' clouds where
// they were, and those drawn over the sky. The shapes of the formulas and
// their constants are the pack's; what the properties become is
// `lighting/clouds.rs`'s.

import package::common::g;
import package::tone::frame;
import package::lighting::sky_light;
import package::sky::atmosphere::{atmosphere_fog_toward, has_atmosphere};

// `Params` in `lighting/clouds.rs`.
struct Clouds {
    // x = Cover, y = Density thinned by the camera's height, z = steps,
    // w = the detail's strength.
    shape: vec4<f32>,
    // Color in light units; w = how fine the detail is.
    color: vec4<f32>,
    // How far the wind has carried the field (km); w = the change of shape.
    drift: vec4<f32>,
    // The march's size in pixels, then the frame's.
    sizes: vec4<f32>,
    // World to last frame's clip space.
    last_view_proj: mat4x4<f32>,
    // x: 1 when last frame's clouds can't be blended in.
    last: vec4<f32>,
};

@group(2) @binding(0) var<uniform> c: Clouds;
// The shadow map and the march.
@group(2) @binding(1) var repeat_sampler: sampler;
// Repeating across, clamped through the layer.
@group(2) @binding(2) var layer_sampler: sampler;
@group(2) @binding(3) var field: texture_3d<f32>;
@group(2) @binding(4) var detail: texture_3d<f32>;
@group(2) @binding(5) var flow: texture_2d<f32>;
// The march's: the shadow map over one tile of the field.
@group(3) @binding(0) var shadow_map: texture_2d<f32>;
// The blend's and drawing over the sky's.
@group(2) @binding(10) var clamp_sampler: sampler;
@group(2) @binding(11) var march_color: texture_2d<f32>;
@group(2) @binding(12) var march_dist: texture_2d<f32>;
@group(2) @binding(13) var last_color: texture_2d<f32>;
@group(2) @binding(14) var last_dist: texture_2d<f32>;
@group(2) @binding(15) var blended: texture_2d<f32>;

// The layer's units are kilometers; 1 stud is 0.28 m.
const KM_PER_STUD: f32 = 0.00028;
const STUDS_PER_KM: f32 = 3571.4286;
// The planet's center this far below the origin, and the layer's bottom
// and top as radii from it (km).
const PLANET_DEPTH: f32 = 971.0;
const LAYER_BOTTOM: f32 = 973.0;
const LAYER_TOP: f32 = 975.5;
// The field repeats every this many km across.
const FIELD_TILE: f32 = 32.0;
// Studio's march reads a blue-noise texture at each pixel's coordinate in
// texels as if they were normalized, so every pixel reads the same texel:
// about its average (Inference).
const NOISE: f32 = 0.5;

// Studio's phase function, by where the cosine between the view and the
// sun's light's travel falls from -1 to 1 (`shaders_glsl3.pack`).
const PHASE = array<f32, 48>(
    15.71791, 12.89409, 7.719984, 5.107763, 3.957092, 3.128542, 2.467976, 1.940606,
    1.519268, 1.172060, 0.9068110, 0.6966390, 0.5309800, 0.4037420, 0.3046740, 0.2300430,
    0.1724650, 0.1289850, 0.09606200, 0.07203500, 0.05467100, 0.04189300, 0.03290200, 0.02654900,
    0.02250100, 0.02016900, 0.01880300, 0.01828600, 0.01808600, 0.02294900, 0.02805700, 0.03309800,
    0.03307300, 0.03600000, 0.05108700, 0.1080680, 0.1728080, 0.2078000, 0.1323690, 0.1117660,
    0.1104010, 0.1052310, 0.1041890, 0.1084980, 0.1274740, 0.2075670, 0.3197790, 0.4679400,
);

struct ScreenOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// A triangle over the whole target, at the far plane.
@vertex
fn vs_clouds(@builtin(vertex_index) i: u32) -> ScreenOut {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: ScreenOut;
    out.clip = vec4(xy, 0.0, 1.0);
    out.ndc = xy;
    return out;
}

// The direction from the camera through a point of the screen.
fn ray(ndc: vec2<f32>) -> vec3<f32> {
    let p = g.sky_from_clip * vec4(ndc, 1.0, 1.0);
    return normalize(p.xyz / p.w);
}

fn uv_of(ndc: vec2<f32>) -> vec2<f32> {
    return vec2(0.5 + 0.5 * ndc.x, 0.5 - 0.5 * ndc.y);
}

// How far along `d` (km) the ray enters the layer's bottom and leaves its
// top, from the camera inside the planet's sphere.
fn layer_span(d: vec3<f32>) -> vec2<f32> {
    let o = g.camera_pos.xyz * KM_PER_STUD + vec3(0.0, PLANET_DEPTH, 0.0);
    let b = dot(d, o);
    let k = dot(o, o) - vec2(LAYER_BOTTOM * LAYER_BOTTOM, LAYER_TOP * LAYER_TOP);
    return -b + sqrt(max(b * b - k, vec2(0.0)));
}

// Nine samples or fewer read a level coarser.
fn coarse_level() -> f32 {
    return select(0.0, 1.0, c.shape.z <= 9.0);
}

// The field at `p` (km across, the height through the layer from 0 to 1
// as y), pushed along the flow, with the detail added: cloud where it's
// below Cover.
fn cloud_field(p: vec3<f32>, level: f32) -> f32 {
    let push = textureSampleLevel(flow, repeat_sampler, p.xz * 0.125 + vec2(0.0, -17.5 * c.drift.w), level).xy * 2.0 - 1.0;
    let along = vec3(push.x, 0.0, push.y);
    var q = p;
    q.x += 39.0625 * c.drift.w;
    // Denser clouds' detail is pushed less.
    let pulls = mix(vec2(0.15, 0.07), vec2(0.075, 0.05), vec2(c.shape.y));
    let rough = vec2(
        textureSampleLevel(detail, repeat_sampler, q * (0.5 * c.color.w) + along * pulls.x, level).x,
        textureSampleLevel(detail, repeat_sampler, q * (2.0 * c.color.w) + along * pulls.y, level).x,
    ) * 2.0 - 1.0;
    let at = (p + along * 0.75) * vec3(1.0 / FIELD_TILE, 1.0, 1.0 / FIELD_TILE);
    let coarse = textureSampleLevel(field, layer_sampler, at.xzy, level).x;
    return coarse + dot(rough, vec2(1.0, 0.35)) * c.shape.w;
}

// The shadow map, over one tile of the field: each texel's column marched
// towards the sun (no lower than 0.3 up) through the layer in 13 steps,
// against a Cover 4% higher, then back. x and z: 1 minus half the
// cloud summed to the last and first cloud met, y: the cloud met per step,
// w: the cloud met coming back.
@fragment
fn fs_cloud_shadow(in: ScreenOut) -> @location(0) vec4<f32> {
    let uv = uv_of(in.ndc);
    var l = g.sun_dir.xyz;
    l.y = select(max(0.3, l.y), min(-0.3, l.y), l.y < 0.0);
    l = normalize(l);
    let stride = l * ((1.0 / 12.0) / max(abs(l.y), 1e-4));
    let start = vec3(FIELD_TILE * uv.x, select(0.015625, 0.984375, l.y <= 0.0), FIELD_TILE * uv.y);
    let cover = 1.04 * c.shape.x;
    let level = coarse_level();
    var kept = 1.0;
    var sum = 0.0;
    var first = -1.0;
    var last = 0.0;
    var met = 1.0;
    for (var k = 0; k <= 12; k++) {
        let f = cloud_field(start + stride * f32(k), level);
        if f < cover && kept > 1e-5 {
            let s = c.shape.y * (cover - f);
            sum += s;
            last = sum * 1.4880953;
            if first < 0.0 {
                first = last;
            }
            kept *= exp2(-0.12022459 * s);
            met += 1.0;
        }
    }
    var back = 0.0;
    if met > 1.0 {
        let end = start + stride * 12.0;
        var kept_back = 1.0;
        for (var k = 0; k <= 12; k++) {
            let f = cloud_field(end - stride * f32(k), level);
            if f < cover {
                let s = c.shape.y * (cover - f);
                kept_back *= exp2(-0.12022459 * s);
                back += s;
                if kept_back <= 1e-5 {
                    break;
                }
            }
        }
    }
    return vec4(1.0 - 0.5 * last, sum / met, 1.0 - 0.5 * first, back);
}

// The phase function at `travel`, the cosine between the view and the
// sun's light's travel.
fn phase(travel: f32) -> f32 {
    var table = PHASE;
    let x = (0.5 + 0.5 * clamp(travel, -1.0, 1.0)) * 46.5;
    let i = min(u32(x), 46u);
    return mix(table[i], table[i + 1u], fract(x));
}

// The light around a cloud at height `h` through the layer, looking along
// `d`: Studio's is its two ambient cubes, lifted by a gray that brightens
// up the layer and the sky gradient, no less than Ambient and
// OutdoorAmbient; Rook's sky light and those two stand for the cubes
// (Approximation).
fn around(d: vec3<f32>, h: f32) -> vec3<f32> {
    let lift = mix(vec3(0.15 + 0.3 * h), g.sky_tint.rgb, vec3(clamp(exp2(-g.sun_dir.y), 0.0, 1.0)));
    let least = g.ambient.rgb * g.ambient.rgb + g.outdoor_ambient.rgb * g.outdoor_ambient.rgb;
    return max(lift * (sky_light(d) + least), least);
}

// The sun's light a cloud at height `h` scatters towards the camera: three
// octaves of its light through `to_sun` of cloud, by the phase function
// (`p`, and `travel` as for `phase`), the light from below through `back`
// of cloud, and a little more where the cloud is thicker (`step` of it in
// this step).
fn sunlit(to_sun: f32, travel: f32, p: f32, back: f32, step: f32, h: f32) -> vec3<f32> {
    let forward = clamp(travel * p * 0.125, 0.0, 1.0);
    let octaves = 0.29 * exp2(-23.70142 * to_sun)
        + 0.169 * mix(0.08, 1.0, smoothstep(0.0, 1.5, abs(travel))) * exp2(-659.5178 * to_sun)
        + (1.0 - c.shape.y) * 320.0 * forward * exp2(-10.0 * to_sun);
    let scattered = 4.0 * (octaves * p * 0.02925 + 0.092 * exp2(-2.325 * back));
    return g.sun_color.rgb * (scattered + 0.0425 + step * 0.135 * h) * 2.1;
}

struct MarchOut {
    // The light the clouds send along the ray (light units), and how much
    // of what's behind them they hide.
    @location(0) color: vec4<f32>,
    // How far along the ray the first cloud is (km), 0 for none.
    @location(1) dist: f32,
};

// Each ray above the horizon through the layer: `shape.z` steps from a
// fixed share of the way along it, each crossing the same height, the
// cloud's light and shade summed front to back, then fogged by the
// Atmosphere with the distance to the layer.
@fragment
fn fs_cloud_march(in: ScreenOut) -> MarchOut {
    var out: MarchOut;
    let d = ray(in.ndc);
    if d.y < 0.0 {
        return out;
    }
    let span = layer_span(d);
    let n = c.shape.z;
    let travel = -dot(g.sun_dir.xyz, d);
    let p = phase(travel);
    let stride = (span.y - span.x) / n;
    var at = g.camera_pos.xyz * KM_PER_STUD + d * span.x;
    at += d * (span.y - span.x) * (0.15 + 0.95 * NOISE);
    at.y = 0.0;
    at += c.drift.xyz;
    let advance = d / (max(d.y, 1e-4) * n);
    var t = span.x;
    var kept = 1.0;
    var light = vec3(0.0);
    var depth = 0.0;
    var hit = -1.0;
    for (var i = 0; i < i32(n); i++) {
        let f = cloud_field(at, coarse_level() + t * t * t * 2.6315789e-5);
        if f < c.shape.x {
            if hit < 0.0 {
                hit = t;
            }
            let sigma = c.shape.y * 50.0 * (c.shape.x - f);
            let shade = textureSampleLevel(shadow_map, repeat_sampler, at.xz / FIELD_TILE, 0.0);
            let below = max(0.0, 1.0 - (f32(i) + f) / n - shade.x);
            let to_sun = min(shade.z * 0.5, (shade.y + 0.35 * NOISE) * below);
            let radiance = (around(d, at.y) + sunlit(to_sun, travel, p, shade.w, sigma / n, at.y)) * c.color.rgb;
            let through = exp(-sigma / n);
            light += radiance * (1.0 - through) * kept;
            kept *= through;
            depth += sigma;
            if kept <= 1e-5 {
                kept = 0.0;
                break;
            }
        }
        t += stride;
        at += advance;
    }
    if hit < 0.0 {
        return out;
    }
    // The sun's light through the cloud's thin edges, looking towards it.
    let edge = clamp(1.0 - 0.08 * depth / n, 0.0, 1.0) * 2.0 * clamp(-travel, 0.0, 1.0) * p * 0.125;
    light += g.sun_color.rgb * edge * (1.0 - kept) * exp2(0.11 - 0.035 * depth) * c.color.rgb;
    var alpha = 1.0 - kept;
    if has_atmosphere() {
        let left = pow(max(exp2(-g.atmosphere.x * 3.5714286 * span.x * span.x), 1e-4), 0.125);
        light = mix(atmosphere_fog_toward(d, left), light, left);
        alpha *= max(left, g.atmosphere.y);
    }
    out.color = vec4(light, alpha);
    out.dist = hit;
    return out;
}

// `t` at `uv` through a 3 × 3 tent a texel of the march wide.
fn tent(t: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let px = 1.0 / c.sizes.xy;
    var sum = vec4(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let w = f32((2 - abs(x)) * (2 - abs(y)));
            sum += w * textureSampleLevel(t, clamp_sampler, uv + vec2(f32(x), f32(y)) * px, 0.0);
        }
    }
    return sum / 16.0;
}

// Clouds from 227 to 238 km away fade out.
fn far_fade(dist: f32) -> f32 {
    let f = clamp((dist - 227.45) * 0.09756097, 0.0, 1.0);
    return f * f;
}

struct ResolveOut {
    @location(0) color: vec4<f32>,
    // The first cloud's distance (km) and how many frames are blended.
    @location(1) dist: vec2<f32>,
};

// This frame's march, softened by the tent, blended with the clouds last
// frame drew where this pixel's cloud was then (up to seven frames), unless
// that cloud was at another distance, the light changed much, or there's
// no last frame to blend.
@fragment
fn fs_cloud_resolve(in: ScreenOut) -> ResolveOut {
    var out: ResolveOut;
    let d = ray(in.ndc);
    if d.y < -0.5 {
        return out;
    }
    let now = tent(march_color, uv_of(in.ndc));
    let dist = textureLoad(march_dist, vec2<i32>(in.clip.xy), 0).x;
    let fade = far_fade(dist);
    let then = c.last_view_proj * vec4(g.camera_pos.xyz + d * dist * STUDS_PER_KM, 1.0);
    let was = then.xy / max(then.w, 1e-6);
    let back = vec2(0.5 + 0.5 * was.x, 0.5 - 0.5 * was.y);
    let texel = clamp(vec2<i32>(back * c.sizes.xy), vec2(0), vec2<i32>(c.sizes.xy) - 1);
    let before = textureLoad(last_dist, texel, 0).xy;
    let old = textureSampleLevel(last_color, clamp_sampler, back, 0.0);
    let moved = select(0.0, abs(1.0 - dist / (1e-5 + before.x)), before.x > 0.0);
    let changed = abs(dot(now, vec4(1.0)) - dot(old, vec4(1.0))) > 0.153;
    if c.last.x > 0.5 || then.w <= 0.0 || moved >= 0.001 || changed {
        out.color = vec4(now.rgb, mix(now.a, 0.0, fade));
        out.dist = vec2(dist, 1.0);
        return out;
    }
    let held = before.y;
    let mixed = (old * held + now) / (held + 1.0);
    out.color = vec4(mixed.rgb, mix(0.75 * now.a + 0.25 * mixed.a, 0.0, fade));
    out.dist = vec2(dist, min(held + 1.0, 7.0));
    return out;
}

// The blended clouds over the sky, through the tent, in the frame's
// encoding with their cover squared as Studio's composite draws them; the
// thinnest left out.
@fragment
fn fs_cloud_over(in: ScreenOut) -> @location(0) vec4<f32> {
    if ray(in.ndc).y < -0.01 {
        discard;
    }
    let cloud = tent(blended, uv_of(in.ndc));
    if cloud.a <= 0.12 {
        discard;
    }
    return vec4(frame(cloud.rgb * g.fog_range.z), cloud.a * cloud.a);
}
