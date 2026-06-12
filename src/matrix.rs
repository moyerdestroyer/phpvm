use anyhow::Result;

use crate::config;
use crate::output;
use crate::runner;

/// Run a command across multiple PHP runtimes and report results.
pub fn run(command: &[String]) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("matrix requires a command to run (e.g. `phpvm matrix composer test`)");
    }

    let project_dir = std::env::current_dir()?;
    let config = config::load_config(&project_dir)?;

    // Build the matrix of PHP versions to test
    let versions = build_matrix(&config)?;

    output::info("PHP Compatibility Matrix");
    output::info("========================");

    for version in &versions {
        match runner::run_silent(version, command) {
            Ok(_) => {
                output::success(&format!("{} PASS", version));
            }
            Err(_) => {
                output::error(&format!("{} FAIL", version));
            }
        }
    }

    Ok(())
}

/// Build the matrix of PHP versions from config or defaults.
fn build_matrix(config: &config::Config) -> Result<Vec<String>> {
    if let Some(ref matrix) = config.matrix {
        return Ok(matrix.clone());
    }

    // Default matrix: latest patch of each supported minor
    // TODO: Resolve these from the manifest
    Ok(vec![
        "8.1.latest".to_string(),
        "8.2.latest".to_string(),
        "8.3.latest".to_string(),
        "8.4.latest".to_string(),
    ])
}
