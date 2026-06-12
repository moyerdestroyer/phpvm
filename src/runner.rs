use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;

use crate::config;
use crate::output;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a modified PATH with the runtime's `bin/` and its (minor-shared)
/// global Composer vendor/bin prepended (for tools installed via `composer global`).
///
/// Globals are shared across patch versions of the same minor series
/// (all 8.3.x use the same `~/.phpvm/composer-homes/8.3/` bucket).
/// Also ensures the composer home directory exists.
fn build_runtime_path(runtime_path: &Utf8PathBuf, resolved: &str) -> Result<String> {
    let bin_dir = runtime_path.join("bin");
    let composer_home = crate::version::composer_home_for(resolved)?;
    // Best effort: make sure it exists for global installs.
    let _ = std::fs::create_dir_all(&composer_home);
    let global_bin = composer_home.join("vendor").join("bin");

    let current_path = std::env::var("PATH").context("No PATH environment variable found")?;
    let separator = if cfg!(windows) { ";" } else { ":" };
    Ok(format!(
        "{}{}{}{}{}",
        bin_dir, separator, global_bin, separator, current_path
    ))
}

/// Execute a command subprocess and check its exit status.
///
/// For `output`-style (captured), the captured stdout/stderr are discarded
/// on success (matrix mode doesn't display them).
fn check_exit_status(exit_status: std::process::ExitStatus) -> Result<()> {
    if !exit_status.success() {
        if let Some(code) = exit_status.code() {
            bail!("Command exited with status {}", code);
        } else {
            bail!("Command terminated by signal");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Collect the list of installed runtime version strings from disk.
pub fn installed_versions() -> Result<Vec<String>> {
    let runtimes_dir = config::runtimes_dir()?;
    if !runtimes_dir.exists() {
        return Ok(Vec::new());
    }
    let versions = std::fs::read_dir(&runtimes_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    Ok(versions)
}

/// Run a command against a specific PHP runtime, streaming output to the terminal.
pub fn run(version: &str, command: &[String]) -> Result<()> {
    if command.is_empty() {
        bail!("run requires a command (e.g. `phpvm run 8.3 php -v`)");
    }

    let resolved = resolve_runtime(version)?;
    let runtime_path = require_runtime(&resolved, version)?;

    output::info(&format!("Running with PHP {}", resolved));

    let new_path = build_runtime_path(&runtime_path, &resolved)?;
    let composer_home = crate::version::composer_home_for(&resolved)?;

    let (program, args) = command
        .split_first()
        .context("run requires a non-empty command")?;

    let status = Command::new(program)
        .args(args)
        .env("PATH", &new_path)
        .env("COMPOSER_HOME", composer_home.as_str())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to execute command")?
        .wait()
        .context("Failed to execute command")?;

    check_exit_status(status)
}

/// Run a command silently (for matrix use). Returns `Ok(())` on success,
/// `Err` on failure. Stdout and stderr are captured, not displayed.
pub fn run_silent(version: &str, command: &[String]) -> Result<()> {
    let resolved = resolve_runtime(version)?;
    require_runtime(&resolved, version)?;

    let runtime_path = config::runtimes_dir()?.join(&resolved);
    let new_path = build_runtime_path(&runtime_path, &resolved)?;
    let composer_home = crate::version::composer_home_for(&resolved)?;

    let (program, args) = command
        .split_first()
        .context("run_silent requires a non-empty command")?;

    let output = Command::new(program)
        .args(args)
        .env("PATH", &new_path)
        .env("COMPOSER_HOME", composer_home.as_str())
        .output()
        .context("Failed to execute command")?;

    check_exit_status(output.status)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve a version specifier against installed runtimes.
pub fn resolve_version(version: &str) -> Result<String> {
    let available = installed_versions()?;
    crate::version::resolve_specifier(version, &available)
}

fn resolve_runtime(version: &str) -> Result<String> {
    resolve_version(version)
}

/// Verify the runtime directory exists at `~/.phpvm/runtimes/{resolved}/`.
/// Returns the runtime directory path on success.
fn require_runtime(resolved: &str, original: &str) -> Result<Utf8PathBuf> {
    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(resolved);

    if !runtime_path.exists() {
        bail!(
            "PHP runtime {} is not installed. Run `phpvm install {}` first.",
            resolved,
            original
        );
    }

    Ok(runtime_path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_versions_returns_vec() {
        let versions = installed_versions();
        // May be empty in CI, but should never error.
        assert!(versions.is_ok());
    }

    #[test]
    fn build_runtime_path_includes_bin_and_globals() {
        // Make data_dir() return a deterministic fake base so that
        // composer_home_for produces the expected minor bucket path.
        std::env::set_var("PHPVM_HOME", "/home/user/.phpvm");
        // Temporarily override PATH for deterministic test.
        std::env::set_var("PATH", "/usr/bin:/bin");
        let runtime = Utf8PathBuf::from("/home/user/.phpvm/runtimes/8.3.12");
        // Pass the resolved version so we can compute the minor-shared globals dir (8.3)
        let result = build_runtime_path(&runtime, "8.3.12").unwrap();
        // Should start with the runtime's own (per-patch) bin
        assert!(result.starts_with("/home/user/.phpvm/runtimes/8.3.12/bin:"));
        // Globals are now shared per-minor under a top-level composer-homes dir
        assert!(result.contains("/home/user/.phpvm/composer-homes/8.3/vendor/bin:"));
        assert!(result.ends_with(":/usr/bin:/bin") || result.contains(":/usr/bin:/bin"));
    }

    #[test]
    fn check_exit_status_success() {
        // Simulate a successful exit status (code 0).
        let output = Command::new("true").output();
        if let Ok(out) = output {
            assert!(check_exit_status(out.status).is_ok());
        }
    }

    #[test]
    fn check_exit_status_failure() {
        // Simulate a failing exit status (code 1).
        let output = Command::new("false").output();
        if let Ok(out) = output {
            let result = check_exit_status(out.status);
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Command exited with status"));
        }
    }

    #[test]
    fn run_rejects_empty_command() {
        let result = run("8.3", &[]);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires a command"));
    }

    #[test]
    fn run_rejects_bogus_version() {
        let result = run("not.a.version", &["php".to_string(), "-v".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn run_silent_rejects_bogus_version() {
        let result = run_silent("not.a.version", &["php".to_string(), "-v".to_string()]);
        assert!(result.is_err());
    }
}
