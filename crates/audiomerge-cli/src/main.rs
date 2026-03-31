use std::path::PathBuf;
use std::sync::mpsc;

use clap::{Parser, Subcommand, ValueEnum};

use audiomerge_core::probe::probe;
use audiomerge_core::types::{
    CrossfadeConfig, CurvePreset, Job, NormalizeConfig, OutputConfig, OutputFormat, ProgressEvent,
    TrackConfig,
};

#[derive(Parser)]
#[command(name = "audiomerge", version, about = "Loudness-normalized audio merging with crossfades")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Merge 2-3 audio files with loudness normalization and crossfades
    Merge {
        /// Input audio files (2 or 3)
        #[arg(required = true, num_args = 2..=3)]
        files: Vec<PathBuf>,

        /// Output file path
        #[arg(short, long, default_value = "merged.flac")]
        output: PathBuf,

        /// Output format: flac, wav (default: inferred from output extension, else flac)
        #[arg(long)]
        format: Option<FormatArg>,

        /// Crossfade duration in seconds
        #[arg(long, default_value = "1.0")]
        crossfade: f64,

        /// Crossfade curve shape
        #[arg(long, default_value = "equal-power")]
        curve: CurveArg,

        /// Target integrated loudness (LUFS)
        #[arg(long, default_value = "-14.0")]
        lufs: f64,

        /// True peak ceiling (dBFS)
        #[arg(long, default_value = "-1.5")]
        true_peak: f64,

        /// Loudness range target (LU)
        #[arg(long, default_value = "11.0")]
        lra: f64,

        /// Print detailed processing info
        #[arg(short, long)]
        verbose: bool,
    },
    /// Probe audio files and print metadata
    Probe {
        /// Audio files to probe
        #[arg(required = true, num_args = 1..)]
        files: Vec<PathBuf>,
    },
    /// Print version information
    Version,
}

#[derive(Clone, ValueEnum)]
enum FormatArg {
    Flac,
    Wav,
}

#[derive(Clone, ValueEnum)]
enum CurveArg {
    Linear,
    EqualPower,
    Sin,
    Cubic,
    Exp,
}

impl CurveArg {
    fn to_preset(&self) -> CurvePreset {
        match self {
            CurveArg::Linear => CurvePreset::Linear,
            CurveArg::EqualPower => CurvePreset::EqualPower,
            CurveArg::Sin => CurvePreset::Sinusoidal,
            CurveArg::Cubic => CurvePreset::Cubic,
            CurveArg::Exp => CurvePreset::Exponential,
        }
    }
}

fn infer_format(path: &PathBuf, explicit: Option<FormatArg>) -> OutputFormat {
    if let Some(fmt) = explicit {
        return match fmt {
            FormatArg::Flac => OutputFormat::Flac,
            FormatArg::Wav => OutputFormat::Wav,
        };
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("wav") => OutputFormat::Wav,
        _ => OutputFormat::Flac,
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Merge {
            files,
            output,
            format,
            crossfade,
            curve,
            lufs,
            true_peak,
            lra,
            verbose,
        } => {
            let fmt = infer_format(&output, format);
            let track_count = files.len();
            let output_path = output.clone();
            let job = Job {
                tracks: files.into_iter().map(|p| TrackConfig { path: p }).collect(),
                output: OutputConfig {
                    path: output,
                    format: fmt,
                },
                crossfade: CrossfadeConfig {
                    duration_secs: crossfade,
                    curve: curve.to_preset(),
                },
                normalize: NormalizeConfig {
                    target_lufs: lufs,
                    true_peak_dbfs: true_peak,
                    loudness_range: lra,
                },
            };
            let (tx, rx) = mpsc::channel::<ProgressEvent>();

            eprintln!(
                "Merging {} tracks → {}",
                track_count,
                output_path.display()
            );

            // Spawn a thread to print progress
            let progress_handle = std::thread::spawn(move || {
                use audiomerge_core::types::Stage;
                let mut last_stage = None;
                for event in rx {
                    if verbose {
                        eprintln!("  [{:?}] {}", event.stage, event.message);
                    } else {
                        // Show one line per stage transition (skip Done, handled below)
                        if event.stage != Stage::Done && last_stage != Some(event.stage) {
                            let label = match event.stage {
                                Stage::Probe => "Probing inputs...",
                                Stage::Decode => "Decoding tracks...",
                                Stage::Resample => "Resampling...",
                                Stage::Normalize => "Normalizing loudness...",
                                Stage::Crossfade => "Applying crossfades...",
                                Stage::Encode => "Encoding output...",
                                Stage::Done => unreachable!(),
                            };
                            eprintln!("  {label}");
                        }
                    }
                    last_stage = Some(event.stage);
                }
            });

            match audiomerge_core::run(job, Some(tx)) {
                Ok(()) => {
                    let _ = progress_handle.join();
                    eprintln!("Saved to {}", output_path.display());
                }
                Err(e) => {
                    let _ = progress_handle.join();
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Probe { files } => {
            println!(
                "{:<40} {:>10} {:>8} {:>6} {:>10}",
                "File", "Duration", "Rate", "Ch", "Codec"
            );
            println!("{}", "-".repeat(78));

            for path in &files {
                match probe(path) {
                    Ok(info) => {
                        println!(
                            "{:<40} {:>9.2}s {:>7} {:>5} {:>10}",
                            path.display(),
                            info.duration_secs,
                            info.sample_rate,
                            info.channels,
                            info.codec,
                        );
                    }
                    Err(e) => {
                        eprintln!("Error probing {}: {e}", path.display());
                    }
                }
            }
        }
        Commands::Version => {
            println!(
                "audiomerge {}",
                env!("CARGO_PKG_VERSION")
            );
        }
    }
}
