//! The ranges the audio API's properties keep to: a value set outside one
//! reads back at its end (all observed in Studio 0.742 and 0.748; setting
//! -1e9 and 1e9 on each). Unlike the legacy SoundEffects, which read back
//! anything. Both ignore NaN: the value before stays (Confirmed in Studio
//! 0.748, conformance case `content/sound_effects`).

use rbx_types::NumberRange;
use rook_dom::{DomError, InstanceId};
use rook_types::Value;

use crate::runtime::world::World;

/// (class, property, lowest, highest).
const RANGES: &[(&str, &str, f64, f64)] = &[
    ("AudioPlayer", "Volume", 0.0, 10.0),
    ("AudioPlayer", "PlaybackSpeed", 0.0, 20.0),
    ("AudioFader", "Volume", 0.0, 3.0),
    ("AudioEqualizer", "LowGain", -80.0, 10.0),
    ("AudioEqualizer", "MidGain", -80.0, 10.0),
    ("AudioEqualizer", "HighGain", -80.0, 10.0),
    ("AudioDistortion", "Level", 0.0, 1.0),
    ("AudioPitchShifter", "Pitch", 0.5, 2.0),
    ("AudioReverb", "DecayRatio", 0.1, 1.0),
    ("AudioReverb", "DecayTime", 0.1, 20.0),
    ("AudioReverb", "Density", 0.1, 1.0),
    ("AudioReverb", "Diffusion", 0.1, 1.0),
    ("AudioReverb", "DryLevel", -80.0, 20.0),
    ("AudioReverb", "EarlyDelayTime", 0.0, 0.3),
    ("AudioReverb", "HighCutFrequency", 20.0, 20_000.0),
    ("AudioReverb", "LateDelayTime", 0.0, 0.1),
    ("AudioReverb", "LowShelfFrequency", 20.0, 20_000.0),
    ("AudioReverb", "LowShelfGain", -36.0, 12.0),
    ("AudioReverb", "ReferenceFrequency", 20.0, 20_000.0),
    ("AudioReverb", "WetLevel", -80.0, 20.0),
    ("AudioEcho", "DelayTime", 0.001, 5.0),
    ("AudioEcho", "DryLevel", -80.0, 10.0),
    ("AudioEcho", "Feedback", 0.0, 1.0),
    ("AudioEcho", "RampTime", 0.0, 60_000.0),
    ("AudioEcho", "WetLevel", -80.0, 10.0),
    ("AudioChorus", "Depth", 0.0, 1.0),
    ("AudioChorus", "Mix", 0.0, 1.0),
    ("AudioChorus", "Rate", 0.0, 20.0),
    ("AudioFlanger", "Depth", 0.01, 1.0),
    ("AudioFlanger", "Mix", 0.0, 1.0),
    ("AudioFlanger", "Rate", 0.0, 20.0),
    ("AudioCompressor", "Attack", 0.0001, 0.5),
    ("AudioCompressor", "MakeupGain", -30.0, 30.0),
    ("AudioCompressor", "Ratio", 1.0, 50.0),
    ("AudioCompressor", "Release", 0.01, 5.0),
    ("AudioCompressor", "Threshold", -60.0, 0.0),
    ("AudioFilter", "Frequency", 20.0, 22_000.0),
    ("AudioFilter", "Gain", -30.0, 30.0),
    ("AudioFilter", "Q", 0.1, 10.0),
    ("AudioTremolo", "Depth", 0.0, 1.0),
    ("AudioTremolo", "Duty", 0.0, 1.0),
    ("AudioTremolo", "Frequency", 0.0, 20.0),
    ("AudioTremolo", "Shape", 0.0, 1.0),
    ("AudioTremolo", "Skew", -1.0, 1.0),
    ("AudioTremolo", "Square", 0.0, 1.0),
    ("AudioLimiter", "MaxLevel", -12.0, 0.0),
    ("AudioLimiter", "Release", 0.001, 1.0),
    ("AudioGate", "Attack", 0.001, 5.0),
    ("AudioGate", "Release", 0.001, 5.0),
];

/// AudioEqualizer's `MidRange` ends each keep within 200 to 20,000 Hz
/// (10 to 30,000 read back as 200 to 20,000).
const MID_RANGE: (f32, f32) = (200.0, 20_000.0);

/// AudioGate's `Threshold` ends each keep within -80 to 30 dB.
const GATE_THRESHOLD: (f32, f32) = (-80.0, 30.0);

/// The range `class`'s `property` keeps to, if it has one: what scripts
/// read back, and what the mixer plays a value saved outside it at.
fn range(class: &str, property: &str) -> Option<(f64, f64)> {
    RANGES
        .iter()
        .find(|(c, p, _, _)| *c == class && *p == property)
        .map(|&(_, _, lo, hi)| (lo, hi))
}

/// `v` kept to `class`'s `property`'s range, if it has one. NaN stays
/// NaN: scripts can't set it, and a saved one plays at the property's
/// default (the mixer's choice).
pub fn keep(class: &str, property: &str, v: f64) -> f64 {
    match range(class, property) {
        Some((lo, hi)) => v.clamp(lo, hi),
        None => v,
    }
}

/// Whether NaN set on `id`'s `name` is ignored: the audio API's ranged
/// properties and every legacy SoundEffect's numbers (Confirmed in
/// Studio 0.748 for all of them; not every class does: `Sky`'s
/// `SunAngularSize` keeps NaN).
fn ignores_nan(w: &World, id: InstanceId, class: &str, name: &str) -> bool {
    range(class, name).is_some() || w.dom.is_a(id, "SoundEffect")
}

/// Store `value` within its range, if `name` has one on `id`'s class, or
/// ignore it if it's a NaN the property doesn't take.
#[expect(
    clippy::disallowed_methods,
    reason = "stores the value in the variant the property already holds"
)]
pub(crate) fn clamp(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let class = w.dom.class_name(id);
    if value.as_number().is_some_and(f64::is_nan) && ignores_nan(w, id, class, name) {
        return Some(Ok(()));
    }
    let ends = match (class, name) {
        ("AudioEqualizer", "MidRange") => Some(MID_RANGE),
        ("AudioGate", "Threshold") => Some(GATE_THRESHOLD),
        _ => None,
    };
    if let (Some((lo, hi)), Value::NumberRange(r)) = (ends, value) {
        let range = NumberRange::new(r.min.clamp(lo, hi), r.max.clamp(lo, hi));
        return Some(
            w.dom
                .set_property_by_name(id, name, Value::NumberRange(range)),
        );
    }
    range(class, name)?;
    let v = keep(class, name, value.as_number()?);
    let stored = match w.dom.property_by_name(id, name) {
        Some(Value::Float64(_)) => Value::Float64(v),
        _ => Value::Float32(v as f32),
    };
    Some(w.dom.set_property_by_name(id, name, stored))
}
