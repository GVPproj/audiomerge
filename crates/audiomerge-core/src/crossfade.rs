use crate::types::CurvePreset;

/// Compute the fade-out gain for a given curve at position t (0.0 to 1.0).
pub fn fade_out(curve: CurvePreset, t: f64) -> f64 {
    match curve {
        CurvePreset::Linear => 1.0 - t,
        CurvePreset::EqualPower | CurvePreset::Sinusoidal => {
            (t * std::f64::consts::FRAC_PI_2).cos()
        }
        CurvePreset::Cubic => (1.0 - t).powi(3),
        CurvePreset::Exponential => (1.0 - t).powi(4),
    }
}

/// Compute the fade-in gain for a given curve at position t (0.0 to 1.0).
pub fn fade_in(curve: CurvePreset, t: f64) -> f64 {
    match curve {
        CurvePreset::Linear => t,
        CurvePreset::EqualPower | CurvePreset::Sinusoidal => {
            (t * std::f64::consts::FRAC_PI_2).sin()
        }
        CurvePreset::Cubic => t.powi(3),
        CurvePreset::Exponential => t.powi(4),
    }
}

/// Apply a crossfade between two interleaved sample buffers.
///
/// `overlap_frames` is the number of frames (not samples) to overlap.
/// Returns a new buffer of length: `len(a) + len(b) - overlap_frames * channels`.
pub fn crossfade(
    a: &[f32],
    b: &[f32],
    channels: u16,
    overlap_frames: usize,
    curve: CurvePreset,
) -> Vec<f32> {
    let ch = channels as usize;
    let a_frames = a.len() / ch;
    let b_frames = b.len() / ch;

    assert!(
        overlap_frames <= a_frames && overlap_frames <= b_frames,
        "overlap_frames ({overlap_frames}) exceeds track length (a={a_frames}, b={b_frames})"
    );

    let out_frames = a_frames + b_frames - overlap_frames;
    let mut out = vec![0.0f32; out_frames * ch];

    // Region 1: A before overlap (copy directly)
    let a_pre = a_frames - overlap_frames;
    out[..a_pre * ch].copy_from_slice(&a[..a_pre * ch]);

    // Region 2: Overlap
    for i in 0..overlap_frames {
        let t = if overlap_frames <= 1 {
            0.5
        } else {
            i as f64 / (overlap_frames - 1) as f64
        };
        let g_out = fade_out(curve, t) as f32;
        let g_in = fade_in(curve, t) as f32;

        for c in 0..ch {
            let a_sample = a[(a_pre + i) * ch + c];
            let b_sample = b[i * ch + c];
            out[(a_pre + i) * ch + c] = a_sample * g_out + b_sample * g_in;
        }
    }

    // Region 3: B after overlap (copy directly)
    let b_post_start = overlap_frames * ch;
    let out_post_start = (a_pre + overlap_frames) * ch;
    out[out_post_start..].copy_from_slice(&b[b_post_start..]);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    const ALL_CURVES: [CurvePreset; 5] = [
        CurvePreset::Linear,
        CurvePreset::EqualPower,
        CurvePreset::Sinusoidal,
        CurvePreset::Cubic,
        CurvePreset::Exponential,
    ];

    #[test]
    fn fade_out_at_zero_is_one_for_all_curves() {
        for curve in ALL_CURVES {
            let g = fade_out(curve, 0.0);
            assert!((g - 1.0).abs() < 1e-10, "{curve:?}: fade_out(0) = {g}");
        }
    }

    #[test]
    fn fade_out_at_one_is_zero_for_all_curves() {
        for curve in ALL_CURVES {
            let g = fade_out(curve, 1.0);
            assert!(g.abs() < 1e-10, "{curve:?}: fade_out(1) = {g}");
        }
    }

    #[test]
    fn fade_in_at_zero_is_zero_for_all_curves() {
        for curve in ALL_CURVES {
            let g = fade_in(curve, 0.0);
            assert!(g.abs() < 1e-10, "{curve:?}: fade_in(0) = {g}");
        }
    }

    #[test]
    fn fade_in_at_one_is_one_for_all_curves() {
        for curve in ALL_CURVES {
            let g = fade_in(curve, 1.0);
            assert!((g - 1.0).abs() < 1e-10, "{curve:?}: fade_in(1) = {g}");
        }
    }

    #[test]
    fn equal_power_sums_to_one_at_midpoint() {
        let g_out = fade_out(CurvePreset::EqualPower, 0.5);
        let g_in = fade_in(CurvePreset::EqualPower, 0.5);
        // For equal-power: cos²(π/4) + sin²(π/4) = 1.0
        let sum_sq = g_out * g_out + g_in * g_in;
        assert!(
            (sum_sq - 1.0).abs() < 1e-10,
            "equal power sum of squares at midpoint: {sum_sq}"
        );
    }

    #[test]
    fn all_curves_monotonic() {
        for curve in ALL_CURVES {
            let steps = 100;
            let mut prev_out = fade_out(curve, 0.0);
            let mut prev_in = fade_in(curve, 0.0);
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                let g_out = fade_out(curve, t);
                let g_in = fade_in(curve, t);
                assert!(
                    g_out <= prev_out + 1e-10,
                    "{curve:?}: fade_out not monotonically decreasing at t={t}"
                );
                assert!(
                    g_in >= prev_in - 1e-10,
                    "{curve:?}: fade_in not monotonically increasing at t={t}"
                );
                prev_out = g_out;
                prev_in = g_in;
            }
        }
    }

    #[test]
    fn output_length_correct() {
        let a = testutil::generate_samples(&testutil::SignalSpec::default_mono());
        let b = testutil::generate_samples(&testutil::SignalSpec::default_mono());
        let overlap = 4410; // 0.1s at 44100
        let result = crossfade(&a, &b, 1, overlap, CurvePreset::Linear);
        assert_eq!(result.len(), a.len() + b.len() - overlap);
    }

    #[test]
    fn zero_overlap_is_concatenation() {
        let a = vec![1.0f32, 2.0, 3.0];
        let b = vec![4.0f32, 5.0, 6.0];
        let result = crossfade(&a, &b, 1, 0, CurvePreset::Linear);
        assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn non_overlapping_regions_are_identical() {
        let a = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let b = vec![6.0f32, 7.0, 8.0, 9.0, 10.0];
        let overlap = 2;
        let result = crossfade(&a, &b, 1, overlap, CurvePreset::Linear);
        // A's non-overlap region: first 3 samples
        assert_eq!(&result[..3], &[1.0, 2.0, 3.0]);
        // B's non-overlap region: last 3 samples
        assert_eq!(&result[result.len() - 3..], &[8.0, 9.0, 10.0]);
    }

    #[test]
    fn stereo_interleaving_preserved() {
        // A: 4 frames stereo = 8 samples
        let a = vec![1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0];
        // B: 4 frames stereo = 8 samples
        let b = vec![5.0, -5.0, 6.0, -6.0, 7.0, -7.0, 8.0, -8.0];
        let result = crossfade(&a, &b, 2, 2, CurvePreset::Linear);
        // Output: 4 + 4 - 2 = 6 frames = 12 samples
        assert_eq!(result.len(), 12);
        // Non-overlap region of A (frames 0-1)
        assert_eq!(&result[..4], &[1.0, -1.0, 2.0, -2.0]);
        // Non-overlap region of B (frames 4-5)
        assert_eq!(&result[8..], &[7.0, -7.0, 8.0, -8.0]);
    }

    #[test]
    fn crossfade_output_bounded_in_overlap() {
        let a = vec![0.5f32; 1000];
        let b = vec![0.5f32; 1000];
        let result = crossfade(&a, &b, 1, 500, CurvePreset::EqualPower);
        let p = testutil::peak(&result);
        // Overlap samples should not exceed sum of input amplitudes
        assert!(p <= 1.0 + 1e-6, "peak {p} exceeds sum of amplitudes");
    }
}
