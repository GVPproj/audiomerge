use audiomerge_core::testutil;
use audiomerge_core::types::*;

fn make_job(paths: Vec<std::path::PathBuf>, output: std::path::PathBuf, crossfade_secs: f64) -> Job {
    Job {
        tracks: paths.into_iter().map(|p| TrackConfig { path: p }).collect(),
        output: OutputConfig {
            path: output,
            format: OutputFormat::Wav,
        },
        crossfade: CrossfadeConfig {
            duration_secs: crossfade_secs,
            curve: CurvePreset::EqualPower,
        },
        normalize: NormalizeConfig::default(),
    }
}

#[test]
fn two_track_merge_output_duration_is_sum_minus_crossfade() {
    let dur = 6.0;
    let xfade = 1.0;
    let spec = testutil::SignalSpec {
        duration_secs: dur,
        ..testutil::SignalSpec::default_mono()
    };

    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.wav");
    let p2 = dir.path().join("b.wav");
    let out = dir.path().join("out.wav");
    testutil::write_wav(&spec, &p1);
    testutil::write_wav(&spec, &p2);

    let job = make_job(vec![p1, p2], out.clone(), xfade);
    audiomerge_core::run(job, None).unwrap();

    let reader = hound::WavReader::open(&out).unwrap();
    let actual_frames = reader.len() as usize / reader.spec().channels as usize;
    let actual_dur = actual_frames as f64 / reader.spec().sample_rate as f64;
    let expected_dur = dur * 2.0 - xfade;
    assert!(
        (actual_dur - expected_dur).abs() < 0.1,
        "expected ~{expected_dur}s, got {actual_dur}s"
    );
}

#[test]
fn three_tracks_different_amplitudes_output_near_target_lufs() {
    let dir = tempfile::tempdir().unwrap();

    // Three tracks at different amplitudes
    let loud = testutil::SignalSpec {
        duration_secs: 6.0,
        ..testutil::SignalSpec::loud_tone(440.0, 44100, 6.0)
    };
    let medium = testutil::SignalSpec {
        duration_secs: 6.0,
        ..testutil::SignalSpec::default_mono()
    };
    let quiet = testutil::SignalSpec {
        duration_secs: 6.0,
        ..testutil::SignalSpec::quiet_tone(440.0, 44100, 6.0)
    };

    let p1 = dir.path().join("loud.wav");
    let p2 = dir.path().join("medium.wav");
    let p3 = dir.path().join("quiet.wav");
    let out = dir.path().join("merged.wav");

    testutil::write_wav(&loud, &p1);
    testutil::write_wav(&medium, &p2);
    testutil::write_wav(&quiet, &p3);

    let target_lufs = -14.0;
    let job = Job {
        tracks: vec![
            TrackConfig { path: p1 },
            TrackConfig { path: p2 },
            TrackConfig { path: p3 },
        ],
        output: OutputConfig {
            path: out.clone(),
            format: OutputFormat::Wav,
        },
        crossfade: CrossfadeConfig {
            duration_secs: 0.5,
            curve: CurvePreset::EqualPower,
        },
        normalize: NormalizeConfig {
            target_lufs,
            ..Default::default()
        },
    };

    audiomerge_core::run(job, None).unwrap();

    // Decode output and measure loudness
    let decoded = audiomerge_core::decode::decode(&out).unwrap();
    let lufs = testutil::measure_loudness_lufs(&decoded.samples, decoded.channels, decoded.sample_rate);
    // The merged output should be reasonably close to target (within 2 LU given crossfade effects)
    assert!(
        (lufs - target_lufs).abs() < 2.0,
        "output loudness {lufs} LUFS, expected near {target_lufs}"
    );
}

#[test]
fn mono_plus_stereo_input_produces_stereo_output() {
    let dir = tempfile::tempdir().unwrap();

    let mono_spec = testutil::SignalSpec {
        duration_secs: 6.0,
        ..testutil::SignalSpec::default_mono()
    };
    let stereo_spec = testutil::SignalSpec {
        duration_secs: 6.0,
        ..testutil::SignalSpec::default_stereo()
    };

    let p1 = dir.path().join("mono.wav");
    let p2 = dir.path().join("stereo.wav");
    let out = dir.path().join("out.wav");

    testutil::write_wav(&mono_spec, &p1);
    testutil::write_wav(&stereo_spec, &p2);

    let job = make_job(vec![p1, p2], out.clone(), 0.5);
    audiomerge_core::run(job, None).unwrap();

    let reader = hound::WavReader::open(&out).unwrap();
    assert_eq!(reader.spec().channels, 2, "output should be stereo");
}

#[test]
fn flac_output_produces_valid_file() {
    let spec = testutil::SignalSpec {
        duration_secs: 6.0,
        ..testutil::SignalSpec::default_mono()
    };

    let dir = tempfile::tempdir().unwrap();
    let p1 = dir.path().join("a.wav");
    let p2 = dir.path().join("b.wav");
    let out = dir.path().join("out.flac");

    testutil::write_wav(&spec, &p1);
    testutil::write_wav(&spec, &p2);

    let job = Job {
        tracks: vec![
            TrackConfig { path: p1 },
            TrackConfig { path: p2 },
        ],
        output: OutputConfig {
            path: out.clone(),
            format: OutputFormat::Flac,
        },
        crossfade: CrossfadeConfig::default(),
        normalize: NormalizeConfig::default(),
    };

    audiomerge_core::run(job, None).unwrap();

    assert!(out.exists());
    assert!(std::fs::metadata(&out).unwrap().len() > 0);
}
