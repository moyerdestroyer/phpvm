use std::path::Path;
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

fn resolve_program(runtime_path: &Utf8PathBuf, resolved: &str, program: &str) -> Result<String> {
    if is_explicit_path(program) {
        return Ok(program.to_string());
    }

    let bin_dir = runtime_path.join("bin");
    let composer_home = crate::version::composer_home_for(resolved)?;
    let global_bin = composer_home.join("vendor").join("bin");

    for dir in [&bin_dir, &global_bin] {
        for candidate in managed_program_candidates(program) {
            let path = dir.join(&candidate);
            if path.exists() {
                return Ok(path.to_string());
            }
        }
    }

    bail!(
        "Command `{}` was not found in PHPVM runtime bin or Composer global bin for PHP {}. \
         Install it with `phpvm run {} composer global require <package>` or run it by explicit path.",
        program,
        resolved,
        resolved
    );
}

fn is_explicit_path(program: &str) -> bool {
    program.contains('/') || program.contains('\\') || Path::new(program).is_absolute()
}

fn managed_program_candidates(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let base = program.strip_suffix(".exe").unwrap_or(program);
        vec![
            program.to_string(),
            format!("{}.exe", base),
            format!("{}.bat", base),
            format!("{}.cmd", base),
        ]
    }

    #[cfg(not(windows))]
    {
        vec![program.to_string()]
    }
}

fn status_message(exit_status: std::process::ExitStatus) -> String {
    if let Some(code) = exit_status.code() {
        format!("Command exited with status {}", code)
    } else {
        "Command terminated by signal".to_string()
    }
}

/// Result of a captured command run.
pub struct SilentRunResult {
    pub resolved_version: String,
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
    let program = resolve_program(&runtime_path, &resolved, program)?;

    let status = Command::new(&program)
        .args(args)
        .env("PATH", &new_path)
        .env("COMPOSER_HOME", composer_home.as_str())
        .env("PHPVM_VERSION", &resolved)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to execute command")?
        .wait()
        .context("Failed to execute command")?;

    if !status.success() {
        bail!("{}", status_message(status));
    }
    Ok(())
}

/// Run a command silently (for matrix use). Returns `Ok(())` on success,
/// `Err` on failure. Stdout and stderr are captured, not displayed.
pub fn run_silent(version: &str, command: &[String]) -> Result<SilentRunResult> {
    let resolved = resolve_runtime(version)?;
    require_runtime(&resolved, version)?;

    let runtime_path = config::runtimes_dir()?.join(&resolved);
    let new_path = build_runtime_path(&runtime_path, &resolved)?;
    let composer_home = crate::version::composer_home_for(&resolved)?;

    let (program, args) = command
        .split_first()
        .context("run_silent requires a non-empty command")?;
    let program = resolve_program(&runtime_path, &resolved, program)?;

    let output = Command::new(&program)
        .args(args)
        .env("PATH", &new_path)
        .env("COMPOSER_HOME", composer_home.as_str())
        .env("PHPVM_VERSION", &resolved)
        .output()
        .context("Failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        let mut message = status_message(output.status);
        if !stdout.trim().is_empty() {
            message.push_str("\nstdout:\n");
            message.push_str(stdout.trim_end());
        }
        if !stderr.trim().is_empty() {
            message.push_str("\nstderr:\n");
            message.push_str(stderr.trim_end());
        }
        bail!("{}", message);
    }

    Ok(SilentRunResult {
        resolved_version: resolved,
    })
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
    use std::ffi::OsString;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvSnapshot {
        phpvm_home: Option<OsString>,
        path: Option<OsString>,
    }

    impl EnvSnapshot {
        fn capture() -> Self {
            Self {
                phpvm_home: std::env::var_os("PHPVM_HOME"),
                path: std::env::var_os("PATH"),
            }
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            if let Some(value) = &self.phpvm_home {
                std::env::set_var("PHPVM_HOME", value);
            } else {
                std::env::remove_var("PHPVM_HOME");
            }

            if let Some(value) = &self.path {
                std::env::set_var("PATH", value);
            } else {
                std::env::remove_var("PATH");
            }
        }
    }

    #[test]
    fn installed_versions_returns_vec() {
        let versions = installed_versions();
        // May be empty in CI, but should never error.
        assert!(versions.is_ok());
    }

    #[test]
    fn build_runtime_path_includes_bin_and_globals() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture();
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
    fn resolve_program_finds_composer_global_tool() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture();
        let dir = tempfile::TempDir::new().unwrap();
        let home = Utf8PathBuf::from_path_buf(dir.path().join(".phpvm")).unwrap();
        std::env::set_var("PHPVM_HOME", home.as_str());

        let runtime = home.join("runtimes").join("8.3.12");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::create_dir_all(home.join("composer-homes").join("8.3").join("vendor/bin"))
            .unwrap();
        std::fs::File::create(
            home.join("composer-homes")
                .join("8.3")
                .join("vendor/bin")
                .join("phpcs"),
        )
        .unwrap();

        let program = resolve_program(&runtime, "8.3.12", "phpcs").unwrap();
        assert!(program.ends_with("composer-homes/8.3/vendor/bin/phpcs"));
    }

    #[test]
    fn resolve_program_rejects_host_fallback_for_bare_tool() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture();
        let dir = tempfile::TempDir::new().unwrap();
        let home = Utf8PathBuf::from_path_buf(dir.path().join(".phpvm")).unwrap();
        let host_bin = Utf8PathBuf::from_path_buf(dir.path().join("host-bin")).unwrap();
        std::env::set_var("PHPVM_HOME", home.as_str());
        std::env::set_var("PATH", host_bin.as_str());

        let runtime = home.join("runtimes").join("8.3.12");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::create_dir_all(&host_bin).unwrap();
        std::fs::File::create(host_bin.join("phpcs")).unwrap();

        let result = resolve_program(&runtime, "8.3.12", "phpcs");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("was not found in PHPVM"));
    }

    #[test]
    fn status_message_success_is_not_used_for_ok_status() {
        // Simulate a successful exit status (code 0).
        let output = Command::new("true").output();
        if let Ok(out) = output {
            assert_eq!(status_message(out.status), "Command exited with status 0");
        }
    }

    #[test]
    fn status_message_failure() {
        // Simulate a failing exit status (code 1).
        let output = Command::new("false").output();
        if let Ok(out) = output {
            assert!(status_message(out.status).contains("Command exited with status"));
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
