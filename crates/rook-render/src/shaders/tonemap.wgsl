// The last pass: the frame (Studio's encoding of light units, `tone.wgsl`)
// through the place's tone curve and the ColorCorrections' matrix, onto
// the screen, with Highlights and then the GUI over it
// (docs/behavior/lighting/post-processing.md). The frame, the mask and the
// post effects' textures are the 3D view's size and land on its rectangle
// of the screen (`tone.view`); the GUI layer is the screen's.

import package::post::focus::{distance_of, far_circle};
import package::tone::{FRAME_LIGHT, linear_to_srgb, srgb_to_linear, studio_light3};

struct Tone {
    // x: the curve (0 Default, 1 Retro); y: 1 when the mask holds
    // Highlights; z: 1 when the GUI layer holds GUI; w: 1 when `color`
    // applies.
    curve: vec4<f32>,
    // The ColorCorrections' matrix: each row times the sRGB pixel, plus
    // its w.
    color: array<vec4<f32>, 3>,
    // x: how much of the blurred frame replaces the frame; y: 1 to add
    // the bloom texture's light; z: 1 to add the sun rays'; w: 1 to mix
    // in the depth of field's fields.
    effects: vec4<f32>,
    // The Blooms added in place (`Size` 0): thresholds in light units,
    // then intensities.
    in_place: array<vec4<f32>, 2>,
    // The depth of field's focus: x its distance, y its radius, in studs.
    focus: vec4<f32>,
    // Where the frame goes on the screen: xy its top left, zw its size,
    // in pixels.
    view: vec4<f32>,
    // Each Highlight's fill and outline (sRGB, and their opacity), by its
    // number - 1 (`renderer/passes/highlights.rs`).
    looks: array<vec4<f32>, 62>,
};

@group(0) @binding(0) var frame_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> tone: Tone;
// Each pixel's Highlight number, 0 for none.
@group(0) @binding(2) var mask: texture_2d<u32>;
// The GUI, premultiplied sRGB (`gui.wgsl`).
@group(0) @binding(3) var gui_layer: texture_2d<f32>;
// The post effects' textures (`renderer/passes/post.rs`), black when off.
@group(1) @binding(0) var post_sampler: sampler;
// The blurred frame, in the frame's encoding, maybe smaller than it.
@group(1) @binding(1) var blurred: texture_2d<f32>;
// The chained Blooms' light, in light units, half the frame's size.
@group(1) @binding(2) var bloom: texture_2d<f32>;
// The sun rays' light, in light units, half the frame's size.
@group(1) @binding(3) var rays: texture_2d<f32>;
// The depth of field's far field (times its circle, in alpha) and near
// field (how much it covers in alpha), in light units, half the frame's
// size.
@group(1) @binding(4) var far_field: texture_2d<f32>;
@group(1) @binding(5) var near_field: texture_2d<f32>;
// The main pass's depth, while the depth of field reads it.
@group(1) @binding(6) var depth: texture_depth_2d;

