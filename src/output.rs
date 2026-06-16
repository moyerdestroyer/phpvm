use std::fmt;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use console::{style, Term};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Color / progress gates
// ---------------------------------------------------------------------------

static COLOR_OVERRIDE: AtomicBool = AtomicBool::new(false);
static COLOR_FORCED: AtomicBool = AtomicBool::new(false);

/// Whether ANSI styling is enabled (respects `NO_COLOR`, `CLICOLOR_FORCE`, TTY).
pub fn color_enabled() -> bool {
    if COLOR_OVERRIDE.load(Ordering::Relaxed) {
        return COLOR_FORCED.load(Ordering::Relaxed);
    }

    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }

    match std::env::var("CLICOLOR_FORCE").ok().as_deref() {
        Some("0") => return false,
        Some("1") => return true,
        _ => {}
    }

    Term::stdout().features().colors_supported()
}

/// Override color enablement (for unit tests). Pass `false` to disable styling.
#[cfg(test)]
pub fn set_color_enabled(enabled: bool) {
    COLOR_OVERRIDE.store(true, Ordering::Relaxed);
    COLOR_FORCED.store(enabled, Ordering::Relaxed);
}

/// Reset color override (for unit tests).
#[cfg(test)]
pub fn reset_color_override() {
    COLOR_OVERRIDE.store(false, Ordering::Relaxed);
}

/// Whether live spinners and progress bars should be drawn.
pub fn progress_enabled() -> bool {
    color_enabled() && Term::stderr().is_term()
}

