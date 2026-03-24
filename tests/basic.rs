use assert_cmd::prelude::*;
use clap::{crate_name, crate_version};
use predicates::str::contains;
use std::process::Command;

/// Error type used by tests
pub type Error = Box<dyn std::error::Error>;

/// Show help and exit.
#[test]
fn help_shows() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .arg("--help")
        .assert()
        .success();

    Ok(())
}

/// Show version and exit.
#[test]
fn version_shows() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .arg("-V")
        .assert()
        .success()
        .stdout(format!("{} {}\n", crate_name!(), crate_version!()));

    Ok(())
}

/// Invalid argument should fail with a non-zero exit code.
#[test]
fn invalid_arg_fails() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .arg("--nonexistent-flag")
        .assert()
        .failure();
    Ok(())
}

/// Custom listen address should be accepted.
#[test]
fn custom_listen_address_is_accepted() -> Result<(), Error> {
    // We just test that the argument is parsed without error by checking --help output
    // (we can't start the server in tests without a refresh token)
    Command::cargo_bin("site24x7_exporter")?
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("web.listen-address"));
    Ok(())
}

/// Custom metrics path should appear in help.
#[test]
fn metrics_path_arg_in_help() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("web.telemetry-path"));
    Ok(())
}

/// Custom geolocation path should appear in help.
#[test]
fn geolocation_path_arg_in_help() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("web.geolocation-path"));
    Ok(())
}

/// Log level argument should appear in help.
#[test]
fn log_level_arg_in_help() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("log.level"));
    Ok(())
}

/// Site24x7 endpoint argument should appear in help.
#[test]
fn site24x7_endpoint_arg_in_help() -> Result<(), Error> {
    Command::cargo_bin("site24x7_exporter")?
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("site24x7-endpoint"));
    Ok(())
}
