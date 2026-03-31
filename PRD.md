# audiomerge — Product Requirements Document

**Version:** 0.2
**Status:** Draft
**Stack:** Rust (zero external runtime dependencies)

---

## Implementation Progress

| Component | Status | Notes |
|-----------|--------|-------|
| Cargo workspace & crate structure | DONE | Workspace with `audiomerge-core` and `audiomerge-cli` |
| `types.rs` — Job, configs, enums | DONE | All types from Section 5 implemented with Default impls |
| `error.rs` — AudioError enum | DONE | All variants with Display; 8 unit tests passing |
| `testutil.rs` — test helpers | DONE | SignalSpec, generate_samples, write_wav, rms/peak/loudness measurement; 14 unit tests |
| `crossfade.rs` — curve functions + crossfade engine | DONE | All 5 curve presets, N-input crossfade; 10 unit tests |
| `normalize.rs` — EBU R128 + true-peak limiter | DONE | ebur128-based measurement, gain + limiter; 6 unit tests |
| `resample.rs` — sample rate conversion + upmix | DONE | rubato-based sinc resampling, mono→stereo upmix; 5 unit tests |
| `probe.rs` — file metadata extraction | DONE | symphonia-based header reading; 3 unit tests |
| `decode.rs` — full audio decoding | DONE | symphonia decode to f32 interleaved; 3 unit tests |
| `encode.rs` — WAV + FLAC output | DONE | hound (WAV) + flacenc (FLAC) encoding; 4 unit tests |
| `pipeline.rs` — full orchestration | DONE | probe→decode→resample→normalize→crossfade→encode; 4 unit tests |
| Integration tests — full_pipeline | DONE | 4 tests: duration, loudness, mono+stereo, FLAC output |
| Integration tests — file_roundtrip | DONE | 3 tests: WAV roundtrip, stereo roundtrip, FLAC encode |
| Integration tests — cli_smoke | DONE | 4 tests: runs without crash, version, merge validation, probe error |
| CLI (`main.rs`) — clap interface | DONE | merge, probe, version subcommands; all flags per Section 7; 4 smoke tests |
| Property tests (proptest) | DONE | 6 tests: crossfade length invariant, bounded overlap, fade boundaries/monotonicity, normalization idempotency |

**Total tests: 75 passing** (58 unit + 7 integration + 6 property + 4 CLI smoke)

---

## 1. Overview

`audiomerge` is a CLI tool that takes 2–3 audio files, normalizes their loudness, applies a true-peak limiter, and crossfades them into a single output file. It is designed as a thin CLI binary over a reusable Rust library crate (`audiomerge-core`) that can later be consumed by a GUI frontend (Tauri or web-based).

All audio decoding, resampling, loudness measurement, crossfading, and encoding are handled by native Rust crates — no ffmpeg or any other external runtime dependency is required.

---

## 2. Goals

- Produce a perceptually consistent merged audio file from heterogeneous sources
- Support broadcast-grade EBU R128 loudness normalization
- Apply a true-peak limiter post-normalization to prevent clipping
- Crossfade tracks with configurable duration and curve shapes
- Output to FLAC or WAV
- Keep all business logic in `audiomerge-core` — the CLI is just a consumer
- Ship as a single static binary with zero runtime dependencies

---

## 3. Non-Goals (v0.1)

- No built-in audio playback
- No GUI (deferred; architecture must support it)
- No MP3 or other lossy output formats
- No EQ, reverb, or other creative DSP
- No cloud/remote file support

---

## 4. Architecture

```
audiomerge/
├── Cargo.toml               # Workspace root
├── crates/
│   ├── audiomerge-core/      # Library crate — all business logic
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs        # Public API: run(job, progress_tx) -> Result<()>
│   │       ├── types.rs      # Job, TrackConfig, OutputFormat, CurvePreset
│   │       ├── probe.rs      # Decode headers — duration, sample rate, channels
│   │       ├── decode.rs     # Decode full audio to f32 samples via symphonia
│   │       ├── normalize.rs  # EBU R128 loudness normalization + true-peak limiter
│   │       ├── resample.rs   # Sample rate conversion via rubato
│   │       ├── crossfade.rs  # Crossfade engine for N inputs
│   │       ├── encode.rs     # FLAC/WAV encoder
│   │       ├── pipeline.rs   # Orchestrates probe → decode → normalize → crossfade → encode
│   │       └── error.rs      # Typed error enum
│   └── audiomerge-cli/       # Binary crate — thin CLI shell
│       ├── Cargo.toml
│       └── src/
│           └── main.rs       # clap CLI entrypoint — no business logic
└── tests/
    └── integration/          # Integration tests with real audio processing
```

