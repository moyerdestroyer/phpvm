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
// Versions / ls command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_ls_succeeds() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("ls")
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// ls-remote command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_ls_remote_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("ls-remote")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("available for install"));
}

// ---------------------------------------------------------------------------
// info command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_info_requires_version() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("info")
        .assert()
        .failure();
}

#[test]
fn phpvm_info_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("info")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("metadata"))
        .stdout(predicate::str::contains("PHP version"));
}

// ---------------------------------------------------------------------------
// use command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_use_without_pin_or_default_fails() {
    let home = tempfile::tempdir().unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .arg("use")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No PHP version specified"));
}

#[test]
fn phpvm_use_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("use")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("use"))
        .stdout(predicate::str::contains("fnm use"))
        .stdout(predicate::str::contains("wrapper"));
}

#[test]
fn phpvm_use_unknown_version_fails() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("use")
        .arg("99.99.99")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not installed"))
        .stderr(predicate::str::contains("phpvm install"));
}

#[test]
fn phpvm_env_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("env")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("shell integration"))
        .stdout(predicate::str::contains("fnm"));
}

#[test]
fn phpvm_env_wrapper_marks_use_invocations() {
    let home = tempfile::tempdir().unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::str::contains("__phpvm_bin="))
        .stdout(predicate::str::contains("use|deactivate"))
        .stdout(predicate::str::contains(
            r#"PHPVM_SHELL_INTEGRATION=1 "$__phpvm_bin""#,
        ));
}

#[test]
fn phpvm_deactivate_emits_path_cleanup() {
    let home = tempfile::tempdir().unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .arg("deactivate")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "unset PHPVM_VERSION COMPOSER_HOME PHPRC PHP_INI_SCAN_DIR",
        ))
        .stdout(predicate::str::contains("hash -r"));
}

#[test]
fn phpvm_deactivate_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("deactivate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Undo"));
}

#[test]
fn phpvm_env_emits_cd_hook_when_use_on_cd_enabled() {
    let home = tempfile::tempdir().unwrap();
    let config_dir = home.path().join("config.toml");
    std::fs::write(&config_dir, "use_on_cd = true\n").unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::str::contains("__phpvm_auto_use"));
}

#[test]
fn phpvm_env_omits_cd_hook_by_default() {
    let home = tempfile::tempdir().unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .arg("env")
        .assert()
        .success()
        .stdout(predicate::str::contains("__phpvm_auto_use").not());
}

#[test]
fn phpvm_env_fish_output() {
    let home = tempfile::tempdir().unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .arg("env")
        .arg("--shell")
        .arg("fish")
        .assert()
        .success()
        .stdout(predicate::str::contains("function phpvm"));
}

#[test]
fn phpvm_use_reads_phpvm_version_file() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join(".phpvm-version"), "8.3\n").unwrap();

    Command::cargo_bin("phpvm")
        .unwrap()
        .env("PHPVM_HOME", home.path())
        .current_dir(project.path())
        .arg("use")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not installed"))
        .stderr(predicate::str::contains("8.3"));
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
        .failure()
        .stdout(predicate::str::contains("RELEASE BLOCKED"));
}

#[test]
fn phpvm_release_check_json_in_empty_dir() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("release-check")
        .arg("--json")
        .assert()
        .failure()
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

#[test]
fn phpvm_matrix_rejects_unknown_report_format() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("matrix")
        .arg("--report")
        .arg("jsno")
        .arg("php")
        .arg("-v")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
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

#[test]
fn phpvm_profile_use_help() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("profile")
        .arg("use")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("profile"))
        .stdout(predicate::str::contains("--version"));
}

#[test]
fn phpvm_use_accepts_profile_flag() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("use")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--profile"));
}

// ---------------------------------------------------------------------------
// profile list command
// ---------------------------------------------------------------------------

#[test]
fn phpvm_profile_list_lists_builtins() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("profile")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("wordpress"))
        .stdout(predicate::str::contains("laravel"))
        .stdout(predicate::str::contains("minimal"));
}

#[test]
fn phpvm_profile_list_json_output() {
    Command::cargo_bin("phpvm")
        .unwrap()
        .arg("profile")
        .arg("list")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("\"source\""));
}
