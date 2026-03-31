use std::path::Path;

use crate::error::AudioError;
use crate::types::OutputFormat;

/// Encode interleaved f32 samples to an output file.
pub fn encode(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
    format: OutputFormat,
    path: &Path,
) -> Result<(), AudioError> {
    match format {
        OutputFormat::Wav => encode_wav(samples, channels, sample_rate, path),
        OutputFormat::Flac => encode_flac(samples, channels, sample_rate, path),
    }
}

fn encode_wav(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
    path: &Path,
) -> Result<(), AudioError> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    for &s in samples {
        let quantized = (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
        writer
            .write_sample(quantized)
            .map_err(|e| AudioError::EncodeFailed {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
    }
    writer.finalize().map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    Ok(())
}

fn encode_flac(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
    path: &Path,
) -> Result<(), AudioError> {
    use flacenc::component::BitRepr;
    use flacenc::config::Encoder as EncoderConfig;
    use flacenc::error::Verify;
    use flacenc::source::MemSource;

    let bits_per_sample = 16usize;
    let signal: Vec<i32> = samples
        .iter()
        .map(|&s| (s * 32767.0).round().clamp(-32768.0, 32767.0) as i32)
        .collect();

    let source = MemSource::from_samples(&signal, channels as usize, bits_per_sample, sample_rate as usize);
    let config = EncoderConfig::default().into_verified().map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: format!("FLAC config verify: {e:?}").into(),
    })?;
    let block_size = config.block_size;
    let flac_stream = flacenc::encode_with_fixed_block_size(
        &config,
        source,
        block_size,
    )
    .map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: format!("FLAC encode: {e:?}").into(),
    })?;

    let mut file = std::fs::File::create(path).map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let mut sink = flacenc::bitsink::ByteSink::new();
    flac_stream.write(&mut sink).map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: format!("FLAC write: {e:?}").into(),
    })?;
    std::io::Write::write_all(&mut file, sink.as_slice()).map_err(|e| AudioError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn wav_roundtrip_samples_match() {
        let spec = testutil::SignalSpec::default_mono();
        let original = testutil::generate_samples(&spec);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.wav");

        encode(&original, 1, 44100, OutputFormat::Wav, &path).unwrap();

        // Read back
        let mut reader = hound::WavReader::open(&path).unwrap();
        let read_back: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32767.0)
            .collect();

        assert_eq!(read_back.len(), original.len());
        // 16-bit quantization tolerance: 1/32768
        let tolerance = 1.0 / 32768.0 + 1e-6;
        testutil::assert_samples_approx_eq(&read_back, &original, tolerance);
    }

    #[test]
    fn wav_header_metadata_correct() {
        let spec = testutil::SignalSpec {
            sample_rate: 48000,
            channels: 2,
            duration_secs: 1.0,
            ..testutil::SignalSpec::default_stereo()
        };
        let samples = testutil::generate_samples(&spec);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("header_test.wav");

        encode(&samples, 2, 48000, OutputFormat::Wav, &path).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 48000);
        assert_eq!(reader.spec().bits_per_sample, 16);
    }

    #[test]
    fn flac_encodes_without_error() {
        let spec = testutil::SignalSpec::default_mono();
        let samples = testutil::generate_samples(&spec);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.flac");

        encode(&samples, 1, 44100, OutputFormat::Flac, &path).unwrap();
        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
    }

    #[test]
    fn flac_stereo_encodes_without_error() {
        let spec = testutil::SignalSpec::default_stereo();
        let samples = testutil::generate_samples(&spec);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stereo.flac");

        encode(&samples, 2, 44100, OutputFormat::Flac, &path).unwrap();
        assert!(path.exists());
    }
}