### Key design principle

`audiomerge-core` exposes a single entry point:

```rust
pub fn run(job: Job, progress: Option<Sender<ProgressEvent>>) -> Result<(), AudioError>;
```

The CLI constructs a `Job` and calls it. A future Tauri GUI, web server, or another binary does the same — no business logic leaks into the CLI crate.

---

## 5. Core Data Types (`audiomerge-core/src/types.rs`)

```rust
/// Top-level input to the pipeline.
pub struct Job {
    pub tracks: Vec<TrackConfig>,
    pub output: OutputConfig,
    pub crossfade: CrossfadeConfig,
    pub normalize: NormalizeConfig,
}

/// A single input file.
pub struct TrackConfig {
    pub path: PathBuf,
    // Future: per-track gain trim, label, etc.
}

/// Controls the output file.
pub struct OutputConfig {
    pub path: PathBuf,
    pub format: OutputFormat, // FLAC | WAV
}

/// Output format enum.
pub enum OutputFormat {
    Flac,
    Wav,
}

/// EBU R128 normalization parameters.
pub struct NormalizeConfig {
    pub target_lufs: f64,      // default: -14.0 (streaming-friendly; -23.0 for broadcast)
    pub true_peak_dbfs: f64,   // default: -1.5 (headroom before clipping)
    pub loudness_range: f64,   // default: 11.0 (LRA, dynamic range target)
}

/// Crossfade parameters.
pub struct CrossfadeConfig {
    pub duration_secs: f64,    // default: 1.0
    pub curve: CurvePreset,    // default: EqualPower
}

/// Crossfade curve shapes.
pub enum CurvePreset {
    Linear,      // linear ramp
    EqualPower,  // equal-power (natural for music)
    Sinusoidal,  // sine-based
    Cubic,       // cubic, slower fade
    Exponential, // exponential, sharp
}

/// Derived from probing all inputs. Used internally by the pipeline.
pub(crate) struct ResolvedPipeline {
    pub target_sample_rate: u32, // max of all input sample rates
    pub target_channels: u16,    // max of all input channel counts
}
```

---

## 6. Pipeline Stages

All processing happens in-memory on `Vec<f32>` sample buffers (interleaved). No temp files are needed — this is a significant simplification over the ffmpeg-based design.

### Stage 1 — Probe

Use `symphonia` to read codec headers from each input file without fully decoding:

- Duration (seconds)
- Sample rate (Hz)
- Channel count
- Codec name

After probing all inputs, populate `ResolvedPipeline`:
- `target_sample_rate` — the highest sample rate among all inputs (avoids lossy downsampling)
- `target_channels` — the highest channel count among all inputs (e.g. if one track is mono and two are stereo, target is stereo)

Fail fast if any file is unreadable or below minimum duration (default: 5s). Emit a warning if crossfade duration >= track duration.

### Stage 2 — Decode

Fully decode each track to `Vec<f32>` interleaved samples using `symphonia`. Supported input codecs: FLAC, WAV/PCM, MP3, Vorbis, and anything else symphonia supports.

Decoding can run concurrently per track via `rayon` or tokio tasks.

### Stage 3 — Resample + Channel Normalization

For each decoded track, if its sample rate or channel count differs from the resolved target:

- **Resample** via `rubato` (sinc interpolation) to `target_sample_rate`
- **Upmix** mono → stereo by duplicating the channel, or downmix if needed

This runs per-track and is parallelizable.

### Stage 4 — Normalize (per-track, parallelizable)

Using the `ebur128` crate (Rust bindings to libebur128, compiled and statically linked):

1. **Measure** — feed all samples through an `EbuR128` instance to get integrated loudness (LUFS) and true peak
2. **Apply gain** — compute the linear gain needed to hit `target_lufs` and apply it sample-by-sample
3. **True-peak limit** — clamp any samples that exceed `true_peak_dbfs` using a lookahead limiter to avoid hard clipping artifacts

