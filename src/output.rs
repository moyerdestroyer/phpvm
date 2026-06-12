use std::fmt;

use console::style;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Styled terminal output
// ---------------------------------------------------------------------------

/// Output level for styled messages.
pub enum Level {
    Info,
    Success,
    #[allow(dead_code)]
    Warning,
    Error,
}

/// Print a styled message to stdout.
pub fn print(level: Level, message: &str) {
    let styled = match level {
        Level::Info => style(message).cyan().to_string(),
        Level::Success => style(message).green().to_string(),
        Level::Warning => style(message).yellow().to_string(),
        Level::Error => style(message).red().to_string(),
    };
    println!("{}", styled);
}

/// Print an info message.
pub fn info(message: &str) {
    print(Level::Info, message);
}

/// Print a success message.
pub fn success(message: &str) {
    print(Level::Success, message);
}

/// Print a warning message.
#[allow(dead_code)]
pub fn warn(message: &str) {
    print(Level::Warning, message);
}

/// Print an error message.
pub fn error(message: &str) {
    print(Level::Error, message);
}

// ---------------------------------------------------------------------------
// Structured result types (for JSON output support)
// ---------------------------------------------------------------------------

/// Result of running a single command against a single PHP version.
#[derive(Debug, Clone, Serialize)]
pub struct MatrixEntry {
    pub php_version: String,
    pub status: RunStatus,
    pub output: Option<String>,
}

/// Whether a matrix run passed or failed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RunStatus {
    Pass,
    Fail,
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunStatus::Pass => write!(f, "PASS"),
            RunStatus::Fail => write!(f, "FAIL"),
        }
    }
}

/// Full result of a matrix run.
#[derive(Debug, Clone, Serialize)]
pub struct MatrixResult {
    pub command: Vec<String>,
    pub entries: Vec<MatrixEntry>,
    pub overall: RunStatus,
}

impl MatrixResult {
    /// Determine overall status: Pass if all entries pass, Fail otherwise.
    pub fn compute_overall(entries: &[MatrixEntry]) -> RunStatus {
        if entries.iter().all(|e| matches!(e.status, RunStatus::Pass)) {
            RunStatus::Pass
        } else {
            RunStatus::Fail
        }
    }
}

/// Result of a doctor inspection.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorResult {
    pub project_type: Option<String>,
    pub php_constraint: Option<String>,
    pub profile: Option<String>,
    pub recommended_matrix: Vec<String>,
}

/// Result of a release-check.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseCheckResult {
    pub project_type: Option<String>,
    pub php_constraint: Option<String>,
    pub entries: Vec<MatrixEntry>,
    pub overall: RunStatus,
}

// ---------------------------------------------------------------------------
// Output format
// ---------------------------------------------------------------------------

/// Supported output formats.
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Human,
    #[allow(dead_code)]
    Json,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Human => write!(f, "human"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

/// Print a matrix result in the requested format.
#[allow(dead_code)]
pub fn print_matrix_result(result: &MatrixResult, format: OutputFormat) {
    match format {
        OutputFormat::Human => {
            info("PHP Compatibility Matrix");
            info("========================");
            for entry in &result.entries {
                match entry.status {
                    RunStatus::Pass => success(&format!("{} {}", entry.php_version, entry.status)),
                    RunStatus::Fail => error(&format!("{} {}", entry.php_version, entry.status)),
                }
            }
            println!();
            match result.overall {
                RunStatus::Pass => success("Overall: PASS"),
                RunStatus::Fail => error("Overall: FAIL"),
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
            });
            println!("{}", json);
        }
    }
}

/// Print a doctor result in the requested format.
pub fn print_doctor_result(result: &DoctorResult, format: OutputFormat) {
    match format {
        OutputFormat::Human => {
            info("Project Inspection");
            info("==================");

            match &result.project_type {
                Some(pt) => info(&format!("Project Type: {}", pt)),
                None => info("Project Type: unknown"),
            }

            match &result.php_constraint {
                Some(c) => info(&format!("PHP Constraint: {}", c)),
                None => info("PHP Constraint: not specified"),
            }

            match &result.profile {
                Some(p) => info(&format!("Profile: {}", p)),
                None => info("Profile: not specified"),
            }

            if !result.recommended_matrix.is_empty() {
                info("Recommended Matrix:");
                for v in &result.recommended_matrix {
                    println!("  {}", v);
                }
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
            });
            println!("{}", json);
        }
    }
}

/// Print a release-check result in the requested format.
pub fn print_release_check_result(result: &ReleaseCheckResult, format: OutputFormat) {
    match format {
        OutputFormat::Human => {
            info("Release Compatibility Check");
            info("===========================");

            match &result.project_type {
                Some(pt) => info(&format!("Detected: {}", pt)),
                None => info("Detected: unknown project type"),
            }

            match &result.php_constraint {
                Some(c) => info(&format!("PHP Constraint: {}", c)),
                None => info("PHP Constraint: not specified"),
            }

            println!();
            info("Testing:");
            for entry in &result.entries {
                match entry.status {
                    RunStatus::Pass => success(&format!("{} {}", entry.php_version, entry.status)),
                    RunStatus::Fail => error(&format!("{} {}", entry.php_version, entry.status)),
                }
            }

            println!();
            match result.overall {
                RunStatus::Pass => success("Result: RELEASE READY"),
                RunStatus::Fail => error("Result: RELEASE BLOCKED"),
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
            });
            println!("{}", json);
        }
    }
}

// ---------------------------------------------------------------------------
// Progress steps
// ---------------------------------------------------------------------------

/// A progress step in a multi-step operation.
#[allow(dead_code)]
pub struct Step {
    pub label: String,
    pub status: StepStatus,
}

#[allow(dead_code)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed(String),
}

impl fmt::Display for StepStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepStatus::Pending => write!(f, " "),
            StepStatus::Running => write!(f, "⠋"),
            StepStatus::Done => write!(f, "✓"),
            StepStatus::Failed(msg) => write!(f, "✗ {}", msg),
        }
    }
}
