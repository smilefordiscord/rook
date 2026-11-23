//! Rook's own cloud textures, for when the Roblox install's aren't there.
//! The field's values are spread as the install's `sky/clouds.dds` spreads
//! its own, so a `Cover` covers as much of the sky with either, and the
//! detail's as `sky/cloudDetail3D.dds`'s; their shapes are Rook's (tiling
//! cells and value noise, the usual makings of cloud noise since Schneider
//! 2015). Made once per process, the first time clouds are drawn.

use std::sync::LazyLock;

use super::{CloudNoise, Volume};

/// The field's texels across a tile, and through the layer (the install's
/// is 256 × 256 × 32).
const FIELD_SIDE: u32 = 128;
const FIELD_DEPTH: u32 = 32;
/// The detail noise's texels along each side (the install's is 64).
const DETAIL_SIDE: u32 = 32;
/// The flow's texels along each side (the install's is 256).
const FLOW_SIDE: u32 = 64;

/// How the install's field spreads its values (`sky/clouds.dds` of
/// `version-8d7a1009c17e4ff8`, measured 2026-11-23): the value at each
/// share of its texels, from the least.
const FIELD_SPREAD: [(f32, f32); 9] = [
    (0.0, 0.0),
    (0.01, 0.286),
    (0.1, 0.435),
    (0.25, 0.518),
    (0.5, 0.573),
    (0.75, 0.655),
    (0.9, 0.733),
    (0.99, 0.847),
    (1.0, 1.0),
];

/// The same for its detail noise (`sky/cloudDetail3D.dds`), which reaches
/// only half way: the shaders take `2v − 1`, so detail only ever adds
/// cloud.
const DETAIL_SPREAD: [(f32, f32); 9] = [
    (0.0, 0.0),
    (0.01, 0.094),
    (0.1, 0.169),
    (0.25, 0.216),
    (0.5, 0.271),
    (0.75, 0.322),
    (0.9, 0.396),
    (0.99, 0.475),
    (1.0, 0.498),
];

/// Rook's own cloud textures, made the first time they're asked for.
pub(super) fn rooks_own() -> &'static CloudNoise {
    static OWN: LazyLock<CloudNoise> = LazyLock::new(|| {
        profiling::scope!("cloud noise");
        CloudNoise {
            field: field(),
            detail: detail(),
            flow: flow(),
        }
    });
    &OWN
}

/// The field: low where there's cloud. Big round cells of cloud with
/// smaller puffs on them and finer noise, thinning towards the layer's top and bottom, then each
/// texel given the value the install's field has at its rank.
fn field() -> Volume {
    let (side, depth) = (FIELD_SIDE, FIELD_DEPTH);
    let mut cloud = Vec::with_capacity((side * side * depth) as usize);
    for layer in 0..depth {
        let h = (layer as f32 + 0.5) / depth as f32;
        // Flat bottoms, rounded tops.
        let profile = 0.15 * (1.0 - smoothstep(0.0, 0.12, h)) + 0.35 * smoothstep(0.55, 1.0, h);
        for row in 0..side {
            for col in 0..side {
                let (x, z) = (col as f32 / side as f32, row as f32 / side as f32);
                let big = cells([x * 6.0, h * 1.5, z * 6.0], [6, 1 << 20, 6], 1);
                let puffs = cells([x * 16.0, h * 4.0, z * 16.0], [16, 1 << 20, 16], 6);
                let fine = fbm([x * 12.0, h * 3.0, z * 12.0], [12, 1 << 20, 12], 4, 2);
                cloud.push(0.6 * big + 0.25 * puffs + 0.15 * fine - profile);
            }
        }
    }
    // The most cloud gets the least value.
    let spread = ranked(&cloud, &FIELD_SPREAD, true);
    Volume::with_levels([side, side, depth], 1, spread)
}

/// The detail noise: value noise repeating on every side.
fn detail() -> Volume {
    let side = DETAIL_SIDE;
    let mut noise = Vec::with_capacity((side * side * side) as usize);
    for layer in 0..side {
        for row in 0..side {
            for col in 0..side {
                let p = [col, layer, row].map(|v| v as f32 / side as f32 * 4.0);
                noise.push(fbm(p, [4; 3], 3, 3));
            }
        }
    }
    Volume::with_levels([side; 3], 1, ranked(&noise, &DETAIL_SPREAD, false))
}

/// The flow: two channels of smooth noise around the middle, X then Z
/// (the shaders take `2v − 1`).
fn flow() -> Volume {
    let side = FLOW_SIDE;
    let mut texels = Vec::with_capacity((side * side * 2) as usize);
    for row in 0..side {
        for col in 0..side {
            let p = [
                col as f32 / side as f32 * 4.0,
                0.0,
                row as f32 / side as f32 * 4.0,
            ];
            for seed in [4, 5] {
                let v = 0.5 + 0.8 * (fbm(p, [4, 1 << 20, 4], 2, seed) - 0.5);
                texels.push(byte(v));
            }
        }
    }
    Volume::with_levels([side, side, 1], 2, texels)
}

