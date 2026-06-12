use anyhow::Result;

use crate::config;
use crate::output;

/// Inspect the current project and display recommendations.
pub fn run() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let config = config::load_config(&project_dir)?;

    output::info("Project Inspection");
    output::info("==================");

    // TODO: Detect project type (WordPress plugin, Laravel app, Composer library)
    // TODO: Read composer.json for PHP constraints
    // TODO: Read composer.json for extension requirements
    // TODO: Recommend a matrix based on constraints

    if let Some(ref constraint) = config.php_constraint {
        output::info(&format!("PHP Constraint: {}", constraint));
    } else {
        output::info("PHP Constraint: not specified");
    }

    if let Some(ref profile) = config.profile {
        output::info(&format!("Profile: {}", profile));
    } else {
        output::info("Profile: not specified");
    }

    Ok(())
}

/// Run a release-check: verify compatibility claims before release.
pub fn release_check() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let config = config::load_config(&project_dir)?;

    output::info("Release Compatibility Check");
    output::info("===========================");

    // TODO: Detect project type
    // TODO: Extract PHP constraint from composer.json
    // TODO: Build matrix from constraint
    // TODO: Run matrix and report results
    // TODO: Output RELEASE READY or RELEASE BLOCKED

    if let Some(ref constraint) = config.php_constraint {
        output::info(&format!("PHP Constraint: {}", constraint));
    }

    Ok(())
}
