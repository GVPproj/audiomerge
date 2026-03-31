use crate::error::AudioError;

/// Resample an interleaved f32 buffer from `from_rate` to `to_rate`.
///
/// If rates are equal, returns a clone (passthrough).
pub fn resample(
    samples: &[f32],
    channels: u16,
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, AudioError> {
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }

    let ch = channels as usize;
    let num_frames_in = samples.len() / ch;

    if num_frames_in == 0 {
        return Ok(vec![]);
    }

    use rubato::{FftFixedInOut, Resampler};

    let chunk_size = 1024;
    let mut resampler = FftFixedInOut::<f32>::new(from_rate as usize, to_rate as usize, chunk_size, ch)
        .map_err(|e| AudioError::ResampleFailed {
            source: format!("rubato init: {e}").into(),
        })?;

    // De-interleave into per-channel vectors
    let mut channel_bufs: Vec<Vec<f32>> = vec![vec![]; ch];
    for (i, &s) in samples.iter().enumerate() {
        channel_bufs[i % ch].push(s);
    }

    let actual_chunk = resampler.input_frames_next();
    let mut output_channels: Vec<Vec<f32>> = vec![vec![]; ch];

    // Process full chunks
    let mut pos = 0;
    while pos + actual_chunk <= num_frames_in {
        let input: Vec<&[f32]> = channel_bufs.iter().map(|c| &c[pos..pos + actual_chunk]).collect();
        let out = resampler.process(&input, None).map_err(|e| AudioError::ResampleFailed {
            source: format!("rubato process: {e}").into(),
        })?;
        for (c, buf) in out.iter().enumerate() {
            output_channels[c].extend_from_slice(buf);
        }
        pos += actual_chunk;
    }

    // Handle remaining frames by padding with zeros
    if pos < num_frames_in {
        let remaining = num_frames_in - pos;
        let padded: Vec<Vec<f32>> = channel_bufs
            .iter()
            .map(|c| {
                let mut v = c[pos..].to_vec();
                v.resize(actual_chunk, 0.0);
                v
            })
            .collect();
        let input: Vec<&[f32]> = padded.iter().map(|c| c.as_slice()).collect();
        let out = resampler.process(&input, None).map_err(|e| AudioError::ResampleFailed {
            source: format!("rubato process tail: {e}").into(),
        })?;

        // Only take the proportional amount of output
        let expected_out = (remaining as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;
        for (c, buf) in out.iter().enumerate() {
            let take = expected_out.min(buf.len());
            output_channels[c].extend_from_slice(&buf[..take]);
        }
    }

    // Re-interleave
    let out_frames = output_channels[0].len();
    let mut interleaved = vec![0.0f32; out_frames * ch];
    for f in 0..out_frames {
        for c in 0..ch {
            interleaved[f * ch + c] = output_channels[c][f];
        }
    }

    Ok(interleaved)
}

/// Upmix mono to stereo by duplicating the channel.
pub fn upmix_mono_to_stereo(samples: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        out.push(s);
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn same_rate_is_passthrough() {
        let spec = testutil::SignalSpec::default_mono();
        let samples = testutil::generate_samples(&spec);
        let result = resample(&samples, 1, 44100, 44100).unwrap();
        assert_eq!(result.len(), samples.len());
        testutil::assert_samples_approx_eq(&result, &samples, 0.0);
    }

    #[test]
    fn duration_preserved_after_upsample() {
        let spec = testutil::SignalSpec {
            sample_rate: 22050,
            channels: 1,
            duration_secs: 1.0,
            ..testutil::SignalSpec::default_mono()
        };
        let samples = testutil::generate_samples(&spec);
        let result = resample(&samples, 1, 22050, 44100).unwrap();
        let in_dur = testutil::duration_secs(&samples, 1, 22050);
        let out_dur = testutil::duration_secs(&result, 1, 44100);
        assert!(
            (in_dur - out_dur).abs() < 0.05,
            "duration changed: {in_dur} -> {out_dur}"
        );
    }

    #[test]
    fn duration_preserved_after_downsample() {
        let spec = testutil::SignalSpec {
            sample_rate: 48000,
            channels: 1,
            duration_secs: 1.0,
            ..testutil::SignalSpec::default_mono()
        };
        let samples = testutil::generate_samples(&spec);
        let result = resample(&samples, 1, 48000, 44100).unwrap();
        let in_dur = testutil::duration_secs(&samples, 1, 48000);
        let out_dur = testutil::duration_secs(&result, 1, 44100);
        assert!(
            (in_dur - out_dur).abs() < 0.05,
            "duration changed: {in_dur} -> {out_dur}"
        );
    }

    #[test]
    fn mono_to_stereo_upmix_duplicates() {
        let mono = vec![1.0f32, 2.0, 3.0];
        let stereo = upmix_mono_to_stereo(&mono);
        assert_eq!(stereo, vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    }

    #[test]
    fn empty_resample_returns_empty() {
        let result = resample(&[], 1, 44100, 48000).unwrap();
        assert!(result.is_empty());
    }
}
