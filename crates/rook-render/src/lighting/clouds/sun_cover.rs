//! How much of the sun's light the clouds let through to the world, in the
//! form of Studio's `CloudsSunCoverFS` (`shaders_glsl3.pack`, read for
//! facts): the cloud the shadow map finds over the point where the light's
//! ray from the camera leaves the layer, averaged over 28 taps in two
//! rings around it, kept as `max(0, 1 − k × average²)`. Studio draws it
//! into a single texel its CPU side takes back; Rook works it out on the
//! CPU from the field the GPU reads, once a frame, and the renderer scales
//! the sun's (or moon's) light by it for everything it lights, the clouds
//! included (Inference: a one-texel target is read back, and Studio's
//! overcast darkens the ground and greys the clouds).

use glam::Vec3;

use super::{CloudNoise, Clouds, FIELD_TILE_KM, KM_PER_STUD, Volume};

/// The planet's center below the origin, and the layer's top as a radius
/// from it (km; as `shaders/sky/clouds.wgsl`).
const PLANET_DEPTH: f32 = 971.0;
const LAYER_TOP: f32 = 975.5;

/// Studio's taps, in units of its radius: the middle, ten on a ring at
/// 0.54 and seventeen on the ring at 1.
const TAPS: [[f32; 2]; 28] = [
    [0.0, 0.0],
    [0.54, 0.0],
    [0.436_869_2, 0.317_404_06],
    [0.166_869_18, 0.513_570_5],
    [-0.166_869_18, 0.513_570_5],
    [-0.436_869_2, 0.317_404_06],
    [-0.54, 0.0],
    [-0.436_869_2, -0.317_404_06],
    [-0.166_869_18, -0.513_570_5],
    [0.166_869_18, -0.513_570_5],
    [0.436_869_2, -0.317_404_06],
    [1.0, 0.0],
    [0.932_472_2, 0.361_241_67],
    [0.739_008_9, 0.673_695_6],
    [0.445_738_35, 0.895_163_3],
    [0.092_268_36, 0.995_734_2],
    [-0.273_662_98, 0.961_825_7],
    [-0.602_634_6, 0.798_017_2],
    [-0.850_217_2, 0.526_432_2],
    [-0.982_973_1, 0.183_749_51],
    [-0.982_973_1, -0.183_749_51],
    [-0.850_217_2, -0.526_432_2],
    [-0.602_634_6, -0.798_017_2],
    [-0.273_662_98, -0.961_825_7],
    [0.092_268_36, -0.995_734_2],
    [0.445_738_35, -0.895_163_3],
    [0.739_008_9, -0.673_695_6],
    [0.932_472_2, -0.361_241_67],
];

/// The rings' radius (km): two texels of Rook's 128-texel shadow map
/// over the field's 32 km (Approximation: Studio's is fed by its CPU).
const TAP_RADIUS_KM: f32 = 0.5;

/// `k`: how fast the cloud overhead takes the light away (Approximation,
/// set so the ground under Cover 0.9, Density 0.7 at ClockTime 14 darkens
/// about as a Studio 0.748 screenshot shows; Studio's CPU feeds it).
const STRENGTH: f32 = 20.0;

/// The share of the light (from `light`, a unit direction towards it) that
/// reaches a camera at `camera` (studs) under `clouds`, with the field
/// carried `drift` km by the wind: 1 with no cloud overhead.
pub(super) fn light_kept(
    noise: &CloudNoise,
    clouds: &Clouds,
    camera: Vec3,
    light: Vec3,
    (drift, density): ([f64; 3], f32),
) -> f32 {
    if light.y <= 0.0 || clouds.cover <= 0.0 {
        return 1.0;
    }
    let o = camera * KM_PER_STUD as f32 + Vec3::new(0.0, PLANET_DEPTH, 0.0);
    let b = light.dot(o);
    let leaves = -b + (b * b - (o.dot(o) - LAYER_TOP * LAYER_TOP)).max(0.0).sqrt();
    let at = camera * KM_PER_STUD as f32 + light * leaves;
    let (x, z) = (at.x + drift[0] as f32, at.z + drift[2] as f32);
    let sum: f32 = TAPS
        .iter()
        .map(|[dx, dz]| {
            column(
                &noise.field,
                (x + dx * TAP_RADIUS_KM, z + dz * TAP_RADIUS_KM),
                light,
                (clouds.cover, density),
            )
        })
        .sum();
    let average = sum / TAPS.len() as f32;
    (1.0 - STRENGTH * average * average).max(0.0)
}