fn styled_if<F>(text: &str, apply: F) -> String
where
    F: FnOnce(&str) -> console::StyledObject<&str>,
{
    if color_enabled() {
        apply(text).to_string()
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// Styled terminal output
// ---------------------------------------------------------------------------

/// Output level for styled messages.
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

/// Print a styled message to stdout.
pub fn print(level: Level, message: &str) {
    let line = match level {
        Level::Info => line_info(message),
        Level::Success => line_success(message),
        Level::Warning => line_warn(message),
        Level::Error => line_error(message),
    };
    println!("{line}");
}

/// Print an info message.
pub fn info(message: &str) {
    print(Level::Info, message);
}

/// Print a success message.
pub fn success(message: &str) {
    print(Level::Success, message);
}

/// Print a success message to stderr (safe when stdout is eval-able shell output).
pub fn success_stderr(message: &str) {
    print_stderr(Level::Success, message);
}

/// Print a warning message.
pub fn warn(message: &str) {
    print(Level::Warning, message);
}

fn print_stderr(level: Level, message: &str) {
    let styled = match level {
        Level::Info => styled_if(message, |m| style(m).cyan()),
        Level::Success => styled_if(message, |m| style(m).green()),
        Level::Warning => styled_if(message, |m| style(m).yellow()),
        Level::Error => styled_if(message, |m| style(m).red()),
    };
    eprintln!("{}", styled);
}

/// Print an info message to stderr.
pub fn info_stderr(message: &str) {
    print_stderr(Level::Info, message);
}

/// Print an error message.
#[allow(dead_code)]
pub fn error(message: &str) {
    print(Level::Error, message);
}

fn line_heading(title: &str) -> String {
    styled_if(title, |t| style(t).cyan().bold())
}

fn line_info(message: &str) -> String {
    styled_if(message, |m| style(m).cyan())
}

fn line_success(message: &str) -> String {
    styled_if(message, |m| style(m).green())
}

fn line_error(message: &str) -> String {
    styled_if(message, |m| style(m).red())
}

fn line_warn(message: &str) -> String {
    styled_if(message, |m| style(m).yellow())
}

fn line_label(key: &str, value: &str) -> String {
    let key_col = format!("{key:<16}");
    format!("{}{}", styled_if(&key_col, |k| style(k).dim()), value)
}

fn line_list_item(message: &str) -> String {
    format!("  {message}")
}

fn line_list_item_dim(message: &str) -> String {
    format!("  {}", dim(message))
}

/// Print a section heading (bold cyan).
pub fn heading(title: &str) {
    println!("{}", line_heading(title));
}

/// Print a dim label + value pair (`PHP: 8.3.23`).
pub fn label(key: &str, value: &str) {
    println!("{}", line_label(key, value));
}

/// Return dim-styled text (or plain when color is off).
pub fn dim(message: &str) -> String {
    styled_if(message, |m| style(m).dim())
}

/// Return bold-styled text (or plain when color is off).
pub fn bold(message: &str) -> String {
    styled_if(message, |m| style(m).bold())
}

/// Print a PASS/FAIL badge with semantic coloring.
pub fn status_badge(passed: bool, text: &str) -> String {
    if passed {
        styled_if(text, |t| style(t).green().bold())
    } else {
        styled_if(text, |t| style(t).red().bold())
    }
}

/// Print an indented list item.
pub fn list_item(message: &str) {
    println!("  {}", message);
}

/// Print an indented list item with dim styling.
pub fn list_item_dim(message: &str) {
    println!("  {}", dim(message));
}

/// Print an active runtime entry (`* 8.3.23`).
pub fn list_active_item(item: &str) {
    println!("{} {}", styled_if("*", |s| style(s).green().bold()), item);
}

/// Print a blank line.
pub fn blank() {
    println!();
}

/// Print a fatal error to stderr and exit with code 1.
pub fn fatal(err: &anyhow::Error) -> ! {
    fatal_with_code(err, 1);
}

/// Print a fatal error to stderr and exit with the given code.
pub fn fatal_with_code(err: &anyhow::Error, code: i32) -> ! {
    eprintln!(
        "{} {:#}",
        styled_if("error:", |p| style(p).red().bold()),
        err
    );
    std::process::exit(code);
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
    pub required_extensions: Vec<String>,
    pub missing_extensions: Vec<String>,
    pub recommended_matrix: Vec<String>,

    // Runtime verification (best-effort, populated when an installed runtime can be
    // resolved for the active/profile context). Supports the static-only model
    // (bin/php -v, composer -V, php -m contains manifest catalog).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_php_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_catalog_extensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_extras_not_in_binary: Option<Vec<String>>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
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

fn render_matrix_entry_lines(entry: &MatrixEntry) -> Vec<String> {
    let mut lines = Vec::new();
    match entry.status {
        RunStatus::Pass => {
            lines.push(line_success(&format!(
                "{} {}",
                entry.php_version,
                status_badge(true, "PASS")
            )));
        }
        RunStatus::Fail => {
            lines.push(line_error(&format!(
                "{} {}",
                entry.php_version,
                status_badge(false, "FAIL")
            )));
            if let Some(output) = &entry.output {
                for line in output.lines() {
                    lines.push(line_list_item_dim(line));
                }
            }
        }
    }
    lines
}

fn render_matrix_human(result: &MatrixResult, live_reported: bool) -> String {
    let mut lines = vec![line_heading("PHP Compatibility Matrix")];
    if live_reported {
        for entry in &result.entries {
            if matches!(entry.status, RunStatus::Fail) {
                if let Some(output) = &entry.output {
                    lines.push(line_error(&format!("{} details:", entry.php_version)));
                    for line in output.lines() {
                        lines.push(line_list_item_dim(line));
                    }
                }
            }
        }
    } else {
        for entry in &result.entries {
            lines.extend(render_matrix_entry_lines(entry));
        }
    }
    lines.push(String::new());
    match result.overall {
        RunStatus::Pass => {
            lines.push(line_success(&format!(
                "Overall: {}",
                status_badge(true, "PASS")
            )));
        }
        RunStatus::Fail => {
            lines.push(line_error(&format!(
                "Overall: {}",
                status_badge(false, "FAIL")
            )));
        }
    }
    lines.join("\n")
}

/// Print a matrix result in the requested format.
pub fn print_matrix_result(result: &MatrixResult, format: OutputFormat, live_reported: bool) {
    match format {
        OutputFormat::Human => {
            println!("{}", render_matrix_human(result, live_reported));
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
            });
            println!("{}", json);
        }
    }
}

fn render_doctor_human(result: &DoctorResult) -> String {
    let mut lines = vec![line_heading("Project Inspection")];
    lines.push(line_label(
        "Project Type:",
        result.project_type.as_deref().unwrap_or("unknown"),
    ));
    lines.push(line_label(
        "PHP Constraint:",
        result.php_constraint.as_deref().unwrap_or("not specified"),
    ));
    lines.push(line_label(
        "Profile:",
        result.profile.as_deref().unwrap_or("not specified"),
    ));

    if !result.required_extensions.is_empty() {
        lines.push(line_label(
            "Required:",
            &result.required_extensions.join(", "),
        ));
    }

    if !result.missing_extensions.is_empty() {
        lines.push(line_warn(&format!(
            "Missing from active profile: {}",
            result.missing_extensions.join(", ")
        )));
        if let Some(profile_name) = &result.profile {
            lines.push(line_info(&format!(
                "Try: phpvm profile use {profile_name} or phpvm profile edit {profile_name}"
            )));
        }
    }

    if !result.recommended_matrix.is_empty() {
        lines.push(line_info("Recommended Matrix:"));
        for v in &result.recommended_matrix {
            lines.push(line_list_item(v));
        }
    }

    // Runtime verification section (when doctor was able to inspect an active runtime)
    if let Some(rv) = &result.runtime_version {
        lines.push(String::new());
        lines.push(line_info("Runtime Verification:"));
        let ok = result.runtime_ok.unwrap_or(false);
        let badge = status_badge(ok, if ok { "OK" } else { "ISSUES" });
        lines.push(line_list_item(&format!("{} {}", rv, badge)));
        if let Some(vline) = &result.runtime_php_version {
            lines.push(line_list_item_dim(vline));
        }
        if let Some(missing) = &result.missing_catalog_extensions {
            if !missing.is_empty() {
                lines.push(line_error(&format!(
                    "Missing from binary (catalog): {}",
                    missing.join(", ")
                )));
            }
        }
        if let Some(extras) = &result.profile_extras_not_in_binary {
            if !extras.is_empty() {
                lines.push(line_warn(&format!(
                    "Profile lists (optional / not in this static build): {}",
                    extras.join(", ")
                )));
            }
        }
    }

    lines.join("\n")
}

/// Print a doctor result in the requested format.
pub fn print_doctor_result(result: &DoctorResult, format: OutputFormat) {
    match format {
        OutputFormat::Human => {
            println!("{}", render_doctor_human(result));
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(result).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize result: {}\"}}", e)
            });
            println!("{}", json);
        }
    }
}

