use std::path::Path;

use crate::error::AudioError;
use crate::types::ProbeResult;

/// Probe an audio file to extract metadata without fully decoding.
pub fn probe(path: &Path) -> Result<ProbeResult, AudioError> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    if !path.exists() {
        return Err(AudioError::FileNotFound(path.to_path_buf()));
    }

    let file = std::fs::File::open(path).map_err(|_| AudioError::FileNotFound(path.to_path_buf()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| AudioError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| AudioError::DecodeFailed {
            path: path.to_path_buf(),
            source: "no default track found".into(),
        })?;

    let codec_params = &track.codec_params;

    let sample_rate = codec_params.sample_rate.unwrap_or(0);
    let channels = codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(0);

    let codec_name = format!("{:?}", codec_params.codec);

    let duration_secs = codec_params
        .n_frames
        .map(|n| n as f64 / sample_rate as f64)
        .unwrap_or(0.0);

    Ok(ProbeResult {
        duration_secs,
        sample_rate,
        channels,
        codec: codec_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use std::path::PathBuf;

    #[test]
    fn probe_wav_returns_correct_metadata() {
        let spec = testutil::SignalSpec {
            sample_rate: 48000,
            channels: 2,
            duration_secs: 2.0,
            ..testutil::SignalSpec::default_stereo()
        };
        let (_dir, path) = testutil::write_wav_temp(&spec, "probe_test.wav");
        let result = probe(&path).unwrap();
        assert_eq!(result.sample_rate, 48000);
        assert_eq!(result.channels, 2);
        assert!((result.duration_secs - 2.0).abs() < 0.1, "duration: {}", result.duration_secs);
    }

    #[test]
    fn probe_mono_wav() {
        let spec = testutil::SignalSpec::default_mono();
        let (_dir, path) = testutil::write_wav_temp(&spec, "mono.wav");
        let result = probe(&path).unwrap();
        assert_eq!(result.channels, 1);
        assert_eq!(result.sample_rate, 44100);
    }

    #[test]
    fn probe_nonexistent_file_returns_error() {
        let result = probe(Path::new("/tmp/definitely_does_not_exist_98765.wav"));
        assert!(result.is_err());
        match result.unwrap_err() {
            AudioError::FileNotFound(p) => {
                assert!(p.to_string_lossy().contains("definitely_does_not_exist"));
            }
            other => panic!("expected FileNotFound, got: {other}"),
        }
    }
}
