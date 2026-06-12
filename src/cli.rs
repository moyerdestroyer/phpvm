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
        /// Command and arguments to execute across the matrix
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },

    /// Inspect the current project and show recommendations
    Doctor,

    /// Verify compatibility claims before a release
    ReleaseCheck,

    /// List installed PHP runtimes
    Versions,
}