fn render_release_check_human(result: &ReleaseCheckResult, live_reported: bool) -> String {
    let mut lines = vec![line_heading("Release Compatibility Check")];
    lines.push(line_label(
        "Detected:",
        result
            .project_type
            .as_deref()
            .unwrap_or("unknown project type"),
    ));
    lines.push(line_label(
        "PHP Constraint:",
        result.php_constraint.as_deref().unwrap_or("not specified"),
    ));
    lines.push(String::new());

    if !live_reported {
        lines.push(line_info("Testing:"));
        for entry in &result.entries {
            lines.extend(render_matrix_entry_lines(entry));
        }
        lines.push(String::new());
    }

    match result.overall {
        RunStatus::Pass => {
            lines.push(line_success(&format!(
                "Result: {}",
                status_badge(true, "RELEASE READY")
            )));
        }
        RunStatus::Fail => {
            lines.push(line_error(&format!(
                "Result: {}",
                status_badge(false, "RELEASE BLOCKED")
            )));
            if live_reported {
                for entry in &result.entries {
                    if matches!(entry.status, RunStatus::Fail) {
                        if let Some(output) = &entry.output {
                            lines.push(line_error(&format!("{} details:", entry.php_version)));
                            for line in output.lines() {
                                lines.push(line_list_item_dim(line));
                            }
                        }
                    }
                }
            }
        }
    }

    lines.join("\n")
}

