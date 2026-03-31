use std::path::PathBuf;

/// Typed error enum for all audiomerge-core operations.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("unsupported codec '{codec}' in file: {path}")]
    UnsupportedCodec { path: PathBuf, codec: String },

    #[error("track too short: {path} is {duration_secs:.1}s but minimum is {min_secs:.1}s")]
    TrackTooShort {
        path: PathBuf,
        duration_secs: f64,
        min_secs: f64,
    },

    #[error("crossfade too long: {crossfade_secs:.1}s crossfade but shortest track is {shortest_track_secs:.1}s")]
    CrossfadeTooLong {
        crossfade_secs: f64,
        shortest_track_secs: f64,
    },

    #[error("decode failed for {path}: {source}")]
    DecodeFailed {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("encode failed for {path}: {source}")]
    EncodeFailed {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("resample failed: {source}")]
    ResampleFailed {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("too few tracks: got {count}, need at least 2")]
    TooFewTracks { count: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn display_file_not_found_contains_path() {
        let err = AudioError::FileNotFound(PathBuf::from("/tmp/missing.wav"));
        let msg = err.to_string();
        assert!(msg.contains("/tmp/missing.wav"), "got: {msg}");
    }

    #[test]
    fn display_unsupported_codec_contains_codec_and_path() {
        let err = AudioError::UnsupportedCodec {
            path: PathBuf::from("test.xyz"),
            codec: "xyz".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("xyz"), "got: {msg}");
        assert!(msg.contains("test.xyz"), "got: {msg}");
    }

    #[test]
    fn display_track_too_short_contains_durations() {
        let err = AudioError::TrackTooShort {
            path: PathBuf::from("short.wav"),
            duration_secs: 2.0,
            min_secs: 5.0,
        };
        let msg = err.to_string();
        assert!(msg.contains("2.0"), "got: {msg}");
        assert!(msg.contains("5.0"), "got: {msg}");
        assert!(msg.contains("short.wav"), "got: {msg}");
    }

    #[test]
    fn display_crossfade_too_long_contains_durations() {
        let err = AudioError::CrossfadeTooLong {
            crossfade_secs: 10.0,
            shortest_track_secs: 3.0,
        };
        let msg = err.to_string();
        assert!(msg.contains("10.0"), "got: {msg}");
        assert!(msg.contains("3.0"), "got: {msg}");
    }

    #[test]
    fn display_too_few_tracks_contains_count() {
        let err = AudioError::TooFewTracks { count: 1 };
        let msg = err.to_string();
        assert!(msg.contains("1"), "got: {msg}");
        assert!(msg.contains("at least 2"), "got: {msg}");
    }

    #[test]
    fn display_decode_failed_contains_path() {
        let err = AudioError::DecodeFailed {
            path: PathBuf::from("bad.mp3"),
            source: "corrupt header".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("bad.mp3"), "got: {msg}");
        assert!(msg.contains("corrupt header"), "got: {msg}");
    }

    #[test]
    fn display_encode_failed_contains_path() {
        let err = AudioError::EncodeFailed {
            path: PathBuf::from("out.flac"),
            source: "disk full".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("out.flac"), "got: {msg}");
        assert!(msg.contains("disk full"), "got: {msg}");
    }

    #[test]
    fn display_resample_failed_contains_source() {
        let err = AudioError::ResampleFailed {
            source: "invalid ratio".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid ratio"), "got: {msg}");
    }
}
