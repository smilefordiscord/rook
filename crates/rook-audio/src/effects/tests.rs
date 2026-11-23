use super::*;

const EQ: Effect = Effect::Equalizer {
    low: 0.0,
    mid: 0.0,
    high: 0.0,
    range: CROSSOVERS,
};
const DIST: Effect = Effect::Distortion { level: 0.5 };
const PITCH: Effect = Effect::PitchShift { octave: 2.0 };

/// The orders measured in Studio: at Priority 0 Equalizer before
/// Distortion before PitchShift; an explicit priority above or below
/// the base moves an effect; ties go to the later child.
#[test]
fn orders_by_priority() {
    assert_eq!(ordered([(PITCH, 0), (DIST, 0), (EQ, 0)]), [EQ, DIST, PITCH]);
    assert_eq!(ordered([(DIST, 9), (EQ, 0)]), [DIST, EQ]);
    assert_eq!(ordered([(DIST, 8), (EQ, 0)]), [EQ, DIST]);
    assert_eq!(ordered([(DIST, 0), (EQ, 3)]), [DIST, EQ]);
    assert_eq!(ordered([(DIST, 0), (EQ, 4)]), [EQ, DIST]);
    assert_eq!(ordered([(PITCH, 0), (DIST, 2)]), [PITCH, DIST]);
    assert_eq!(ordered([(DIST, -5), (EQ, -10)]), [DIST, EQ]);
}

#[test]
fn keeps_state_across_settings() {
    let mut chain = Chain::new(crate::SAMPLE_RATE);
    chain.sync(&[EQ]);
    let mut frames = vec![[0.5, 0.5]; 64];
    chain.process(&mut frames);
    chain.sync(&[Effect::Equalizer {
        low: -6.0,
        mid: 0.0,
        high: 0.0,
        range: CROSSOVERS,
    }]);
    assert!(matches!(chain.stages[0], Stage::Equalizer(_)));
    chain.sync(&[DIST]);
    assert!(matches!(chain.stages[0], Stage::Distortion(_)));
}

/// A script's new setting reaches a running effect without restarting
/// it: a tremolo half a cycle in carries on from its trough.
#[test]
fn new_effects_take_settings_in_place() {
    let mut chain = Chain::new(crate::SAMPLE_RATE);
    chain.sync(&[Effect::Tremolo(Tremolo::default())]);
    let mut frames = vec![[1.0, 1.0]; crate::SAMPLE_RATE as usize / 10];
    chain.process(&mut frames);
    chain.sync(&[Effect::Tremolo(Tremolo {
        depth: 0.5,
        ..Tremolo::default()
    })]);
    let mut next = [[1.0, 1.0]];
    chain.process(&mut next);
    assert!((next[0][0] - 0.5).abs() < 0.01, "{:?}", next[0]);
    let flange = |delay, mix, rate| Effect::Flange { delay, mix, rate };
    let mut stage = Stage::new(&flange(0.01, 0.5, 1.0), crate::SAMPLE_RATE as f32);
    assert!(
        stage.set(&flange(0.02, 0.8, 2.0)),
        "a flange is taken in place"
    );
    for (a, b) in [
        (
            Effect::Filter(Filter::default()),
            Effect::Filter(Filter {
                kind: FilterType::Lowpass48dB,
                ..Filter::default()
            }),
        ),
        (
            Effect::Limiter {
                max_level: 0.0,
                release: 0.01,
            },
            Effect::Limiter {
                max_level: -6.0,
                release: 0.1,
            },
        ),
        (
            Effect::Gate {
                threshold: [-36.0, -24.0],
                attack: 0.01,
                release: 0.1,
            },
            Effect::Gate {
                threshold: [-50.0, -40.0],
                attack: 0.1,
                release: 1.0,
            },
        ),
    ] {
        let mut stage = Stage::new(&a, crate::SAMPLE_RATE as f32);
        assert!(stage.set(&b), "{b:?} is taken in place");
    }
}
