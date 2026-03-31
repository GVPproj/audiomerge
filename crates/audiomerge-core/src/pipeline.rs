use std::sync::mpsc::Sender;

use crate::error::AudioError;
use crate::types::{Job, ProgressEvent, Stage};

/// Run the full audiomerge pipeline.
///
/// This is the single entry point for all audiomerge processing.
pub fn run(job: Job, progress: Option<Sender<ProgressEvent>>) -> Result<(), AudioError> {
    // Validate inputs
    if job.tracks.len() < 2 {
        return Err(AudioError::TooFewTracks {
            count: job.tracks.len(),
        });
    }

    send_progress(&progress, Stage::Probe, None, "Starting probe...");

    // Stage 1: Probe all inputs
    let probe_results: Vec<_> = job
        .tracks
        .iter()
        .map(|t| crate::probe::probe(&t.path))
        .collect::<Result<Vec<_>, _>>()?;

    let target_sample_rate = probe_results.iter().map(|p| p.sample_rate).max().unwrap();
    let target_channels = probe_results.iter().map(|p| p.channels).max().unwrap();

    // Validate minimum duration
    let min_duration = 5.0;
    for (i, pr) in probe_results.iter().enumerate() {
        if pr.duration_secs < min_duration {
            return Err(AudioError::TrackTooShort {
                path: job.tracks[i].path.clone(),
                duration_secs: pr.duration_secs,
                min_secs: min_duration,
            });
        }
    }

    // Validate crossfade duration
    let shortest = probe_results
        .iter()
        .map(|p| p.duration_secs)
        .fold(f64::INFINITY, f64::min);
    if job.crossfade.duration_secs >= shortest {
        return Err(AudioError::CrossfadeTooLong {
            crossfade_secs: job.crossfade.duration_secs,
            shortest_track_secs: shortest,
        });
    }

    // Stage 2: Decode all tracks
    send_progress(&progress, Stage::Decode, None, "Decoding tracks...");
    let mut decoded: Vec<_> = job
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            send_progress(&progress, Stage::Decode, Some(i), &format!("Decoding track {}", i + 1));
            crate::decode::decode(&t.path)
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Stage 3: Resample + channel normalization
    send_progress(&progress, Stage::Resample, None, "Resampling...");
    for (i, track) in decoded.iter_mut().enumerate() {
        if track.sample_rate != target_sample_rate {
            send_progress(
                &progress,
                Stage::Resample,
                Some(i),
                &format!("Resampling track {} from {} to {}", i + 1, track.sample_rate, target_sample_rate),
            );
            track.samples =
                crate::resample::resample(&track.samples, track.channels, track.sample_rate, target_sample_rate)?;
            track.sample_rate = target_sample_rate;
        }
        if track.channels < target_channels {
            if track.channels == 1 && target_channels == 2 {
                track.samples = crate::resample::upmix_mono_to_stereo(&track.samples);
                track.channels = 2;
            }
        }
    }

    // Stage 4: Normalize
    send_progress(&progress, Stage::Normalize, None, "Normalizing loudness...");
    for (i, track) in decoded.iter_mut().enumerate() {
        send_progress(
            &progress,
            Stage::Normalize,
            Some(i),
            &format!("Normalizing track {}", i + 1),
        );
        track.samples = crate::normalize::normalize(
            &track.samples,
            track.channels,
            track.sample_rate,
            &job.normalize,
        )?;
    }

    // Stage 5: Crossfade
    send_progress(&progress, Stage::Crossfade, None, "Applying crossfades...");
    let overlap_frames = (job.crossfade.duration_secs * target_sample_rate as f64) as usize;

    let mut merged = decoded.remove(0).samples;
    for track in decoded {
        merged = crate::crossfade::crossfade(
            &merged,
            &track.samples,
            target_channels,
            overlap_frames,
            job.crossfade.curve,
        );
    }

    // Stage 6: Encode
    send_progress(&progress, Stage::Encode, None, "Encoding output...");
    crate::encode::encode(
        &merged,
        target_channels,
        target_sample_rate,
        job.output.format,
        &job.output.path,
    )?;

    send_progress(&progress, Stage::Done, None, "Done!");
    Ok(())
}

fn send_progress(
    tx: &Option<Sender<ProgressEvent>>,
    stage: Stage,
    track: Option<usize>,
    message: &str,
) {
    if let Some(tx) = tx {
        let _ = tx.send(ProgressEvent {
            stage,
            track,
            message: message.to_string(),
            percent: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use crate::types::*;

    fn make_test_job(paths: Vec<std::path::PathBuf>, output_path: std::path::PathBuf) -> Job {
        Job {
            tracks: paths.into_iter().map(|p| TrackConfig { path: p }).collect(),
            output: OutputConfig {
                path: output_path,
                format: OutputFormat::Wav,
            },
            crossfade: CrossfadeConfig {
                duration_secs: 0.5,
                ..Default::default()
            },
            normalize: NormalizeConfig::default(),
        }
    }

    #[test]
    fn too_few_tracks_errors() {
        let dir = tempfile::tempdir().unwrap();
        let spec = testutil::SignalSpec::default_mono();
        let path1 = dir.path().join("a.wav");
        testutil::write_wav(&spec, &path1);

        let job = make_test_job(vec![path1], dir.path().join("out.wav"));
        let result = run(job, None);
        assert!(matches!(result, Err(AudioError::TooFewTracks { count: 1 })));
    }

    #[test]
    fn two_track_merge_produces_output() {
        let spec = testutil::SignalSpec {
            duration_secs: 6.0,
            ..testutil::SignalSpec::default_mono()
        };

        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.wav");
        let p2 = dir.path().join("b.wav");
        let out = dir.path().join("out.wav");
        testutil::write_wav(&spec, &p1);
        testutil::write_wav(&spec, &p2);

        let job = make_test_job(vec![p1, p2], out.clone());
        run(job, None).unwrap();

        assert!(out.exists());
        let reader = hound::WavReader::open(&out).unwrap();
        assert!(reader.len() > 0);
    }

    #[test]
    fn three_track_merge_produces_output() {
        let spec = testutil::SignalSpec {
            duration_secs: 6.0,
            ..testutil::SignalSpec::default_mono()
        };

        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.wav");
        let p2 = dir.path().join("b.wav");
        let p3 = dir.path().join("c.wav");
        let out = dir.path().join("out.wav");
        testutil::write_wav(&spec, &p1);
        testutil::write_wav(&spec, &p2);
        testutil::write_wav(&spec, &p3);

        let job = make_test_job(vec![p1, p2, p3], out.clone());
        run(job, None).unwrap();

        assert!(out.exists());
    }

    #[test]
    fn progress_events_received() {
        let spec = testutil::SignalSpec {
            duration_secs: 6.0,
            ..testutil::SignalSpec::default_mono()
        };

        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.wav");
        let p2 = dir.path().join("b.wav");
        let out = dir.path().join("out.wav");
        testutil::write_wav(&spec, &p1);
        testutil::write_wav(&spec, &p2);

        let (tx, rx) = std::sync::mpsc::channel();
        let job = make_test_job(vec![p1, p2], out);
        run(job, Some(tx)).unwrap();

        let events: Vec<_> = rx.try_iter().collect();
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| e.stage == Stage::Done));
    }
}
