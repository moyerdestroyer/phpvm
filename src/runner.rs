use anyhow::Result;

use crate::config;
use crate::output;

/// Run a command against a specific PHP runtime.
pub fn run(version: &str, command: &[String]) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("run requires a command (e.g. `phpvm run 8.3 php -v`)");
    }

    let resolved = crate::version::resolve(version)?;
    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);

    if !runtime_path.exists() {
        anyhow::bail!(
            "PHP runtime {} is not installed. Run `phpvm install {}` first.",
            resolved,
            version
        );
    }

    output::info(&format!("Running with PHP {}", resolved));

    // TODO: Build PATH with runtime's bin/ directory
    // TODO: Execute command in the runtime environment
    // TODO: Stream stdout/stderr

    Ok(())
}

/// Run a command silently (for matrix use). Returns Ok(()) on success, Err on failure.
pub fn run_silent(version: &str, _command: &[String]) -> Result<()> {
    let resolved = crate::version::resolve(version)?;
    let runtimes_dir = config::runtimes_dir()?;
    let runtime_path = runtimes_dir.join(&resolved);

    if !runtime_path.exists() {
        anyhow::bail!("PHP runtime {} is not installed", resolved);
    }

    // TODO: Execute command in the runtime environment, capture exit code
    Ok(())
}
