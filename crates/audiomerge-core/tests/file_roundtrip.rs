use audiomerge_core::testutil;
use audiomerge_core::types::OutputFormat;

#[test]
fn wav_encode_decode_roundtrip() {
    let spec = testutil::SignalSpec::default_mono();
    let original = testutil::generate_samples(&spec);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.wav");

    audiomerge_core::encode::encode(&original, 1, 44100, OutputFormat::Wav, &path).unwrap();
    let decoded = audiomerge_core::decode::decode(&path).unwrap();

    assert_eq!(decoded.channels, 1);
    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.samples.len(), original.len());

    // 16-bit quantization tolerance
    let tolerance = 1.0 / 32768.0 + 1e-6;
    testutil::assert_samples_approx_eq(&decoded.samples, &original, tolerance);
}

#[test]
fn wav_stereo_roundtrip() {
    let spec = testutil::SignalSpec::default_stereo();
    let original = testutil::generate_samples(&spec);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stereo_rt.wav");

    audiomerge_core::encode::encode(&original, 2, 44100, OutputFormat::Wav, &path).unwrap();
    let decoded = audiomerge_core::decode::decode(&path).unwrap();

    assert_eq!(decoded.channels, 2);
    assert_eq!(decoded.samples.len(), original.len());
}

#[test]
fn flac_encode_produces_nonzero_file() {
    let spec = testutil::SignalSpec::default_mono();
    let samples = testutil::generate_samples(&spec);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.flac");

    audiomerge_core::encode::encode(&samples, 1, 44100, OutputFormat::Flac, &path).unwrap();
    assert!(path.exists());
    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.len() > 100, "FLAC file suspiciously small: {} bytes", metadata.len());
}
