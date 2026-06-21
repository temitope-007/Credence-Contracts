//! Integration tests for the `credence-admin` CLI.
//!
//! These drive the built binary directly via `std::process::Command` (no extra
//! test-harness crates) and assert the dry-run behaviour of each subcommand
//! against the real clap interface (positional arguments).

use std::process::Command;

/// Path to the compiled `credence-admin` binary, provided by Cargo to
/// integration tests.
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_credence-admin")
}

#[test]
fn bond_set_early_exit_dry_run() {
    let output = Command::new(bin())
        .args(["bond-set-early-exit-config", "test-bond", "500"])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        output.status.success(),
        "expected success, got status {:?}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Dry run") && stdout.contains("set-early-exit-config"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn bond_set_weights_dry_run() {
    let output = Command::new(bin())
        .args(["bond-set-weights", "test-bond", "10"])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        output.status.success(),
        "expected success, got status {:?}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Dry run") && stdout.contains("set-weights"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn submit_is_not_yet_implemented() {
    // `--submit` is a global flag and must precede the subcommand.
    let output = Command::new(bin())
        .args(["--submit", "bond-set-weights", "test-bond", "10"])
        .output()
        .expect("failed to run credence-admin");

    assert!(
        !output.status.success(),
        "expected failure for unimplemented submit path"
    );
}
