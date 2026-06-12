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

    /// Set the active PHP runtime (persisted across all terminals/sessions).
    ///
    /// `phpvm use` is the single command that chooses which PHP (and its
    /// Composer + per-minor global packages) you are using, much like `fnm use`.
    ///
    /// After the one-time `eval "$(phpvm env)"` in your shell rc, plain
    /// `phpvm use 8.3` will take effect immediately in the current shell
    /// (the wrapper handles applying the exports).
    ///
    /// Globals via `composer global` are isolated per minor series (8.3.x share).
    Use {
        /// PHP version to activate (e.g. 8.3, 8.3.23). Must be installed.
        version: String,
    },

    /// Print shell integration code.
    ///
    /// One-time setup — add to your ~/.zshrc, ~/.bashrc, etc.:
    ///   eval "$(phpvm env)"
    ///
    /// After this (modeled on fnm):
    /// - `phpvm use 8.3` will immediately switch the *current* shell
    ///   (no manual eval of the output required for each use).
    /// - New shells will start with the last-used version active.
    ///
    /// `phpvm use` is the single command that sets your active PHP version
    /// (persisted globally, no separate "default" needed).
    Env {
        /// Activate a specific version instead of the one from `phpvm use`.
        #[arg(long)]
        version: Option<String>,
    },

    /// Show the PHP version that is currently active (from `phpvm use` or
    /// the current shell environment).
    Current,

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
    Ls,

    /// List PHP versions available for install (from remote manifest)
    LsRemote,

    /// Show metadata for a PHP runtime (PHP, Composer, profile, extensions)
    Info {
        /// PHP version specifier (e.g. 8.3, 8.3.23, 8.3.latest)
        version: String,
    },

    /// List installed PHP runtimes (deprecated; use `ls`)
    Versions,
}

/// Parse a report format string into an OutputFormat.
impl Command {
    /// Get the output format for commands that support it.
    pub fn output_format(&self) -> crate::output::OutputFormat {
        match self {
            Command::Matrix { report, .. } => parse_report_format(report),
            Command::Doctor { json }
            | Command::ReleaseCheck { json }
            | Command::Profiles { json } => {
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
