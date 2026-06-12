use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;

use crate::config;
use crate::output;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a modified PATH with the runtime's `bin/` directory prepended.
fn build_path(bin_dir: &Utf8PathBuf) -> Result<String> {
    let current_path = std::env::var("PATH").context("No PATH environment variable found")?;
    let separator = if cfg!(windows) { ";" } else { ":" };
    Ok(format!("{}{}{}", bin_dir, separator, current_path))
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

    let bin_dir = runtime_path.join("bin");
    let new_path = build_path(&bin_dir)?;

    let (program, args) = command
        .split_first()
        .context("run requires a non-empty command")?;

    let status = Command::new(program)
        .args(args)
        .env("PATH", &new_path)
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
    let bin_dir = runtime_path.join("bin");
    let new_path = build_path(&bin_dir)?;

    let (program, args) = command
        .split_first()
        .context("run_silent requires a non-empty command")?;

    let output = Command::new(program)
        .args(args)
        .env("PATH", &new_path)
        .output()
        .context("Failed to execute command")?;

    check_exit_status(output.status)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve a version specifier against installed runtimes.
fn resolve_runtime(version: &str) -> Result<String> {
    let available = installed_versions()?;
    crate::version::resolve_specifier(version, &available)
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
    fn build_path_prepends_bin_dir() {
        // Temporarily override PATH for deterministic test.
        std::env::set_var("PATH", "/usr/bin:/bin");
        let bin = Utf8PathBuf::from("/home/user/.phpvm/runtimes/8.3.12/bin");
        let result = build_path(&bin).unwrap();
        assert!(result.starts_with("/home/user/.phpvm/runtimes/8.3.12/bin:"));
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
