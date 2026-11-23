//! What a sound is heard through: its own effects, its SoundGroup and the
//! groups that one is in, and `SoundService.AmbientReverb`
//! (`docs/behavior/audio/soundgroups-and-effects.md`).

use rook_audio::effects::{Chain, Effect, FLANGE_SPAN, Reverb, Tremolo, ordered};

use super::*;

/// The effect a SoundEffect instance is, if Rook plays its class. Defaults
/// are Studio's.
fn effect(dom: &Dom, id: InstanceId) -> Option<Effect> {
    // A saved file can hold NaN (a script's is ignored, Confirmed), which
    // would stick in an effect's state and silence the mix for good.
    // Inference: NaN plays as the default.
    let f = |name: &str, default: f32| {
        (dom.float_property(id, name))
            .filter(|v| !v.is_nan())
            .unwrap_or(default)
    };
    Some(match dom.class_name(id) {
        "EqualizerSoundEffect" => Effect::Equalizer {
            low: f("LowGain", -20.0),
            mid: f("MidGain", -10.0),
            high: f("HighGain", 0.0),
            range: rook_audio::effects::CROSSOVERS,
        },
        "ReverbSoundEffect" => Effect::Reverb(Reverb {
            decay: f("DecayTime", 1.5),
            density: f("Density", 1.0),
            diffusion: f("Diffusion", 1.0),
            // Within ReverbSoundEffect's -80 to 10 dB (creator-docs),
            // narrower than AudioReverb's.
            dry: f("DryLevel", -6.0).clamp(-80.0, 10.0),
            wet: f("WetLevel", 0.0).clamp(-80.0, 10.0),
            ..Reverb::default()
        }),
        "DistortionSoundEffect" => Effect::Distortion {
            level: f("Level", 0.75),
        },
        "PitchShiftSoundEffect" => Effect::PitchShift {
            octave: f("Octave", 1.25),
        },
        // The legacy effects keep to the ranges creator-docs gives each
        // property, though they read back whatever was set.
        "ChorusSoundEffect" => Effect::Chorus {
            depth: f("Depth", 0.15),
            mix: f("Mix", 0.5),
            rate: f("Rate", 0.5),
        },
        "EchoSoundEffect" => Effect::Echo {
            // From 0.01 s, where AudioEcho's DelayTime goes down to 0.001.
            delay: f("Delay", 1.0).clamp(0.01, 5.0),
            dry: f("DryLevel", 0.0),
            wet: f("WetLevel", 0.0),
            feedback: f("Feedback", 0.5),
        },
        // TODO: duck by `SideChain`'s Sound or SoundGroup; the mixer has to
        // hand that sound's level to the compressor as its key.
        "CompressorSoundEffect" => Effect::Compressor {
            threshold: f("Threshold", -40.0).clamp(-80.0, 0.0),
            // Creator-docs gives no range; AudioCompressor's (Inference).
            ratio: f("Ratio", 40.0).clamp(1.0, 50.0),
            attack: f("Attack", 0.1).clamp(0.1, 1.0),
            release: f("Release", 0.1).clamp(0.0, 5.0),
            makeup: f("GainMakeup", 0.0).clamp(-30.0, 30.0),
        },
        "FlangeSoundEffect" => Effect::Flange {
            delay: f("Depth", 0.45).clamp(0.01, 1.0) * FLANGE_SPAN,
            mix: f("Mix", 0.85),
            rate: f("Rate", 5.0),
        },
        "TremoloSoundEffect" => Effect::Tremolo(Tremolo {
            depth: f("Depth", 1.0),
            duty: f("Duty", 0.5),
            frequency: f("Frequency", 5.0).clamp(0.1, 20.0),
            ..Tremolo::default()
        }),
        _ => return None,
    })
}

/// The enabled effects directly under a Sound or SoundGroup, in the order
/// they run (Confirmed: an effect inside a Folder under the sound does
/// nothing; one added while it plays applies at once).
pub(super) fn effects(dom: &Dom, id: InstanceId) -> Vec<Effect> {
    let children = dom.children(id);
    if children.is_empty() {
        return Vec::new();
    }
    ordered(children.iter().filter_map(|&c| {
        if !dom.is_a(c, "SoundEffect") || dom.bool_property(c, "Enabled") == Some(false) {
            return None;
        }
        let priority = dom.number_property(c, "Priority").unwrap_or(0.0) as i32;
        Some((effect(dom, c)?, priority))
    }))
}

/// The SoundGroup a Sound plays through: its `SoundGroup` property, when
/// that is a SoundGroup still in the tree. Parenting a Sound to a group
/// doesn't put it in the group (creator-docs).
pub(super) fn group(dom: &Dom, sound: InstanceId) -> Option<InstanceId> {
    let g = dom.ref_property(sound, "SoundGroup")?;
    (dom.get(g).is_some() && dom.is_a(g, "SoundGroup")).then_some(g)
}

/// A group's parent group: groups nest by parenting (creator-docs "mix
/// tree"; that the volumes multiply down the tree is Inference).
pub(super) fn parent_group(dom: &Dom, group: InstanceId) -> Option<InstanceId> {
    dom.parent(group).filter(|&p| dom.is_a(p, "SoundGroup"))
}

/// Studio's `PlaybackLoudness` with effects: the level after the sound's
/// own effects, before `Volume` and its groups (Confirmed: an equalizer at
/// -80 dB read 0, a full distortion 971, a group at Volume 0 or with
/// effects changed nothing). `chain` keeps the effects' state from frame
/// to frame.
pub(super) fn loudness_through(reader: &Reader, from: f64, to: f64, chain: &mut Chain) -> f64 {
    let Some((a, b)) = frames_between(reader.audio(), from, to) else {
        return 0.0;
    };
    let mut block: Vec<rook_audio::effects::Frame> = (a..b)
        .map(|i| reader.frame(i).map(|s| f32::from(s) / 32768.0))
        .collect();
    chain.process(&mut block);
    let sum: f64 = block
        .iter()
        .map(|f| f64::from(f[0]).powi(2) + f64::from(f[1]).powi(2))
        .sum();
    (sum / (2 * block.len()) as f64).sqrt() * 1000.0
}

/// The frames from `from` to `to` seconds into `audio`, if any.
pub(super) fn frames_between(audio: &Audio, from: f64, to: f64) -> Option<(usize, usize)> {
    let rate = f64::from(audio.sample_rate);
    let frames = audio.frames();
    let a = ((from.max(0.0) * rate) as usize).min(frames);
    let b = ((to.max(0.0) * rate) as usize).clamp(a, frames);
    (b > a).then_some((a, b))
}

impl DataModel {
    /// The SoundGroups `sounds` play through, and every group those are
    /// in, for the mixer.
    pub fn sound_groups(&self, sounds: &[PlayingSound]) -> Vec<rook_audio::Group<InstanceId>> {
        let w = self.runtime.world();
        super::hearing::groups(&w.dom, sounds.iter().map(|s| s.group))
    }
}

/// `SoundService.AmbientReverb`'s reverb, added to every sound.
pub(super) fn ambient_reverb(dom: &Dom) -> Option<Reverb> {
    let service = dom.find_service("SoundService")?;
    rook_audio::effects::ambient(dom.enum_property(service, "AmbientReverb")?)
}
