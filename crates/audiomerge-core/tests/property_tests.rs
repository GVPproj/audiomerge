use proptest::prelude::*;

use audiomerge_core::crossfade::{crossfade, fade_in, fade_out};
use audiomerge_core::normalize::normalize;
use audiomerge_core::types::{CurvePreset, NormalizeConfig};

/// Strategy for generating a CurvePreset.
fn curve_preset() -> impl Strategy<Value = CurvePreset> {
    prop_oneof![
        Just(CurvePreset::Linear),
        Just(CurvePreset::EqualPower),
        Just(CurvePreset::Sinusoidal),
        Just(CurvePreset::Cubic),
        Just(CurvePreset::Exponential),
    ]
}

// ---------------------------------------------------------------------------
// Crossfade length invariant
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn crossfade_output_length_invariant(
        a_frames in 10usize..500,
        b_frames in 10usize..500,
        channels in prop_oneof![Just(1u16), Just(2u16)],
        curve in curve_preset(),
    ) {
        // overlap must be <= min(a_frames, b_frames)
        let max_overlap = a_frames.min(b_frames);
        // pick an overlap in range [0, max_overlap]
        let overlap_frames = if max_overlap == 0 { 0 } else { max_overlap / 2 };

        let a = vec![0.5f32; a_frames * channels as usize];
        let b = vec![0.5f32; b_frames * channels as usize];

        let result = crossfade(&a, &b, channels, overlap_frames, curve);
        let expected_len = (a_frames + b_frames - overlap_frames) * channels as usize;
        prop_assert_eq!(result.len(), expected_len);
    }

    #[test]
    fn crossfade_output_length_with_random_overlap(
        a_frames in 20usize..300,
        b_frames in 20usize..300,
        overlap_frac in 0.0f64..1.0,
        channels in prop_oneof![Just(1u16), Just(2u16)],
        curve in curve_preset(),
    ) {
        let max_overlap = a_frames.min(b_frames);
        let overlap_frames = (overlap_frac * max_overlap as f64).floor() as usize;

        let a = vec![0.3f32; a_frames * channels as usize];
        let b = vec![0.3f32; b_frames * channels as usize];

        let result = crossfade(&a, &b, channels, overlap_frames, curve);
        let expected_len = (a_frames + b_frames - overlap_frames) * channels as usize;
        prop_assert_eq!(result.len(), expected_len);
    }
}

// ---------------------------------------------------------------------------
// Crossfade output bounded: overlap region samples <= sum of input amplitudes
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn crossfade_overlap_bounded(
        a_frames in 20usize..200,
        b_frames in 20usize..200,
        amplitude in 0.01f32..0.9,
        curve in curve_preset(),
    ) {
        let channels = 1u16;
        let max_overlap = a_frames.min(b_frames);
        let overlap_frames = max_overlap / 2;

        let a = vec![amplitude; a_frames];
        let b = vec![amplitude; b_frames];

        let result = crossfade(&a, &b, channels, overlap_frames, curve);

        // The overlap region starts at (a_frames - overlap_frames) samples
        let overlap_start = a_frames - overlap_frames;
        let overlap_end = overlap_start + overlap_frames;

        for i in overlap_start..overlap_end {
            let sample = result[i].abs();
            // Each sample in the overlap should be <= sum of input amplitudes
            // (fade_out * a + fade_in * b, both gains <= 1.0)
            prop_assert!(
                sample <= amplitude * 2.0 + 1e-6,
                "sample {} = {} exceeds 2 * amplitude = {}",
                i, sample, amplitude * 2.0
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Fade functions: boundary values hold for all curves
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn fade_boundaries(curve in curve_preset()) {
        let fo_0 = fade_out(curve, 0.0);
        let fo_1 = fade_out(curve, 1.0);
        let fi_0 = fade_in(curve, 0.0);
        let fi_1 = fade_in(curve, 1.0);

        prop_assert!((fo_0 - 1.0).abs() < 1e-10, "fade_out(0) should be 1.0, got {fo_0}");
        prop_assert!(fo_1.abs() < 1e-10, "fade_out(1) should be 0.0, got {fo_1}");
        prop_assert!(fi_0.abs() < 1e-10, "fade_in(0) should be 0.0, got {fi_0}");
        prop_assert!((fi_1 - 1.0).abs() < 1e-10, "fade_in(1) should be 1.0, got {fi_1}");
    }

    #[test]
    fn fade_monotonic(
        curve in curve_preset(),
        t1 in 0.0f64..1.0,
        t2 in 0.0f64..1.0,
    ) {
        let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        // fade_out should be non-increasing
        prop_assert!(
            fade_out(curve, lo) >= fade_out(curve, hi) - 1e-10,
            "fade_out not monotonically decreasing: f({lo})={} < f({hi})={}",
            fade_out(curve, lo), fade_out(curve, hi)
        );
        // fade_in should be non-decreasing
        prop_assert!(
            fade_in(curve, hi) >= fade_in(curve, lo) - 1e-10,
            "fade_in not monotonically increasing: f({hi})={} < f({lo})={}",
            fade_in(curve, hi), fade_in(curve, lo)
        );
    }
}

// ---------------------------------------------------------------------------
// Normalization idempotency: normalizing twice ≈ normalizing once
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))] // normalization is expensive

    #[test]
    fn normalization_idempotent(
        freq in 200.0f64..1000.0,
        amplitude in 0.05f64..0.8,
        duration_secs in 1.0f64..3.0,
    ) {
        let sample_rate = 44100u32;
        let channels = 1u16;
        let num_samples = (duration_secs * sample_rate as f64) as usize;

        // Generate a sine wave
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                (amplitude * (2.0 * std::f64::consts::PI * freq * t).sin()) as f32
            })
            .collect();

        let config = NormalizeConfig::default();

        // Normalize once
        let once = normalize(&samples, channels, sample_rate, &config)
            .expect("first normalization failed");

        // Normalize again
        let twice = normalize(&once, channels, sample_rate, &config)
            .expect("second normalization failed");

        // The two should be nearly identical (within a small tolerance)
        prop_assert_eq!(once.len(), twice.len());
        for (i, (a, b)) in once.iter().zip(twice.iter()).enumerate() {
            let diff = (a - b).abs();
            prop_assert!(
                diff < 0.02,
                "sample {} differs: once={} twice={} diff={}",
                i, a, b, diff
            );
        }
    }
}
