use std::path::Path;

use crate::error::AudioError;

/// Decoded audio data.
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Fully decode an audio file to interleaved f32 samples.
pub fn decode(path: &Path) -> Result<DecodedAudio, AudioError> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    if !path.exists() {
        return Err(AudioError::FileNotFound(path.to_path_buf()));
    }

    let file = std::fs::File::open(path).map_err(|e| AudioError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
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

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| AudioError::DecodeFailed {
            path: path.to_path_buf(),
            source: "no default track".into(),
        })?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(1);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| AudioError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let mut all_samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => {
                return Err(AudioError::DecodeFailed {
                    path: path.to_path_buf(),
                    source: Box::new(e),
                });
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = decoder.decode(&packet).map_err(|e| AudioError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

        let spec = *decoded.spec();
        let duration = decoded.capacity();
        let mut sample_buf = SampleBuffer::<f32>::new(duration as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        all_samples.extend_from_slice(sample_buf.samples());
    }

    Ok(DecodedAudio {
        samples: all_samples,
        sample_rate,
        channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn decode_wav_produces_correct_sample_count() {
        let spec = testutil::SignalSpec::default_mono();
        let (_dir, path) = testutil::write_wav_temp(&spec, "decode_test.wav");
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.channels, 1);
        assert_eq!(decoded.sample_rate, 44100);
        // 16-bit WAV quantization means we won't get exactly the same float values,
        // but the frame count should match
        let expected_frames = (44100.0 * 3.0) as usize;
        let actual_frames = decoded.samples.len() / decoded.channels as usize;
        assert_eq!(actual_frames, expected_frames);
    }

    #[test]
    fn decode_stereo_wav() {
        let spec = testutil::SignalSpec::default_stereo();
        let (_dir, path) = testutil::write_wav_temp(&spec, "stereo.wav");
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.channels, 2);
        let expected_frames = (44100.0 * 3.0) as usize;
        let actual_frames = decoded.samples.len() / decoded.channels as usize;
        assert_eq!(actual_frames, expected_frames);
    }

    #[test]
    fn decode_nonexistent_file_errors() {
        let result = decode(Path::new("/tmp/no_such_file_999.wav"));
        assert!(result.is_err());
    }
}