Tracks can be normalized concurrently since they're independent buffers.

### Stage 5 — Crossfade + Encode

With all tracks normalized to the same loudness, sample rate, and channel layout, apply crossfades:

For 3 tracks (A, B, C) with crossfade duration `d`:
1. Overlap the last `d` seconds of A with the first `d` seconds of B, applying the selected curve
2. Overlap the last `d` seconds of the A+B result with the first `d` seconds of C

The crossfade is pure sample math — for each overlapping sample pair, apply the curve function to compute fade-out and fade-in gains, then sum.

**Curve functions** (where `t` goes from 0.0 to 1.0 across the crossfade):

| Curve | Fade-out gain | Fade-in gain |
|-------|--------------|--------------|
| Linear | `1 - t` | `t` |
| EqualPower | `cos(t * π/2)` | `sin(t * π/2)` |
| Sinusoidal | `cos(t * π/2)` | `sin(t * π/2)` |
| Cubic | `(1 - t)³` | `t³` |
| Exponential | `(1 - t)^4` | `t^4` |

Encode the final sample buffer directly to the output file:

| Format | Crate | Notes |
|--------|-------|-------|
| FLAC | `flacenc` or `hound` with FLAC support | Lossless, recommended default |
| WAV | `hound` | PCM 16-bit or 24-bit, uncompressed |

---

## 7. CLI Interface

### Command: `audiomerge merge`

```
audiomerge merge [flags] <file1> <file2> [file3]
```

**Required:**
- `<file1> <file2> [file3]` — Two or three input audio files (FLAC, WAV, MP3, or any format symphonia supports)

**Flags:**

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output` | `merged.flac` | Output file path |
| `--format` | inferred from `-o` ext, else `flac` | Output format: `flac`, `wav` |
| `--crossfade` | `1.0` | Crossfade duration in seconds |
| `--curve` | `equal-power` | Crossfade curve: `linear`, `equal-power`, `sin`, `cubic`, `exp` |
| `--lufs` | `-14.0` | Target integrated loudness (LUFS) (streaming-friendly; use -23 for broadcast) |
| `--true-peak` | `-1.5` | True peak ceiling (dBFS) |
| `--lra` | `11.0` | Loudness range target (LU) |
| `-v, --verbose` | `false` | Print detailed processing info |

**Example usage:**

```bash
# Basic merge with defaults
audiomerge merge track1.flac track2.mp3 track3.wav -o final.flac

# Podcast/spoken word
audiomerge merge intro.wav interview.flac outro.wav \
  --lufs -23 --true-peak -1.0 --crossfade 1.5 --curve linear -o episode.flac

# Music, equal-power crossfade, lossless output
audiomerge merge a.flac b.flac c.flac \
  --crossfade 5 --curve equal-power -o album_side.flac
```

### Command: `audiomerge probe`

```
audiomerge probe <file> [file2 ...]
```

Reads headers from each file and prints a summary table (duration, codec, sample rate, channels). Useful for sanity-checking inputs before a merge.

### Command: `audiomerge version`

Prints `audiomerge` version and build info.

---

## 8. Error Handling

All errors use a typed enum (`AudioError`) so consumers can pattern-match:

```rust
pub enum AudioError {
    FileNotFound(PathBuf),
    UnsupportedCodec { path: PathBuf, codec: String },
    TrackTooShort { path: PathBuf, duration_secs: f64, min_secs: f64 },
    CrossfadeTooLong { crossfade_secs: f64, shortest_track_secs: f64 },
    DecodeFailed { path: PathBuf, source: Box<dyn std::error::Error + Send + Sync> },
    EncodeFailed { path: PathBuf, source: Box<dyn std::error::Error + Send + Sync> },
    ResampleFailed { source: Box<dyn std::error::Error + Send + Sync> },
    TooFewTracks { count: usize },
}
```

- Unreadable input file → `FileNotFound`, fail fast before any processing
- Crossfade duration >= shortest track duration → `CrossfadeTooLong` with suggestion
- Mismatched sample rates or channel layouts → handled automatically via resample/upmix; logged as info
- All errors implement `std::error::Error` and `Display` for human-readable messages

---

## 9. Progress Reporting

The pipeline sends progress events via an `mpsc::Sender<ProgressEvent>`:

```rust
pub struct ProgressEvent {
    pub stage: Stage,
    pub track: Option<usize>, // which track (for decode/normalize)
    pub message: String,
    pub percent: Option<f64>, // 0.0–1.0 where estimable
}