/// The shadow map's cloud met per step over the point `(x, z)` (km), as
/// `fs_cloud_shadow` marches it: 13 steps up through the layer towards
/// the light (no lower than 0.3 up), against a cover 4% higher. The
/// detail and the flow are left out (Approximation).
fn column(field: &Volume, (x, z): (f32, f32), light: Vec3, (cover, density): (f32, f32)) -> f32 {
    let mut l = light;
    l.y = l.y.max(0.3);
    let l = l.normalize();
    let stride = l * ((1.0 / 12.0) / l.y);
    let start = Vec3::new(x, 0.015_625, z);
    let cover = 1.04 * cover;
    let (mut kept, mut sum, mut met) = (1.0f32, 0.0, 1.0);
    for k in 0..=12 {
        let p = start + stride * k as f32;
        let f = sample(field, p);
        if f < cover && kept > 1e-5 {
            let s = density * (cover - f);
            sum += s;
            kept *= (-0.120_224_59 * s).exp2();
            met += 1.0;
        }
    }
    sum / met
}

/// The field at `p` (km across as x and z, 0 to 1 up through the layer as
/// y), filtered as the GPU's sampler does: repeating across, clamped up
/// and down.
fn sample(field: &Volume, p: Vec3) -> f32 {
    let [w, h, d] = field.size.map(|s| s as i64);
    let level = &field.levels[0];
    let tile = FIELD_TILE_KM as f32;
    let coord = |v: f32, n: i64| v * n as f32 - 0.5;
    let (u, v, s) = (
        coord(p.x / tile, w),
        coord(p.z / tile, h),
        coord(p.y.clamp(0.0, 1.0), d),
    );
    let (u0, v0, s0) = (u.floor(), v.floor(), s.floor());
    let (fu, fv, fs) = (u - u0, v - v0, s - s0);
    let texel = |i: i64, j: i64, k: i64| {
        let (i, j, k) = (i.rem_euclid(w), j.rem_euclid(h), k.clamp(0, d - 1));
        f32::from(level[((k * h + j) * w + i) as usize]) / 255.0
    };
    let (i, j, k) = (u0 as i64, v0 as i64, s0 as i64);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |k| {
        lerp(
            lerp(texel(i, j, k), texel(i + 1, j, k), fu),
            lerp(texel(i, j + 1, k), texel(i + 1, j + 1, k), fu),
            fv,
        )
    };
    lerp(plane(k), plane(k + 1), fs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(value: u8) -> CloudNoise {
        let volume = |size: [u32; 3], channels| {
            let n = (size[0] * size[1] * size[2] * channels) as usize;
            Volume::with_levels(size, channels, vec![value; n])
        };
        CloudNoise {
            field: volume([8, 8, 4], 1),
            detail: volume([4, 4, 4], 1),
            flow: volume([4, 4, 1], 2),
        }
    }

    fn clouds(cover: f32) -> Clouds {
        Clouds {
            cover,
            density: 0.7,
            color: [1.0; 3],
        }
    }

    /// Without cloud over the camera, or with the light below the
    /// horizon, all of the light gets through; a thick field everywhere
    /// takes some of it away.
    #[test]
    fn cloud_overhead_takes_light_away() {
        let field = flat(128);
        let up = Vec3::new(0.3, 0.8, 0.2).normalize();
        let kept =
            |cover, light| light_kept(&field, &clouds(cover), Vec3::ZERO, light, ([0.0; 3], 0.7));
        assert_eq!(kept(0.4, up), 1.0, "the field is above the cover");
        assert_eq!(kept(0.7, -up), 1.0, "the light is below the horizon");
        let under = kept(0.6, up);
        assert!(under > 0.0 && under < 1.0, "{under}");
        assert!(kept(0.7, up) < under, "more cover, less light");
    }
}
