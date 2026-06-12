use std::fmt;

use console::style;

/// Output level for messages.
#[allow(dead_code)]
pub enum Level {
    Info,
    Success,
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
