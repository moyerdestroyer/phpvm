use std::collections::BTreeSet;
use std::fs;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::config;
use crate::manifest::Manifest;
use crate::profile::ProfileTemplate;
use crate::runtime_metadata;

/// Where a resolved profile preset lives on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetSource {
    Project,
    Global,
    Runtime,
    Bundled,
}

impl PresetSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PresetSource::Project => "project",
            PresetSource::Global => "global",
            PresetSource::Runtime => "runtime",
            PresetSource::Bundled => "bundled",
        }
    }
}

/// A resolved profile ini preset ready to activate.
#[derive(Debug, Clone)]
pub struct ResolvedPreset {
    #[allow(dead_code)]
    pub name: String,
    pub path: Utf8PathBuf,
    pub source: PresetSource,
}

/// Listed profile preset for `phpvm profile list`.
#[derive(Debug, Clone, Serialize)]
pub struct ListedPreset {
    pub name: String,
    pub path: String,
    pub source: String,
}

const BUNDLED_WORDPRESS: &str = include_str!("../profiles/wordpress.ini");
const BUNDLED_LARAVEL: &str = include_str!("../profiles/laravel.ini");
const BUNDLED_MINIMAL: &str = include_str!("../profiles/minimal.ini");

const BUILTIN_NAMES: &[&str] = &["wordpress", "laravel", "minimal"];

/// Validate a profile preset name (safe for use as a filename stem).
pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Profile name cannot be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        anyhow::bail!("Profile name cannot contain path separators");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        anyhow::bail!(
            "Profile name '{}' contains invalid characters (use letters, numbers, _, -, .)",
            name
        );
    }
    Ok(())
}

/// Project-local preset directory: `<project>/.phpvm/profiles/`.
pub fn project_profiles_dir(project_dir: &Utf8Path) -> Utf8PathBuf {
    project_dir.join(".phpvm").join("profiles")
}

/// Global preset directory: `~/.phpvm/profiles/`.
pub fn global_profiles_dir() -> Result<Utf8PathBuf> {
    Ok(config::data_dir()?.join("profiles"))
}

fn preset_file_path(dir: &Utf8Path, name: &str) -> Utf8PathBuf {
    dir.join(format!("{name}.ini"))
}

fn bundled_starter_content(name: &str) -> Option<&'static str> {
    match name {
        "wordpress" => Some(BUNDLED_WORDPRESS),
        "laravel" => Some(BUNDLED_LARAVEL),
        "minimal" => Some(BUNDLED_MINIMAL),
        _ => None,
    }
}

/// Return bundled starter content or generate from manifest template.
pub fn starter_content(name: &str, manifest: Option<&Manifest>) -> Result<String> {
    if let Some(content) = bundled_starter_content(name) {
        return Ok(content.to_string());
    }

    if let Some(mf) = manifest {
        if let Some(template) = mf.resolve_profile_template(name) {
            return Ok(render_template_ini(&template));
        }
    }

    anyhow::bail!(
        "No profile preset '{}' found. Add .phpvm/profiles/{}.ini or \
         ~/.phpvm/profiles/{}.ini",
        name,
        name,
        name
    )
}

fn render_template_ini(template: &ProfileTemplate) -> String {
    // For the static-only model, profile .ini files are user-level tuning presets
    // (memory, opcache, error reporting, etc.). The extension catalog lives in the
    // manifest and is compiled into the binary; we do not emit extension= load lines
    // from manifest templates for static runtimes.
    [
        format!("; PHPVM profile preset: {}", template.name),
        "; Generated from manifest template (static runtimes have compiled-in extensions)."
            .to_string(),
        String::new(),
        "; Add memory_limit, opcache, error_reporting, etc. here.".to_string(),
        String::new(),
    ]
    .join("\n")
}

/// Write starter content to `dest` only when the file does not exist.
pub fn materialize_starter_if_missing(dest: &Utf8Path, content: &str) -> Result<bool> {
    if dest.exists() {
        return Ok(false);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent))?;
    }
    fs::write(dest, content).with_context(|| format!("Failed to write preset {}", dest))?;
    Ok(true)
}

/// Look up an existing preset without materializing starters.
pub fn find_existing_preset(
    name: &str,
    project_dir: &Utf8Path,
    runtime_dir: &Utf8Path,
) -> Result<Option<ResolvedPreset>> {
    validate_profile_name(name)?;

    let project_path = preset_file_path(&project_profiles_dir(project_dir), name);
    if project_path.exists() {
        return Ok(Some(ResolvedPreset {
            name: name.to_string(),
            path: project_path,
            source: PresetSource::Project,
        }));
    }

    let global_path = preset_file_path(&global_profiles_dir()?, name);
    if global_path.exists() {
        return Ok(Some(ResolvedPreset {
            name: name.to_string(),
            path: global_path,
            source: PresetSource::Global,
        }));
    }

    let runtime_path = runtime_metadata::profile_ini_path(runtime_dir, name);
    if runtime_path.exists() {
        return Ok(Some(ResolvedPreset {
            name: name.to_string(),
            path: runtime_path,
            source: PresetSource::Runtime,
        }));
    }

    Ok(None)
}

