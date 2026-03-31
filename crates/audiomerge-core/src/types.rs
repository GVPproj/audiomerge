use std::path::PathBuf;

/// Top-level input to the pipeline.
#[derive(Debug, Clone)]
pub struct Job {
    pub tracks: Vec<TrackConfig>,
    pub output: OutputConfig,
    pub crossfade: CrossfadeConfig,
    pub normalize: NormalizeConfig,
}

/// A single input file.
#[derive(Debug, Clone)]
pub struct TrackConfig {
    pub path: PathBuf,
}

/// Controls the output file.
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub path: PathBuf,
    pub format: OutputFormat,
}

/// Output format enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Flac,
    Wav,
}

/// EBU R128 normalization parameters.
#[derive(Debug, Clone, Copy)]
pub struct NormalizeConfig {
    /// Target integrated loudness in LUFS. Default: -14.0 (streaming-friendly; -23.0 for broadcast)
    pub target_lufs: f64,
    /// True peak ceiling in dBFS. Default: -1.5
    pub true_peak_dbfs: f64,
    /// Loudness range target in LU. Default: 11.0
    pub loudness_range: f64,
}

impl Default for NormalizeConfig {
    fn default() -> Self {
        Self {
            target_lufs: -14.0,
            true_peak_dbfs: -1.5,
            loudness_range: 11.0,
        }
    }
}

/// Crossfade parameters.
#[derive(Debug, Clone, Copy)]
pub struct CrossfadeConfig {
    /// Crossfade duration in seconds. Default: 1.0
    pub duration_secs: f64,
    /// Crossfade curve shape. Default: EqualPower
    pub curve: CurvePreset,
}

impl Default for CrossfadeConfig {
    fn default() -> Self {
        Self {
            duration_secs: 1.0,
            curve: CurvePreset::EqualPower,
        }
    }
}

/// Crossfade curve shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurvePreset {
    Linear,
    EqualPower,
    Sinusoidal,
    Cubic,
    Exponential,
}

/// Progress event sent from the pipeline to consumers.
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub stage: Stage,
    pub track: Option<usize>,
    pub message: String,
    pub percent: Option<f64>,
}

/// Pipeline processing stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Probe,
    Decode,
    Resample,
    Normalize,
    Crossfade,
    Encode,
    Done,
}

/// Metadata obtained from probing a single audio file.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub duration_secs: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub codec: String,
}
