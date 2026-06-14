use anyhow::Result;

use crate::config;
use crate::output::{self, MatrixEntry, MatrixResult, OutputFormat, RunStatus, VersionSpinner};
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

    let versions = config::resolve_matrix(&config);
    let live = output::live_matrix_progress(format);

    if live {
        output::heading("PHP Compatibility Matrix");
        output::blank();
    }

    let mut entries: Vec<MatrixEntry> = Vec::new();

    for version in &versions {
        let spinner = if live {
            Some(VersionSpinner::start(version))
        } else {
            None
        };

        let entry = match runner::run_silent(version, command) {
            Ok(run) => MatrixEntry {
                php_version: run.resolved_version,
                status: RunStatus::Pass,
                output: None,
            },
            Err(e) => MatrixEntry {
                php_version: version.clone(),
                status: RunStatus::Fail,
                output: Some(e.to_string()),
            },
        };

        if let Some(spinner) = spinner {
            spinner.finish(matches!(entry.status, RunStatus::Pass));
        }

        entries.push(entry);
    }

    let overall = MatrixResult::compute_overall(&entries);

    let result = MatrixResult {
        command: command.to_vec(),
        entries,
        overall,
    };

    output::print_matrix_result(&result, format, live);

    if matches!(result.overall, RunStatus::Fail) {
        anyhow::bail!("Matrix run had failures");
    }

    Ok(())
}