pub enum Stage {
    Probe,
    Decode,
    Resample,
    Normalize,
    Crossfade,
    Encode,
    Done,
}
```

The CLI subscribes and renders simple log lines. A future GUI receives the same events for a progress bar. The `progress` parameter is `Option<Sender<...>>` — passing `None` disables reporting with no overhead.

---

## 10. Testing Strategy

No external tools or fixture files needed — all audio is generated in code and everything runs with `cargo test`.

### Test Helper Module (`audiomerge-core/src/testutil.rs`)

A shared module gated behind `#[cfg(any(test, feature = "test-helpers"))]` that provides synthetic audio generation and verification utilities. The `test-helpers` feature allows integration tests outside the crate to use it.

**Signal generation:**

```rust
/// Configuration for generating a synthetic audio signal.
pub struct SignalSpec {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
    pub components: Vec<SignalComponent>,  // frequency + amplitude + phase
}
```

Convenience constructors:
- `SignalSpec::default_mono()` — 440 Hz, 44100 Hz, 1ch, 3s, amplitude 0.5
- `SignalSpec::default_stereo()` — same but 2ch
- `SignalSpec::silence(rate, channels, duration)` — all-zero buffer
- `SignalSpec::loud_tone(freq, rate, duration)` — amplitude 0.95 (test normalization down)
- `SignalSpec::quiet_tone(freq, rate, duration)` — amplitude 0.01 (test normalization up)
- `SignalSpec::composite(freqs, rate, channels, duration)` — multi-frequency signal

**Buffer generation (no I/O):**
- `generate_samples(spec) -> Vec<f32>` — interleaved f32 samples via `amplitude * sin(2π * freq * t)`
- `generate_silence(rate, channels, duration) -> Vec<f32>`

**WAV file generation (for end-to-end tests):**
- `write_wav(spec, path)` — writes 16-bit WAV via `hound` (already a dependency)
- `write_wav_temp(spec, filename) -> (TempDir, PathBuf)` — writes to a temp directory

**Verification helpers:**
- `rms(samples) -> f32` — root mean square amplitude
- `peak(samples) -> f32` — peak absolute amplitude
- `frame_count(samples, channels) -> usize`
- `duration_secs(samples, channels, rate) -> f64`
- `assert_samples_approx_eq(a, b, tolerance)` — per-sample comparison
- `measure_loudness_lufs(samples, channels, rate) -> f64` — uses `ebur128` crate

### Test Organization

```
crates/audiomerge-core/
  src/
    testutil.rs              # Shared test helper module
    crossfade.rs             # #[cfg(test)] mod tests { ... }
    normalize.rs             # #[cfg(test)] mod tests { ... }
    resample.rs              # #[cfg(test)] mod tests { ... }
    encode.rs                # #[cfg(test)] mod tests { ... }
    probe.rs                 # #[cfg(test)] mod tests { ... }
    error.rs                 # #[cfg(test)] mod tests { ... }
tests/
  integration/
    full_pipeline.rs         # End-to-end through run()
    file_roundtrip.rs        # Encode → decode → verify
    cli_smoke.rs             # Binary invocation tests
```

### Test Tiers

**Unit tests** (in-module `#[cfg(test)]` blocks):

| Module | Key test cases |
|--------|---------------|
| `crossfade.rs` | Curve boundary values (fade_out(0)=1, fade_in(1)=1); equal-power gains sum to 1.0 at midpoint; all curves monotonic; output length = len(A) + len(B) - overlap; zero overlap = concatenation; non-overlapping regions bit-identical; stereo interleaving preserved |
| `normalize.rs` | Loud signal attenuated to target LUFS (within 0.5 LU); quiet signal amplified; true-peak ceiling respected; silence stays silent; channel count preserved |
| `resample.rs` | Duration preserved after resampling; same-rate is passthrough; mono→stereo upmix duplicates channels |
| `encode.rs` | WAV round-trip: samples match within 1/32768 (16-bit quantization); FLAC round-trip same tolerance; header metadata correct |
| `probe.rs` | Correct sample rate, channels, duration from generated WAVs; nonexistent file → `FileNotFound` error |
| `error.rs` | Display messages contain relevant data (paths, counts, durations) |

