//! The audio API on a client: its default listener, scheduled `Play`,
//! analyzers measuring what reaches them, and listeners hearing emitters
//! by distance.

use super::sounds::tone_options;
use crate::tests::*;

#[test]
fn client_graph_listeners_and_analyzers() {
    let options = tone_options("audio-api");
    let mut server = DataModel::new(&options).unwrap();
    {
        let mut w = server.runtime.world_mut();
        let ss = w.dom.find_service("SoundService").unwrap();
        // ListenerLocation Camera, as Studio's Baseplate has it.
        w.dom
            .set_property_by_name(ss, "DefaultListenerLocation", Value::Enum(3))
            .unwrap();
    }
    let first = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        first,
        "LocalScript",
        "Graph",
        r#"
        local SS = game:GetService('SoundService')
        local function wire(a, b)
            local w = Instance.new('Wire') w.SourceInstance = a w.TargetInstance = b w.Parent = workspace
        end
        local function part(z)
            local p = Instance.new('Part') p.Anchored = true p.CFrame = CFrame.new(0, 500, z)
            p.Parent = workspace return p
        end
        local p = Instance.new('AudioPlayer') p.Asset = 'rbxasset://sounds/tone.wav'
        p.Looping = true p.Volume = 0.5 p.Parent = workspace
        local direct = Instance.new('AudioAnalyzer') direct.Parent = workspace
        wire(p, direct)
        local em = Instance.new('AudioEmitter') em.AudioInteractionGroup = 'test' em.Parent = part(-20)
        local li = Instance.new('AudioListener') li.AudioInteractionGroup = 'test' li.Parent = part(0)
        local heard = Instance.new('AudioAnalyzer') heard.Parent = workspace
        wire(p, em) wire(li, heard)
        while not p.IsReady do task.wait() end
        local id = p:Play(SS:GetMixerTime() + 0.2)
        print('scheduled', typeof(id), p.IsPlaying)
        task.wait(0.4)
        print('playing', p.IsPlaying)
        task.wait(0.3)
        print('direct', direct.PeakLevel > 0.29 and direct.PeakLevel < 0.3, direct.RmsLevel > 0.1)
        print('heard', math.floor(heard.PeakLevel / direct.PeakLevel * 1000 + 0.5) / 1000,
            em:GetAudibilityFor(li), #direct:GetSpectrum())
        "#,
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.2);
    let client = session.client(index).unwrap();
    let log = lines(client);
    for line in [
        "scheduled number false",
        "playing true",
        "direct true true",
        "heard 0.2 0.20000000298023224 512",
    ] {
        assert!(log.contains(&line.to_owned()), "{line}: {log:?}");
    }

    // The default listener: under the camera, wired to an output under
    // SoundService, which the mixer hears.
    let camera = client.current_camera().unwrap();
    let dom = &client.runtime.world().dom;
    assert!(
        dom.find_first_child(camera, "AudioListener", false)
            .is_some()
    );
    let ss = dom.find_service("SoundService").unwrap();
    let output = dom
        .find_first_child(ss, "AudioDeviceOutput", false)
        .unwrap();
    let graph = client.audio_graph();
    assert!(graph.has_output());
    assert!(graph.wires.iter().any(|w| w.to == output));
}

/// A splitter's channel pin wired into an AudioChannelMixer's channel pin
/// carries sound (the mixer wasn't wired at all), and the graph keeps which
/// channel each end of the wire is.
#[test]
fn channel_splitter_into_mixer() {
    let options = tone_options("audio-api-channels");
    let mut server = DataModel::new(&options).unwrap();
    let first = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        first,
        "LocalScript",
        "Graph",
        r#"
        local function wire(a, b, from, to)
            local w = Instance.new('Wire') w.SourceInstance = a w.TargetInstance = b
            if from then w.SourceName = from end
            if to then w.TargetName = to end
            w.Parent = workspace
            return w
        end
        local p = Instance.new('AudioPlayer') p.Asset = 'rbxasset://sounds/tone.wav'
        p.Looping = true p.Parent = workspace
        local split = Instance.new('AudioChannelSplitter') split.Parent = workspace
        local mix = Instance.new('AudioChannelMixer') mix.Parent = workspace
        local heard = Instance.new('AudioAnalyzer') heard.Parent = workspace
        wire(p, split)
        local across = wire(split, mix, 'Left', 'Right')
        wire(mix, heard)
        print('connected', across.Connected, #mix:GetInputPins())
        while not p.IsReady do task.wait() end
        p:Play()
        task.wait(0.3)
        print('heard', heard.PeakLevel > 0.1)
        "#,
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.8);
    let client = session.client(index).unwrap();
    let log = lines(client);
    for line in ["connected true 13", "heard true"] {
        assert!(log.contains(&line.to_owned()), "{line}: {log:?}");
    }
    let graph = client.audio_graph();
    assert!(graph.wires.iter().any(|w| {
        w.from_channel == Some(rook_audio::graph::Channel::Left)
            && w.to_channel == Some(rook_audio::graph::Channel::Right)
    }));
}