/// Print a release-check result in the requested format.
pub fn print_release_check_result(
    result: &ReleaseCheckResult,
    format: OutputFormat,
    live_reported: bool,
) {
    match format {
        OutputFormat::Human => {
            println!("{}", render_release_check_human(result, live_reported));
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
// Multi-step install progress
// ---------------------------------------------------------------------------

enum StepMode {
    Plain,
    Indicatif(MultiProgress),
}

/// Tracks progress through a multi-step operation (e.g. runtime install).
pub struct StepList {
    mode: StepMode,
    active_bar: Option<ProgressBar>,
}

impl StepList {
    /// Create a new step list. When progress is disabled, steps print as plain lines.
    pub fn new() -> Self {
        let mode = if progress_enabled() {
            StepMode::Indicatif(MultiProgress::new())
        } else {
            StepMode::Plain
        };
        Self {
            mode,
            active_bar: None,
        }
    }

    /// Mark a step as running.
    pub fn start(&mut self, label: &str) {
        match &self.mode {
            StepMode::Plain => {
                eprintln!("  ... {}", label);
            }
            StepMode::Indicatif(multi) => {
                let progress_bar = multi.add(ProgressBar::new_spinner());
                progress_bar.set_style(step_spinner_style());
                progress_bar.set_message(label.to_string());
                progress_bar.enable_steady_tick(std::time::Duration::from_millis(80));
                self.active_bar = Some(progress_bar);
            }
        }
    }

    /// Mark the current step as completed.
    pub fn done(&mut self, label: &str) {
        let finished = format!("✓ {}", label);
        match &self.mode {
            StepMode::Plain => {
                eprintln!("  {}", styled_if(&finished, |f| style(f).green()));
            }
            StepMode::Indicatif(_) => {
                if let Some(progress_bar) = self.active_bar.take() {
                    progress_bar.finish_with_message(styled_if(&finished, |f| style(f).green()));
                }
            }
        }
    }

    /// Mark the current step as failed.
    pub fn fail(&mut self, label: &str, err: &str) {
        let finished = format!("✗ {}: {}", label, err);
        match &self.mode {
            StepMode::Plain => {
                eprintln!("  {}", styled_if(&finished, |f| style(f).red()));
            }
            StepMode::Indicatif(_) => {
                if let Some(progress_bar) = self.active_bar.take() {
                    progress_bar.finish_with_message(styled_if(&finished, |f| style(f).red()));
                }
            }
        }
    }

    /// Add a byte-progress bar nested under the current multi-progress group.
    pub fn add_download_bar(&self, total_size: Option<u64>) -> ProgressBar {
        match &self.mode {
            StepMode::Plain => ProgressBar::hidden(),
            StepMode::Indicatif(multi) => {
                let progress_bar = multi.add(ProgressBar::new(total_size.unwrap_or(0)));
                if total_size.is_some() {
                    progress_bar.set_style(download_bar_style());
                } else {
                    progress_bar.set_style(download_spinner_style());
                }
                progress_bar
            }
        }
    }

    /// Clear any remaining spinner lines.
    pub fn finish(&self) {
        if let StepMode::Indicatif(multi) = &self.mode {
            multi.clear().ok();
        }
    }
}

impl Default for StepList {
    fn default() -> Self {
        Self::new()
    }
}

fn step_spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .template("  {spinner:.green} {msg}")
        .expect("hardcoded step spinner template is valid")
}

fn download_bar_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("    [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .expect("hardcoded download bar template is valid")
        .progress_chars("#>-")
}

fn download_spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .template("    {spinner:.green} {bytes} downloaded ({elapsed_precise})")
        .expect("hardcoded download spinner template is valid")
}

/// Stream `reader` into `writer`, updating `bar` with byte progress.
pub fn download_with_progress<R: Read>(
    mut reader: R,
    mut writer: impl Write,
    progress_bar: &ProgressBar,
) -> io::Result<u64> {
    if progress_bar.is_hidden() {
        return io::copy(&mut reader, &mut writer);
    }

    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        writer.write_all(&buffer[..bytes_read])?;
        downloaded += bytes_read as u64;
        progress_bar.set_position(downloaded);
    }

    Ok(downloaded)
}

// ---------------------------------------------------------------------------
// Matrix / release-check live progress
// ---------------------------------------------------------------------------

/// Spinner shown while a matrix version is being tested.
pub struct VersionSpinner {
    progress_bar: Option<ProgressBar>,
    version: String,
}