/// Each of `values` as the value `spread` gives its rank (its share of the
/// values below it, from the greatest down with `descending`), as bytes.
fn ranked(values: &[f32], spread: &[(f32, f32)], descending: bool) -> Vec<u8> {
    let mut order: Vec<u32> = (0..values.len() as u32).collect();
    order.sort_unstable_by(|&a, &b| values[a as usize].total_cmp(&values[b as usize]));
    if descending {
        order.reverse();
    }
    let mut out = vec![0; values.len()];
    let last = (values.len() - 1).max(1) as f32;
    for (rank, &i) in order.iter().enumerate() {
        out[i as usize] = byte(through(rank as f32 / last, spread));
    }
    out
}

/// Piecewise linear through `points` (by x, rising).
fn through(x: f32, points: &[(f32, f32)]) -> f32 {
    let i = points
        .partition_point(|p| p.0 <= x)
        .clamp(1, points.len() - 1);
    let ((x0, y0), (x1, y1)) = (points[i - 1], points[i]);
    y0 + (y1 - y0) * ((x - x0) / (x1 - x0).max(1e-6)).clamp(0.0, 1.0)
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A hash of a lattice point (from Wang's and Jenkins's integer mixes).
fn hash(p: [i32; 3], seed: u32) -> u32 {
    let mut h = (p[0] as u32).wrapping_mul(0x8da6_b343)
        ^ (p[1] as u32).wrapping_mul(0xd816_3841)
        ^ (p[2] as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ (h >> 15)
}

/// 0 to 1 from a hash.
fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// The lattice point `p` wrapped into `period` cells.
fn wrap(p: [i32; 3], period: [i32; 3]) -> [i32; 3] {
    [0, 1, 2].map(|a| p[a].rem_euclid(period[a]))
}

/// Value noise at `p` (in cells), 0 to 1, repeating every `period` cells.
fn value(p: [f32; 3], period: [i32; 3], seed: u32) -> f32 {
    let cell = p.map(|v| v.floor() as i32);
    let f = [0, 1, 2].map(|a| {
        let t = p[a] - cell[a] as f32;
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    });
    let corner = |dx, dy, dz| {
        let at = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
        unit(hash(wrap(at, period), seed))
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), f[0]);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), f[0]);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), f[0]);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), f[0]);
    lerp(lerp(x00, x10, f[1]), lerp(x01, x11, f[1]), f[2])
}

/// `octaves` of value noise, each twice as fine and half as strong, 0 to 1.
fn fbm(p: [f32; 3], period: [i32; 3], octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut weight, mut total) = (0.0, 1.0, 0.0);
    for o in 0..octaves {
        let scale = (1 << o) as f32;
        let period = period.map(|v| v.saturating_mul(1 << o));
        sum += weight * value(p.map(|v| v * scale), period, seed + o);
        total += weight;
        weight *= 0.5;
    }
    sum / total
}

/// Cellular noise (Worley 1996): 1 at each cell's point, falling to 0 a
/// cell away, repeating every `period` cells.
fn cells(p: [f32; 3], period: [i32; 3], seed: u32) -> f32 {
    let cell = p.map(|v| v.floor() as i32);
    let mut nearest = f32::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let at = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
                let h = hash(wrap(at, period), seed);
                let point = [
                    h,
                    hash([h as i32, 1, 0], seed),
                    hash([h as i32, 2, 0], seed),
                ];
                let d2: f32 = (0..3)
                    .map(|a| {
                        let d = at[a] as f32 + unit(point[a]) - p[a];
                        d * d
                    })
                    .sum();
                nearest = nearest.min(d2);
            }
        }
    }
    1.0 - nearest.sqrt().min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rook's field spreads its values as the install's does, so a Cover
    /// covers as much of the sky with either.
    #[test]
    fn the_field_spreads_as_the_installs() {
        let field = &rooks_own().field;
        let mut values = field.levels[0].clone();
        values.sort_unstable();
        for (share, expected) in FIELD_SPREAD {
            let at = ((values.len() - 1) as f32 * share) as usize;
            let v = f32::from(values[at]) / 255.0;
            assert!(
                (v - expected).abs() < 0.01,
                "{share}: {v} against {expected}"
            );
        }
    }

    /// The textures repeat: value noise and cells match across a period.
    #[test]
    fn noise_repeats_over_its_period() {
        for p in [[0.3, 0.7, 1.2], [3.9, 0.1, 2.5]] {
            let moved = [p[0] + 6.0, p[1], p[2] + 6.0];
            let period = [6, 1 << 20, 6];
            assert!((value(p, period, 1) - value(moved, period, 1)).abs() < 1e-5);
            assert!((cells(p, period, 1) - cells(moved, period, 1)).abs() < 1e-5);
        }
    }
}
