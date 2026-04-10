# audiomerge

A CLI tool that takes 2-3 audio files, normalizes their loudness (EBU R128), applies a true-peak limiter, and crossfades them into a single output file.

Ships as a single static binary with zero external runtime dependencies.

## Install

```bash
cargo build --release
# Binary is at target/release/audiomerge
```

## Usage

### Merge audio files

```bash
# Basic merge with defaults (equal-power crossfade, -14 LUFS, FLAC output)
audiomerge merge track1.flac track2.mp3 -o output.flac

# Three tracks with WAV output
audiomerge merge intro.wav main.flac outro.wav -o combined.wav

# Podcast / spoken word (broadcast loudness, longer crossfade)
audiomerge merge intro.wav interview.flac outro.wav \
  --lufs -23 --true-peak -1.0 --crossfade 1.5 --curve linear -o episode.flac

# Music (longer crossfade, equal-power curve)
audiomerge merge a.flac b.flac c.flac \
  --crossfade 5 --curve equal-power -o album_side.flac

# Verbose output to see processing stages
audiomerge merge track1.wav track2.wav -o out.flac -v
```

### Merge flags

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output` | `merged.flac` | Output file path |
| `--format` | inferred from `-o` ext, else `flac` | Output format: `flac`, `wav` |
| `--crossfade` | `1.0` | Crossfade duration in seconds |
| `--curve` | `equal-power` | Curve: `linear`, `equal-power`, `sin`, `cubic`, `exp` |
| `--lufs` | `-14.0` | Target integrated loudness (LUFS) |
| `--true-peak` | `-1.5` | True peak ceiling (dBFS) |
| `--lra` | `11.0` | Loudness range target (LU) |
| `-v, --verbose` | `false` | Print detailed processing info |

### Probe audio files

Inspect file metadata without processing:

```bash
audiomerge probe track1.flac track2.mp3 track3.wav
```

Output:

```
File                                       Duration     Rate     Ch      Codec
------------------------------------------------------------------------------
track1.flac                                  180.52s   44100       2       flac
track2.mp3                                   240.10s   48000       2        mp3
track3.wav                                    60.00s   44100       1        pcm
```

### Version

```bash
audiomerge version
```

## Supported formats

**Input:** WAV, FLAC, MP3, Vorbis/OGG (anything symphonia supports)

**Output:** FLAC (lossless, default), WAV (PCM 16-bit)

## How it works

1. **Probe** — read metadata from all input files
2. **Decode** — decode to raw f32 samples
3. **Resample** — convert all tracks to the highest sample rate among inputs
4. **Channel normalize** — upmix mono to stereo if needed
5. **Loudness normalize** — EBU R128 measurement, gain adjustment, true-peak limiting
6. **Crossfade** — overlap tracks with the selected curve
7. **Encode** — write the final output file

## Development

```bash
# Run all tests (75 total)
cargo test

# Run just the property tests
cargo test --test property_tests

# Run just the CLI smoke tests
cargo test -p audiomerge-cli
```

## Architecture

The project is split into two crates:

- **`audiomerge-core`** — all business logic as a reusable library
- **`audiomerge-cli`** — thin CLI shell over the core library

The core exposes a single entry point (`run(job, progress)`) making it easy to build alternative frontends (GUI, web server, etc.) without duplicating logic.

## License

TBD
