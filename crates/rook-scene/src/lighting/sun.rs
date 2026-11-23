use super::*;

/// The sun's direction now, from Lighting's `ClockTime` and
/// `GeographicLatitude`: what its light, its disc and the stars go by.
pub(super) fn sun_now(dom: &Dom, lighting: InstanceId) -> Vec3 {
    let hours = dom.clock_time(lighting) as f32;
    let latitude = dom
        .float_property(lighting, "GeographicLatitude")
        .unwrap_or(41.733);
    sun_direction(hours, latitude)
}

/// Direction to the sun for a clock time and latitude. Roblox turns the sun
/// around the Z axis once a day, rising at +X, on a circle tilted towards
/// +Z by the latitude less 23.5° (Confirmed: `Lighting:GetSunDirection()`
/// in Studio 0.742 for latitudes 0, 41.733 and -30, 2026-10-13).
pub(super) fn sun_direction(hours: f32, latitude_degrees: f32) -> Vec3 {
    let h = day_turn(hours);
    let tilt = (latitude_degrees - 23.5).to_radians();
    Vec3::new(-h.sin() * tilt.cos(), h.cos() * tilt.cos(), tilt.sin())
}

/// How far the sun has turned about the Z axis at `hours`, in radians: 0
/// at noon, once a day.
pub(super) fn day_turn(hours: f32) -> f32 {
    (hours - 12.0) / 24.0 * std::f32::consts::TAU
}

/// The sun's light in Studio's light units on a white face square to it,
/// per unit of `Brightness`, for its height `y` (the sine of its
/// elevation): 0.5 by day (measured: white and black SmoothPlastic at six
/// angles to the sun, Brightness 0.5, 1 and 3, all within 0.02), turning
/// yellow and dimming in the last few degrees above the horizon (measured
/// at five heights: (0.40, 0.40, 0) at y 0.024, (0.70, 0.70, 0.03) at
/// 0.048, (0.91, 0.92, 0.51) at 0.096, white from about 0.15; fitted,
/// Approximation). docs/behavior/lighting/sun.md.
pub(super) fn sun_color(y: f32) -> [f32; 3] {
    let warm = 1.0 - (-y.max(0.0) / 0.04).exp();
    let t = ((y - 0.045) / 0.105).clamp(0.0, 1.0);
    let blue = t * t * (3.0 - 2.0 * t);
    [0.5 * warm, 0.5 * warm, 0.5 * blue]
}

/// The moon's light per unit of `Brightness` (measured once, at 66° up:
/// 0.031 units on white; Approximation elsewhere, faded in as the sun's).
const MOON_LIGHT: f32 = 0.031;

/// The moon for the sun at `sun`: opposite the sun's daily circle
/// (Confirmed: `Lighting:GetMoonDirection()` in Studio 0.743 at midnight,
/// latitude 0, is the noon sun's direction).
pub(super) fn moon_direction(sun: Vec3) -> Vec3 {
    Vec3::new(-sun.x, -sun.y, sun.z)
}

/// How much of the moon's light, or of the sun's or moon's disc, shows at
/// its height `y` (the sine of its elevation), 0 to 1: faded in over the
/// last few degrees above the horizon, as the sun's light is
/// (Approximation).
pub(super) fn horizon_fade(y: f32) -> f32 {
    (1.0 - (-y / 0.04).exp()).max(0.0)
}

/// The direction light comes from and its color in light units: the sun
/// while it's up, else the moon ([`moon_direction`]).
pub(super) fn sun_and_moon(sun: Vec3, brightness: f32) -> (Vec3, [f32; 3]) {
    if sun.y > 0.0 {
        return (sun, sun_color(sun.y).map(|c| c * brightness));
    }
    let moon = moon_direction(sun);
    (moon, [MOON_LIGHT * horizon_fade(moon.y) * brightness; 3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_sun_is_high_and_midnight_below() {
        assert!(sun_direction(12.0, 41.733).y > 0.7);
        assert!(sun_direction(0.0, 41.733).y < 0.0);
        assert!(sun_direction(6.0, 0.0).y.abs() < 1e-4);
    }

    /// `Lighting:GetSunDirection()` in Studio.
    #[test]
    fn sun_matches_studio() {
        for (hours, latitude, studio) in [
            (14.0, 41.733, Vec3::new(-0.4749, 0.8225, 0.3129)),
            (9.0, 0.0, Vec3::new(0.6485, 0.6485, -0.3987)),
            (14.0, -30.0, Vec3::new(-0.2974, 0.5151, -0.8038)),
        ] {
            let d = sun_direction(hours, latitude);
            assert!(d.distance(studio) < 1e-3, "{hours} {latitude}: {d}");
        }
    }
}