impl VersionSpinner {
    /// Begin testing indicator for `version`.
    pub fn start(version: &str) -> Self {
        let progress_bar = if progress_enabled() {
            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} Testing {msg} ...")
                    .expect("hardcoded version spinner template is valid"),
            );
            spinner.set_message(version.to_string());
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));
            Some(spinner)
        } else {
            None
        };

        if progress_bar.is_none() {
            eprintln!("  Testing {} ...", version);
        }

        Self {
            progress_bar,
            version: version.to_string(),
        }
    }

    /// Finish with pass or fail status.
    pub fn finish(self, passed: bool) {
        let badge = if passed {
            status_badge(true, "PASS")
        } else {
            status_badge(false, "FAIL")
        };
        let line = format!("✓ {} {}", self.version, badge);
        let fail_line = format!("✗ {} {}", self.version, badge);

        match self.progress_bar {
            Some(progress_bar) => {
                if passed {
                    progress_bar.finish_with_message(styled_if(&line, |l| style(l).green()));
                } else {
                    progress_bar.finish_with_message(styled_if(&fail_line, |l| style(l).red()));
                }
            }
            None => {
                if passed {
                    eprintln!("  {}", styled_if(&line, |l| style(l).green()));
                } else {
                    eprintln!("  {}", styled_if(&fail_line, |l| style(l).red()));
                }
            }
        }
    }
}

/// Whether live per-version matrix reporting should be used.
pub fn live_matrix_progress(format: OutputFormat) -> bool {
    format == OutputFormat::Human && progress_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_color_off<F: FnOnce()>(f: F) {
        set_color_enabled(false);
        f();
        reset_color_override();
    }

    fn sample_matrix_result(overall: RunStatus) -> MatrixResult {
        MatrixResult {
            command: vec!["composer".into(), "test".into()],
            entries: vec![
                MatrixEntry {
                    php_version: "8.3.23".into(),
                    status: RunStatus::Pass,
                    output: None,
                },
                MatrixEntry {
                    php_version: "8.2.28".into(),
                    status: RunStatus::Fail,
                    output: Some("exit 1".into()),
                },
            ],
            overall,
        }
    }

    fn sample_doctor_result() -> DoctorResult {
        DoctorResult {
            project_type: Some("wordpress".to_string()),
            php_constraint: Some("^8.1".to_string()),
            profile: Some("wordpress".to_string()),
            required_extensions: vec!["mysqli".to_string()],
            missing_extensions: vec!["imagick".to_string()],
            recommended_matrix: vec!["8.1".to_string(), "8.3".to_string()],
            runtime_version: None,
            runtime_ok: None,
            runtime_php_version: None,
            missing_catalog_extensions: None,
            profile_extras_not_in_binary: None,
        }
    }

    fn sample_release_check_result() -> ReleaseCheckResult {
        ReleaseCheckResult {
            project_type: Some("laravel".into()),
            php_constraint: Some("^8.2".into()),
            entries: vec![
                MatrixEntry {
                    php_version: "8.3.23".into(),
                    status: RunStatus::Pass,
                    output: None,
                },
                MatrixEntry {
                    php_version: "8.2.28".into(),
                    status: RunStatus::Fail,
                    output: Some("runtime missing".into()),
                },
            ],
            overall: RunStatus::Fail,
        }
    }

    #[test]
    fn color_disabled_produces_plain_text() {
        with_color_off(|| {
            assert_eq!(dim("hello"), "hello");
            assert_eq!(bold("hello"), "hello");
            assert_eq!(status_badge(true, "PASS"), "PASS");
            assert_eq!(status_badge(false, "FAIL"), "FAIL");
        });
    }

    #[test]
    fn label_formats_key_value() {
        with_color_off(|| {
            assert_eq!(line_label("PHP:", "8.3.23"), "PHP:            8.3.23");
        });
    }

    #[test]
    fn doctor_human_output_snapshot() {
        with_color_off(|| {
            insta::assert_snapshot!(render_doctor_human(&sample_doctor_result()));
        });
    }

    #[test]
    fn matrix_human_output_snapshot() {
        with_color_off(|| {
            insta::assert_snapshot!(render_matrix_human(
                &sample_matrix_result(RunStatus::Fail),
                false
            ));
        });
    }

    #[test]
    fn matrix_live_summary_snapshot() {
        with_color_off(|| {
            insta::assert_snapshot!(render_matrix_human(
                &sample_matrix_result(RunStatus::Fail),
                true
            ));
        });
    }

    #[test]
    fn release_check_human_output_snapshot() {
        with_color_off(|| {
            insta::assert_snapshot!(render_release_check_human(
                &sample_release_check_result(),
                false
            ));
        });
    }

    #[test]
    fn release_check_live_summary_snapshot() {
        with_color_off(|| {
            insta::assert_snapshot!(render_release_check_human(
                &sample_release_check_result(),
                true
            ));
        });
    }
}