/// AudioFilter, AudioFlanger, AudioTremolo, AudioLimiter and AudioGate
/// are heard: a limiter caps what reaches its analyzer, a gate whose
/// threshold the tone never reaches lets nothing through, `GetGainAt`
/// reads the filter's curve, and a setting a script changes reaches the
/// graph.
#[test]
fn filter_flanger_tremolo_limiter_and_gate_are_heard() {
    use rook_audio::effects::{Effect, FLANGER_SPAN, Filter, FilterType, Tremolo};
    use rook_audio::graph::NodeKind;
    let options = tone_options("audio-api-effects");
    let mut server = DataModel::new(&options).unwrap();
    let first = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        first,
        "LocalScript",
        "Effects",
        r#"
        local function wire(a, b)
            local w = Instance.new('Wire') w.SourceInstance = a w.TargetInstance = b w.Parent = workspace
        end
        local function new(class)
            local e = Instance.new(class) e.Parent = workspace return e
        end
        local p = new('AudioPlayer') p.Asset = 'rbxasset://sounds/tone.wav' p.Looping = true
        local limiter = new('AudioLimiter') limiter.MaxLevel = -12
        local gate = new('AudioGate') gate.Threshold = NumberRange.new(10, 20)
        local filter = new('AudioFilter') filter.FilterType = Enum.AudioFilterType.Lowpass12dB
        filter.Frequency = 1000
        local flanger, tremolo = new('AudioFlanger'), new('AudioTremolo')
        local limited, gated, shaped = new('AudioAnalyzer'), new('AudioAnalyzer'), new('AudioAnalyzer')
        wire(p, limiter) wire(limiter, limited)
        wire(p, gate) wire(gate, gated)
        wire(p, filter) wire(filter, flanger) wire(flanger, tremolo) wire(tremolo, shaped)
        print('gain', math.round(filter:GetGainAt(1000) * 10) / 10, math.abs(filter:GetGainAt(100)) < 0.1)
        while not p.IsReady do task.wait() end
        p:Play()
        task.wait(0.3)
        print('limited', limited.PeakLevel > 0.2, limited.PeakLevel <= 0.2512)
        print('gated', gated.PeakLevel, shaped.PeakLevel > 0)
        tremolo.Depth = 0.25
        "#,
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.8);
    let client = session.client(index).unwrap();
    let log = lines(client);
    for line in ["gain -3 true", "limited true true", "gated 0 true"] {
        assert!(log.contains(&line.to_owned()), "{line}: {log:?}");
    }
    let effects: Vec<Effect> = client
        .audio_graph()
        .nodes
        .into_iter()
        .filter_map(|n| match n.kind {
            NodeKind::Effect(e) => e,
            _ => None,
        })
        .collect();
    let expected = [
        Effect::Limiter {
            max_level: -12.0,
            release: 0.01,
        },
        Effect::Gate {
            threshold: [10.0, 20.0],
            attack: 0.01,
            release: 0.1,
        },
        Effect::Filter(Filter {
            kind: FilterType::Lowpass12dB,
            frequency: 1000.0,
            ..Filter::default()
        }),
        Effect::Flange {
            delay: 0.45 * FLANGER_SPAN,
            mix: 0.85,
            rate: 5.0,
        },
        Effect::Tremolo(Tremolo {
            depth: 0.25,
            ..Tremolo::default()
        }),
    ];
    for e in expected {
        assert!(effects.contains(&e), "{e:?} in {effects:?}");
    }
}
