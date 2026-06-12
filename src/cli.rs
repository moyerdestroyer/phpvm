use clap::{Parser, Subcommand};

/// PHP Compatibility Manager — test and run PHP applications across multiple runtimes.
#[derive(Parser)]
#[command(name = "phpvm", version, about)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Install a PHP runtime (e.g. 8.3, 8.3.23)
    Install {
        /// PHP version to install (e.g. 8.3, 8.3.23, 8.3.latest)
        version: String,
        /// Extension profile to install (wordpress, laravel, minimal, or custom)
        #[arg(long)]
        profile: Option<String>,
    },

    /// Run a command against a specific PHP runtime
    Run {
        /// PHP version to use (e.g. 8.3, 8.3.23)
        version: String,
        /// Command and arguments to execute
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },

    /// Run a command across multiple PHP runtimes
    Matrix {
        /// Output format: human or json
        #[arg(long, value_name = "FORMAT", default_value = "human")]
        report: String,
        /// Command and arguments to execute across the matrix
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },

    /// Inspect the current project and show recommendations
    Doctor {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Verify compatibility claims before a release
    ReleaseCheck {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// List available extension profiles
    Profiles {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// List installed PHP runtimes
    Versions,
}

/// Parse a report format string into an OutputFormat.
impl Command {
    /// Get the output format for commands that support it.
    #[allow(dead_code)]
    pub fn output_format(&self) -> crate::output::OutputFormat {
        match self {
            Command::Matrix { report, .. } => parse_report_format(report),
            Command::Doctor { json } | Command::ReleaseCheck { json } => {
                if *json {
                    crate::output::OutputFormat::Json
                } else {
                    crate::output::OutputFormat::Human
                }
            }
            _ => crate::output::OutputFormat::Human,
        }
    }
}

pub fn parse_report_format(format: &str) -> crate::output::OutputFormat {
    match format.to_lowercase().as_str() {
        "json" => crate::output::OutputFormat::Json,
        "human" => crate::output::OutputFormat::Human,
        _ => crate::output::OutputFormat::Human,
    }
}
