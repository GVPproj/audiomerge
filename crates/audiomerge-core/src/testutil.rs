//! Shared test helper module for generating synthetic audio and verifying results.
//!
//! Gated behind `#[cfg(any(test, feature = "test-helpers"))]`.

use std::path::{Path, PathBuf};

/// A single frequency component in a synthetic signal.
#[derive(Debug, Clone)]
pub struct SignalComponent {
    pub frequency_hz: f64,
    pub amplitude: f64,
    pub phase: f64,
}

/// Configuration for generating a synthetic audio signal.
#[derive(Debug, Clone)]
pub struct SignalSpec {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    pub components: Vec<SignalComponent>,
}

impl SignalSpec {
    /// 440 Hz mono tone, 44100 Hz, 3s, amplitude 0.5
    pub fn default_mono() -> Self {
        Self {
            sample_rate: 44100,
            channels: 1,
            duration_secs: 3.0,
            components: vec![SignalComponent {
                frequency_hz: 440.0,
                amplitude: 0.5,
                phase: 0.0,
            }],
        }
    }

    /// 440 Hz stereo tone, 44100 Hz, 3s, amplitude 0.5
    pub fn default_stereo() -> Self {
        Self {
            channels: 2,
            ..Self::default_mono()
        }
    }

    /// All-zero buffer.
    pub fn silence(sample_rate: u32, channels: u16, duration_secs: f64) -> Self {
        Self {
            sample_rate,
            channels,
            duration_secs,
            components: vec![],
        }
    }

    /// High-amplitude tone (0.95) for testing normalization downward.
    pub fn loud_tone(frequency_hz: f64, sample_rate: u32, duration_secs: f64) -> Self {
        Self {
            sample_rate,
            channels: 1,
            duration_secs,
            components: vec![SignalComponent {
                frequency_hz,
                amplitude: 0.95,
                phase: 0.0,
            }],
        }
    }

    /// Low-amplitude tone (0.01) for testing normalization upward.
    pub fn quiet_tone(frequency_hz: f64, sample_rate: u32, duration_secs: f64) -> Self {
        Self {
            sample_rate,
            channels: 1,
            duration_secs,
            components: vec![SignalComponent {
                frequency_hz,
                amplitude: 0.01,
                phase: 0.0,
            }],
        }
    }

    /// Multi-frequency signal.
    pub fn composite(
        freqs: &[f64],
        sample_rate: u32,
        channels: u16,
        duration_secs: f64,
    ) -> Self {
        let amplitude = 1.0 / freqs.len() as f64;
        Self {
            sample_rate,
            channels,
            duration_secs,
            components: freqs
                .iter()
                .map(|&f| SignalComponent {
                    frequency_hz: f,
                    amplitude,
                    phase: 0.0,
                })
                .collect(),
        }
    }
}

/// Generate interleaved f32 samples from a SignalSpec.
pub fn generate_samples(spec: &SignalSpec) -> Vec<f32> {
    let num_frames = (spec.sample_rate as f64 * spec.duration_secs) as usize;
    let num_samples = num_frames * spec.channels as usize;
    let mut samples = vec![0.0f32; num_samples];

    for frame in 0..num_frames {
        let t = frame as f64 / spec.sample_rate as f64;
        let mut value = 0.0f64;
        for comp in &spec.components {
            value +=
                comp.amplitude * (2.0 * std::f64::consts::PI * comp.frequency_hz * t + comp.phase).sin();
        }
        let sample = value as f32;
        for ch in 0..spec.channels as usize {
            samples[frame * spec.channels as usize + ch] = sample;
        }
    }

    samples
}

/// Generate a silent buffer.
pub fn generate_silence(sample_rate: u32, channels: u16, duration_secs: f64) -> Vec<f32> {
    generate_samples(&SignalSpec::silence(sample_rate, channels, duration_secs))
}

/// Write a SignalSpec as a 16-bit WAV file.
pub fn write_wav(spec: &SignalSpec, path: &Path) {
    let samples = generate_samples(spec);
    let wav_spec = hound::WavSpec {
        channels: spec.channels,
        sample_rate: spec.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, wav_spec).expect("failed to create WAV writer");
    for &s in &samples {
        let quantized = (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(quantized).expect("failed to write sample");
    }
    writer.finalize().expect("failed to finalize WAV");
}

/// Write a SignalSpec as a WAV file in a temp directory. Returns (TempDir, PathBuf).
pub fn write_wav_temp(spec: &SignalSpec, filename: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().join(filename);
    write_wav(spec, &path);
    (dir, path)
}

/// Root mean square amplitude of a sample buffer.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Peak absolute amplitude of a sample buffer.
pub fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max)
}

/// Number of frames in an interleaved sample buffer.
pub fn frame_count(samples: &[f32], channels: u16) -> usize {
    samples.len() / channels as usize
}

