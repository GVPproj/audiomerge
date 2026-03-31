use std::process::Command;

fn audiomerge_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_audiomerge"))
}

#[test]
fn cli_runs_without_crash() {
    let output = audiomerge_bin().output().expect("failed to run audiomerge");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Without a subcommand, clap prints usage to stderr
    assert!(
        !stdout.is_empty() || !stderr.is_empty(),
        "no output from audiomerge"
    );
}

#[test]
fn cli_version_subcommand() {
    let output = audiomerge_bin()
        .arg("version")
        .output()
        .expect("failed to run audiomerge version");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("audiomerge"), "version output should contain 'audiomerge'");
}

#[test]
fn cli_merge_too_few_files_errors() {
    let output = audiomerge_bin()
        .args(["merge", "one.wav"])
        .output()
        .expect("failed to run audiomerge merge");
    // clap should reject <2 files
    assert!(!output.status.success());
}

#[test]
fn cli_probe_nonexistent_file() {
    let output = audiomerge_bin()
        .args(["probe", "nonexistent.wav"])
        .output()
        .expect("failed to run audiomerge probe");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error") || stderr.contains("error"),
        "should report error for nonexistent file"
    );
}
