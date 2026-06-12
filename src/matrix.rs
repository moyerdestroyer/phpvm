use anyhow::Result;

use crate::config;
use crate::output::{self, MatrixEntry, MatrixResult, OutputFormat, RunStatus};
use crate::runner;

/// Run a command across multiple PHP runtimes and report results (human-readable).
#[allow(dead_code)]
pub fn run(command: &[String]) -> Result<()> {
    run_with_format(command, OutputFormat::Human)
}

/// Run a command across multiple PHP runtimes and report results in the requested format.
pub fn run_with_format(command: &[String], format: OutputFormat) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("matrix requires a command to run (e.g. `phpvm matrix composer test`)");
    }

    let project_dir = config::current_project_dir()?;
    let config = config::load_config(&project_dir)?;

    // Build the matrix of PHP versions to test
    let versions = config::resolve_matrix(&config);

    let mut entries: Vec<MatrixEntry> = Vec::new();

    for version in &versions {
        match runner::run_silent(version, command) {
            Ok(_) => {
                entries.push(MatrixEntry {
                    php_version: version.clone(),
                    status: RunStatus::Pass,
                    output: None,
                });
            }
            Err(e) => {
                entries.push(MatrixEntry {
                    php_version: version.clone(),
                    status: RunStatus::Fail,
                    output: Some(e.to_string()),
                });
            }
        }
    }

    let overall = MatrixResult::compute_overall(&entries);

    let result = MatrixResult {
        command: command.to_vec(),
        entries,
        overall,
    };

    output::print_matrix_result(&result, format);

    // Return an error if any version failed, so exit code is non-zero
    if matches!(result.overall, RunStatus::Fail) {
        anyhow::bail!("Matrix run had failures");
    }

    Ok(())
}