/// Duration in seconds of an interleaved sample buffer.
pub fn duration_secs(samples: &[f32], channels: u16, sample_rate: u32) -> f64 {
    frame_count(samples, channels) as f64 / sample_rate as f64
}

/// Assert that two sample buffers are approximately equal within tolerance.
pub fn assert_samples_approx_eq(a: &[f32], b: &[f32], tolerance: f32) {
    assert_eq!(a.len(), b.len(), "buffer lengths differ: {} vs {}", a.len(), b.len());
    for (i, (&sa, &sb)) in a.iter().zip(b.iter()).enumerate() {
        assert!(
            (sa - sb).abs() <= tolerance,
            "sample {i} differs: {sa} vs {sb} (tolerance {tolerance})"
        );
    }
}

/// Measure integrated loudness (LUFS) of an interleaved f32 buffer using the ebur128 crate.
pub fn measure_loudness_lufs(samples: &[f32], channels: u16, sample_rate: u32) -> f64 {
    use ebur128::{EbuR128, Mode};

    let mut meter = EbuR128::new(channels as u32, sample_rate, Mode::I).expect("failed to create EbuR128");
    meter.add_frames_f32(samples).expect("failed to add frames");
    meter.loudness_global().expect("failed to measure loudness")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mono_generates_correct_length() {
        let spec = SignalSpec::default_mono();
        let samples = generate_samples(&spec);
        assert_eq!(samples.len(), 44100 * 3); // 3s mono
    }

    #[test]
    fn default_stereo_generates_correct_length() {
        let spec = SignalSpec::default_stereo();
        let samples = generate_samples(&spec);
        assert_eq!(samples.len(), 44100 * 3 * 2); // 3s stereo
    }

    #[test]
    fn silence_generates_zeros() {
        let samples = generate_silence(44100, 1, 1.0);
        assert!(samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn rms_of_silence_is_zero() {
        let samples = generate_silence(44100, 1, 1.0);
        assert_eq!(rms(&samples), 0.0);
    }

    #[test]
    fn peak_of_silence_is_zero() {
        let samples = generate_silence(44100, 1, 1.0);
        assert_eq!(peak(&samples), 0.0);
    }

    #[test]
    fn rms_of_tone_is_positive() {
        let samples = generate_samples(&SignalSpec::default_mono());
        assert!(rms(&samples) > 0.0);
    }

    #[test]
    fn peak_of_loud_tone_near_amplitude() {
        let spec = SignalSpec::loud_tone(440.0, 44100, 1.0);
        let samples = generate_samples(&spec);
        let p = peak(&samples);
        assert!(p > 0.9 && p <= 1.0, "peak was {p}");
    }

    #[test]
    fn frame_count_correct() {
        let samples = generate_samples(&SignalSpec::default_stereo());
        assert_eq!(frame_count(&samples, 2), 44100 * 3);
    }

    #[test]
    fn duration_secs_correct() {
        let spec = SignalSpec::default_mono();
        let samples = generate_samples(&spec);
        let dur = duration_secs(&samples, 1, 44100);
        assert!((dur - 3.0).abs() < 0.001, "duration was {dur}");
    }

    #[test]
    fn write_wav_roundtrip() {
        let spec = SignalSpec::default_mono();
        let (_dir, path) = write_wav_temp(&spec, "test.wav");
        // Verify file exists and is readable by hound
        let reader = hound::WavReader::open(&path).expect("failed to open WAV");
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 44100);
        assert_eq!(reader.spec().bits_per_sample, 16);
    }

    #[test]
    fn measure_loudness_returns_finite() {
        let spec = SignalSpec::default_mono();
        let samples = generate_samples(&spec);
        let lufs = measure_loudness_lufs(&samples, 1, 44100);
        assert!(lufs.is_finite(), "lufs was {lufs}");
        assert!(lufs < 0.0, "lufs should be negative, was {lufs}");
    }

    #[test]
    fn loud_tone_louder_than_quiet_tone() {
        let loud = generate_samples(&SignalSpec::loud_tone(440.0, 44100, 3.0));
        let quiet = generate_samples(&SignalSpec::quiet_tone(440.0, 44100, 3.0));
        let loud_lufs = measure_loudness_lufs(&loud, 1, 44100);
        let quiet_lufs = measure_loudness_lufs(&quiet, 1, 44100);
        assert!(loud_lufs > quiet_lufs, "loud {loud_lufs} should be > quiet {quiet_lufs}");
    }

    #[test]
    fn assert_samples_approx_eq_passes_for_identical() {
        let a = vec![0.1, 0.2, 0.3];
        assert_samples_approx_eq(&a, &a, 0.0);
    }

    #[test]
    #[should_panic(expected = "sample 0 differs")]
    fn assert_samples_approx_eq_fails_for_different() {
        let a = vec![0.1];
        let b = vec![0.5];
        assert_samples_approx_eq(&a, &b, 0.01);
    }
}