/// Resolve a preset by name following project → global → runtime → materialize order.
pub fn resolve_preset(
    name: &str,
    project_dir: &Utf8Path,
    runtime_dir: &Utf8Path,
    manifest: Option<&Manifest>,
) -> Result<ResolvedPreset> {
    validate_profile_name(name)?;

    if let Some(preset) = find_existing_preset(name, project_dir, runtime_dir)? {
        return Ok(preset);
    }

    let global_path = preset_file_path(&global_profiles_dir()?, name);
    let content = starter_content(name, manifest)?;
    materialize_starter_if_missing(&global_path, &content)?;
    Ok(ResolvedPreset {
        name: name.to_string(),
        path: global_path,
        source: PresetSource::Bundled,
    })
}

/// Strip load directives before writing a preset to PHPVM's managed INI file.
///
/// Static runtimes compile their catalog into the binary, so profiles must not
/// attempt to load host or user-provided extension libraries.
pub(crate) fn strip_extension_load_directives(ini_contents: &str) -> String {
    let mut lines: Vec<&str> = ini_contents
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("extension=") && !trimmed.starts_with("zend_extension=")
        })
        .collect();
    if lines.last().is_some_and(|line| !line.is_empty()) {
        lines.push("");
    }
    lines.join("\n")
}

/// Collect all known preset names from project, global, runtime, and builtins.
pub fn discover_presets(
    project_dir: &Utf8Path,
    runtime_dir: Option<&Utf8Path>,
) -> Result<Vec<ListedPreset>> {
    let mut seen = BTreeSet::new();
    let mut listed = Vec::new();

    let mut add_dir = |dir: &Utf8Path, source: PresetSource| -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir.as_std_path())
            .with_context(|| format!("Failed to read profile preset directory {}", dir))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ini") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if seen.insert(stem.to_string()) {
                listed.push(ListedPreset {
                    name: stem.to_string(),
                    path: Utf8PathBuf::from_path_buf(path)
                        .map_err(|p| anyhow::anyhow!("Invalid UTF-8 path: {:?}", p))?
                        .to_string(),
                    source: source.as_str().to_string(),
                });
            }
        }
        Ok(())
    };

    add_dir(&project_profiles_dir(project_dir), PresetSource::Project)?;
    add_dir(&global_profiles_dir()?, PresetSource::Global)?;
    if let Some(runtime_dir) = runtime_dir {
        add_dir(
            &runtime_metadata::profiles_ini_dir(runtime_dir),
            PresetSource::Runtime,
        )?;
    }

    for name in BUILTIN_NAMES {
        if seen.insert((*name).to_string()) {
            listed.push(ListedPreset {
                name: (*name).to_string(),
                path: format!("(bundled starter: {name}.ini)"),
                source: PresetSource::Bundled.as_str().to_string(),
            });
        }
    }

    listed.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(listed)
}

/// Open a preset in the user's editor.
pub fn edit_preset(path: &Utf8Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let mut cmd = std::process::Command::new(program);
    for arg in parts {
        cmd.arg(arg);
    }
    cmd.arg(path.as_str());

    let status = cmd
        .status()
        .with_context(|| format!("Failed to launch editor '{editor}'"))?;

    if !status.success() {
        anyhow::bail!("Editor exited with status {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn utf8(dir: &TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap()
    }

    #[test]
    fn strip_extension_load_directives_removes_load_directives() {
        let ini = "; preset\nextension=curl\nmemory_limit = 256M\nzend_extension=opcache\n";
        let filtered = strip_extension_load_directives(ini);
        assert!(filtered.contains("; preset"));
        assert!(filtered.contains("memory_limit = 256M"));
        assert!(!filtered.contains("extension=curl"));
        assert!(!filtered.contains("zend_extension=opcache"));
    }

    #[test]
    fn materialize_starter_if_missing_does_not_overwrite() -> Result<()> {
        let dir = TempDir::new()?;
        let path = dir.path().join("test.ini");
        let utf8_path = Utf8PathBuf::from_path_buf(path.clone()).unwrap();
        fs::write(&path, "original")?;
        assert!(!materialize_starter_if_missing(&utf8_path, "new")?);
        assert_eq!(fs::read_to_string(&path)?, "original");
        Ok(())
    }

    #[test]
    fn validate_profile_name_rejects_path_separators() {
        assert!(validate_profile_name("../evil").is_err());
        assert!(validate_profile_name("good-name").is_ok());
    }

    #[test]
    fn resolve_preset_prefers_project_over_global() -> Result<()> {
        let project = TempDir::new()?;
        let project_dir = utf8(&project);
        let profiles_dir = project_profiles_dir(&project_dir);
        fs::create_dir_all(&profiles_dir)?;
        let project_ini = profiles_dir.join("custom.ini");
        fs::write(&project_ini, "; project")?;

        let runtime = TempDir::new()?;
        let runtime_dir = utf8(&runtime);

        let resolved = resolve_preset("custom", &project_dir, &runtime_dir, None)?;
        assert_eq!(resolved.source, PresetSource::Project);
        assert_eq!(resolved.path, project_ini);
        Ok(())
    }
}
