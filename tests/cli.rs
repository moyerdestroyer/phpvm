use assert_cmd::Command;
use predicates::prelude::*;

// ---------------------------------------------------------------------------
// Help and version
// ---------------------------------------------------------------------------

#[test]
fn phpvm_help_succeeds() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("PHP Compatibility Manager"));
}

#[test]
fn phpvm_version_succeeds() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("phpvm"));
}

// ---------------------------------------------------------------------------
// Subcommand help
// ---------------------------------------------------------------------------

#[test]
fn phpvm_install_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Install a PHP runtime"));
}

#[test]
fn phpvm_run_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("run")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Run a command"));
}

#[test]
fn phpvm_matrix_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("matrix")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("multiple PHP runtimes"));
}

#[test]
fn phpvm_doctor_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("doctor")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Inspect"));
}

#[test]
fn phpvm_release_check_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("release-check")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("compatibility"));
}

// ---------------------------------------------------------------------------
// Versions command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_versions_without_runtimes() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("versions")
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Doctor command (no project context)
// ---------------------------------------------------------------------------

#[test]
fn phpvm_doctor_in_empty_dir() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("doctor")
        .assert()
        .success();
}

#[test]
fn phpvm_doctor_json_in_empty_dir() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("doctor")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"project_type\""));
}

// ---------------------------------------------------------------------------
// Release-check command (no project context)
// ---------------------------------------------------------------------------

#[test]
fn phpvm_release_check_in_empty_dir() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("release-check")
        .assert()
        .success();
}

#[test]
fn phpvm_release_check_json_in_empty_dir() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("release-check")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"entries\""));
}

// ---------------------------------------------------------------------------
// Matrix command (requires a command argument)
// ---------------------------------------------------------------------------

#[test]
fn phpvm_matrix_requires_command() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("matrix")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Run command (requires version and command)
// ---------------------------------------------------------------------------

#[test]
fn phpvm_run_requires_version() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("run")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Install command (requires version)
// ---------------------------------------------------------------------------

#[test]
fn phpvm_install_requires_version() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("install")
        .assert()
        .failure();
}

#[test]
fn phpvm_install_accepts_profile_flag() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("install")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--profile"));
}

// ---------------------------------------------------------------------------
// Profiles command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_profiles_lists_builtins() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("profiles")
        .assert()
        .success()
        .stdout(predicate::str::contains("wordpress"))
        .stdout(predicate::str::contains("laravel"))
        .stdout(predicate::str::contains("minimal"));
}

#[test]
fn phpvm_profiles_json_output() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("profiles")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"extensions\""));
}
