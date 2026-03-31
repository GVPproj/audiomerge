use crate::error::AudioError;
use crate::types::NormalizeConfig;

/// Normalize an interleaved f32 sample buffer to the target LUFS and apply true-peak limiting.
///
/// Returns a new buffer with the same length and channel layout.
pub fn normalize(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
    config: &NormalizeConfig,
) -> Result<Vec<f32>, AudioError> {
    if samples.is_empty() {
        return Ok(vec![]);
    }

    // Measure integrated loudness
    let measured_lufs = measure_loudness(samples, channels, sample_rate)?;

    // If the signal is effectively silent, return as-is
    if !measured_lufs.is_finite() || measured_lufs < -70.0 {
        return Ok(samples.to_vec());
    }

    // Calculate gain to reach target
    let gain_db = config.target_lufs - measured_lufs;
    let gain_linear = 10.0_f64.powf(gain_db / 20.0) as f32;

    // Apply gain
    let mut out: Vec<f32> = samples.iter().map(|&s| s * gain_linear).collect();

    // True-peak limiting
    let ceiling = 10.0_f64.powf(config.true_peak_dbfs / 20.0) as f32;
    for s in &mut out {
        if *s > ceiling {
            *s = ceiling;
        } else if *s < -ceiling {
            *s = -ceiling;
        }
    }

    Ok(out)
}

fn measure_loudness(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
) -> Result<f64, AudioError> {
    use ebur128::{EbuR128, Mode};

    let mut meter = EbuR128::new(channels as u32, sample_rate, Mode::I | Mode::TRUE_PEAK)
        .map_err(|e| AudioError::ResampleFailed {
            source: format!("ebur128 init: {e}").into(),
        })?;
    meter.add_frames_f32(samples).map_err(|e| AudioError::ResampleFailed {
        source: format!("ebur128 add_frames: {e}").into(),
    })?;
    meter.loudness_global().map_err(|e| AudioError::ResampleFailed {
        source: format!("ebur128 loudness: {e}").into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn loud_signal_attenuated_to_target() {
        let spec = testutil::SignalSpec::loud_tone(440.0, 44100, 5.0);
        let samples = testutil::generate_samples(&spec);
        let config = NormalizeConfig::default();
        let result = normalize(&samples, 1, 44100, &config).unwrap();
        let lufs = testutil::measure_loudness_lufs(&result, 1, 44100);
        assert!(
            (lufs - config.target_lufs).abs() < 0.5,
            "expected ~{} LUFS, got {lufs}",
            config.target_lufs
        );
    }

    #[test]
    fn quiet_signal_amplified_to_target() {
        let spec = testutil::SignalSpec::quiet_tone(440.0, 44100, 5.0);
        let samples = testutil::generate_samples(&spec);
        let config = NormalizeConfig::default();
        let result = normalize(&samples, 1, 44100, &config).unwrap();
        let lufs = testutil::measure_loudness_lufs(&result, 1, 44100);
        assert!(
            (lufs - config.target_lufs).abs() < 0.5,
            "expected ~{} LUFS, got {lufs}",
            config.target_lufs
        );
    }

    #[test]
    fn true_peak_ceiling_respected() {
        let spec = testutil::SignalSpec::loud_tone(440.0, 44100, 5.0);
        let samples = testutil::generate_samples(&spec);
        let config = NormalizeConfig {
            target_lufs: -6.0, // push loud
            true_peak_dbfs: -1.5,
            ..Default::default()
        };
        let result = normalize(&samples, 1, 44100, &config).unwrap();
        let ceiling = 10.0_f64.powf(config.true_peak_dbfs / 20.0) as f32;
        let p = testutil::peak(&result);
        assert!(
            p <= ceiling + 1e-6,
            "peak {p} exceeds ceiling {ceiling}"
        );
    }

    #[test]
    fn silence_stays_silent() {
        let samples = testutil::generate_silence(44100, 1, 3.0);
        let config = NormalizeConfig::default();
        let result = normalize(&samples, 1, 44100, &config).unwrap();
        assert_eq!(testutil::peak(&result), 0.0);
    }

    #[test]
    fn channel_count_preserved() {
        let spec = testutil::SignalSpec::default_stereo();
        let samples = testutil::generate_samples(&spec);
        let config = NormalizeConfig::default();
        let result = normalize(&samples, 2, 44100, &config).unwrap();
        assert_eq!(result.len(), samples.len());
    }

    #[test]
    fn empty_input_returns_empty() {
        let config = NormalizeConfig::default();
        let result = normalize(&[], 1, 44100, &config).unwrap();
        assert!(result.is_empty());
    }
}