@vertex
fn vs_tonemap(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let xy = vec2(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    return vec4(xy, 0.0, 1.0);
}

@fragment
fn fs_tonemap(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let in_view = at.xy - tone.view.xy;
    var out = vec3(0.0);
    if all(in_view >= vec2(0.0)) && all(in_view < tone.view.zw) {
        out = view_pixel(in_view);
    }
    if tone.curve.z == 1.0 {
        // Over the screen's sRGB pixels, as Roblox blends GUI.
        let g = textureLoad(gui_layer, vec2<i32>(at.xy), 0);
        out = srgb_to_linear(g.rgb + linear_to_srgb(out) * (1.0 - g.a));
    }
    return vec4(out, 1.0);
}

// The 3D view's finished pixel at `at`, in pixels from the view's top
// left.
fn view_pixel(at: vec2<f32>) -> vec3<f32> {
    let p = vec2<i32>(at);
    let v = textureLoad(frame_texture, p, 0).rgb;
    var units = v * v * FRAME_LIGHT;
    let uv = at / vec2<f32>(textureDimensions(frame_texture));
    // The depth of field's fields replace the frame where they blur it,
    // before anything adds to it (Studio resolves them into the frame).
    if tone.effects.w > 0.0 {
        // How blurred this pixel's own surface is, at full resolution, so
        // sharp things keep their edges against a blurred background.
        let far = textureSampleLevel(far_field, post_sampler, uv, 0.0);
        let z = distance_of(textureLoad(depth, p, 0));
        if far.a > 1e-4 {
            units = mix(units, far.rgb / far.a, far_circle(z, tone.focus.xy));
        }
        let near = textureSampleLevel(near_field, post_sampler, uv, 0.0);
        units = mix(units, near.rgb, near.a);
    }
    // Bloom's light adds before the curve (Studio's
    // `ImageProcessBloomFS`), and a Bloom without bleed adds each texel's
    // own light over its threshold, as its chain's first copy keeps it.
    if any(tone.in_place[1] != vec4(0.0)) {
        let own = units;
        let brightest = max(own.r, max(own.g, own.b));
        for (var i = 0; i < 4; i++) {
            let over = max(brightest - tone.in_place[0][i], 0.0) / (brightest + 0.001);
            units += own * over * tone.in_place[1][i];
        }
    }
    if tone.effects.y > 0.0 {
        units += textureSampleLevel(bloom, post_sampler, uv, 0.0).rgb;
    }
    // The sun rays' too (Studio's `ImageProcessFXCompositionFS`).
    if tone.effects.z > 0.0 {
        units += textureSampleLevel(rays, post_sampler, uv, 0.0).rgb;
    }
    // The blurred frame is mixed in before the curve, as Studio's last
    // pass mixes it.
    if tone.effects.x > 0.0 {
        let b = textureSampleLevel(blurred, post_sampler, uv, 0.0).rgb;
        units = mix(units, b * b * FRAME_LIGHT, tone.effects.x);
    }
    var out: vec3<f32>;
    // Retro shows the light's square root as the pixel, with no curve
    // (measured: black Decals over white Neon draw 510 x Transparency).
    if tone.curve.x == 1.0 {
        out = srgb_to_linear(min(sqrt(units), vec3(1.0)));
    } else {
        out = studio_light3(units);
    }
    // After the curve, on the pixel's values, as Studio's last pass
    // multiplies its output (`shaders_glsl3.pack`); PostEffects leave
    // Highlights and the GUI alone.
    if tone.curve.w == 1.0 {
        let c = vec4(linear_to_srgb(out), 1.0);
        let graded = vec3(dot(tone.color[0], c), dot(tone.color[1], c), dot(tone.color[2], c));
        out = srgb_to_linear(clamp(graded, vec3(0.0), vec3(1.0)));
    }
    if tone.curve.y == 1.0 {
        out = highlighted(p, out);
    }
    return out;
}

fn mask_at(p: vec2<i32>) -> u32 {
    let last = vec2<i32>(textureDimensions(mask)) - 1;
    return textureLoad(mask, clamp(p, vec2(0), last), 0).r;
}

// The finished pixel `linear` at `p` with Highlights over it (measured in
// Studio 0.742, docs/behavior/rendering/highlight.md): the fill over what its parts
// cover, and a 2-pixel outline centered on the edge of what they cover,
// both mixed in over the screen's sRGB pixels.
fn highlighted(p: vec2<i32>, linear: vec3<f32>) -> vec3<f32> {
    let own = mask_at(p);
    let around = array<u32, 4>(
        mask_at(p + vec2(1, 0)),
        mask_at(p - vec2(1, 0)),
        mask_at(p + vec2(0, 1)),
        mask_at(p - vec2(0, 1)),
    );
    // Inside, the edge is where a neighbor isn't ours; outside, it's
    // the first Highlight next to us.
    var edge = 0u;
    for (var i = 0; i < 4; i++) {
        let n = around[i];
        if own != 0u && n != own {
            edge = own;
        }
        if own == 0u && edge == 0u && n != 0u {
            edge = n;
        }
    }
    var c = linear_to_srgb(linear);
    if own != 0u {
        let fill = tone.looks[(own - 1u) * 2u];
        c = mix(c, fill.rgb, fill.a);
    }
    if edge != 0u {
        let outline = tone.looks[(edge - 1u) * 2u + 1u];
        c = mix(c, outline.rgb, outline.a);
    }
    return srgb_to_linear(c);
}