**Integration tests** (`tests/integration/`):
- Two-track merge: output duration = sum - crossfade overlap
- Three tracks at different amplitudes: output loudness near target LUFS
- Mismatched sample rates: pipeline resamples to highest
- Mono + stereo input: output is stereo
- FLAC output: valid file produced
- WAV and FLAC encode→decode round-trips
- CLI smoke tests: merge succeeds, probe prints info, too few files errors, nonexistent file errors

**Property tests** (via `proptest` crate):
- Crossfade output length invariant: `len(A) + len(B) - overlap` for arbitrary buffer sizes
- Crossfade output bounded: overlap region samples ≤ sum of input amplitudes
- Normalization idempotent: normalizing twice ≈ normalizing once

### Verifying Audio Correctness

1. **Sample-level comparison** — for lossless round-trips, compare every sample within quantization tolerance (1/32768 for 16-bit, 1/8388608 for 24-bit)
2. **Loudness measurement** — use `ebur128` to measure integrated loudness of output, assert within 0.5 LU of target
3. **Duration arithmetic** — frame count = `samples.len() / channels`, duration = `frames / sample_rate` (exact integer math)
4. **Header checks** — re-read encoded file headers to verify sample rate, channels, bit depth
5. **Signal-domain checks** — non-overlapping crossfade regions are bit-identical to input; overlap region RMS is between input RMS values

### Test Dependencies

| Crate | Purpose |
|-------|---------|
| `proptest` | Property-based testing (dev-dependency) |
| `tempfile` | Temp directories for WAV files (dev-dependency + optional for `test-helpers` feature) |
| `approx` | Floating-point comparison macros (dev-dependency) |

---

## 11. Dependencies

| Crate | Purpose | Notes |
|-------|---------|-------|
| `symphonia` | Audio decoding (FLAC, WAV, MP3, Vorbis) | Pure Rust, no system libs |
| `rubato` | Sample rate conversion | High-quality sinc resampling |
| `ebur128` | EBU R128 loudness measurement | Rust bindings, statically compiled |
| `hound` | WAV encoding | Pure Rust, lightweight |
| `flacenc` | FLAC encoding | Pure Rust |
| `clap` | CLI argument parsing | derive-based, well-established |
| `rayon` | Parallel per-track processing | Work-stealing thread pool |
| `thiserror` | Typed error derivation | Zero-cost derive macro |

**External runtime dependencies: none.** Everything compiles into a single static binary.

---

## 12. GUI Readiness Checklist

When a GUI is added later, the following must hold true (and are satisfied by this design):

- [ ] `audiomerge-core` has no `println!`, `eprintln!`, or `std::process::exit` — all output via return values and channels
- [ ] `Job` and all config types are plain structs — `serde::Serialize`/`Deserialize` derivable
- [ ] `ProgressEvent` sender is passed into `run()` — GUI provides its own receiver
- [ ] All errors are typed via `AudioError` enum — GUI can show user-friendly messages per variant
- [ ] No global mutable state in `audiomerge-core`
- [ ] `audiomerge-cli` is the only crate that touches `std::env::args`, `stdout`, or `process::exit`

**Recommended GUI path:** Tauri v2 (Rust backend + React or Svelte frontend). The Tauri app imports `audiomerge-core` directly and exposes `run()` as a Tauri command — no IPC serialization beyond what Tauri provides.

---

## 13. Future Considerations (out of scope for v0.1)

- Per-track trim/gain offset in `TrackConfig`
- Fade-in on first track / fade-out on last track
- `--preset` flag bundling common LUFS/LRA/crossfade combinations (e.g. `podcast`, `music`, `broadcast`)
- Watch mode: re-merge when source files change
- MP3 output (requires statically linking lame or a pure-Rust encoder)
- Web server mode: expose `audiomerge-core` as an HTTP API
- Waveform preview output (render samples to PNG)
