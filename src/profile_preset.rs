use std::collections::BTreeSet;
use std::fs;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::config;
use crate::manifest::{Manifest, ManifestEntry, RuntimeExtension};
use crate::output;
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
const DEFAULT_DYNAMIC_EXTENSIONS: &[&str] = &["openssl", "phar", "mbstring"];

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
pub fn starter_content(
    name: &str,
    manifest: Option<&Manifest>,
    catalog: &[String],
) -> Result<String> {
    if let Some(content) = bundled_starter_content(name) {
        return Ok(content.to_string());
    }

    if let Some(mf) = manifest {
        if let Some(template) = mf.resolve_profile_template(name) {
            return Ok(render_template_ini(&template, catalog));
        }
    }

    anyhow::bail!(
        "No profile preset '{}' found. Create one with `phpvm profile new {}` \
         or add .phpvm/profiles/{}.ini",
        name,
        name,
        name
    )
}

fn render_template_ini(template: &ProfileTemplate, catalog: &[String]) -> String {
    let mut lines = vec![
        format!("; PHPVM profile preset: {}", template.name),
        "; Generated from manifest template.".to_string(),
        String::new(),
    ];

    let enabled: BTreeSet<&str> = template.extensions.iter().map(String::as_str).collect();
    for ext in catalog {
        if enabled.contains(ext.as_str()) {
            lines.push(format!("extension={ext}"));
        } else {
            lines.push(format!(";extension={ext}"));
        }
    }
    lines.push(String::new());
    lines.join("\n")
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
    catalog: &[String],
) -> Result<ResolvedPreset> {
    validate_profile_name(name)?;

    if let Some(preset) = find_existing_preset(name, project_dir, runtime_dir)? {
        return Ok(preset);
    }

    let global_path = preset_file_path(&global_profiles_dir()?, name);
    let content = starter_content(name, manifest, catalog)?;
    materialize_starter_if_missing(&global_path, &content)?;
    Ok(ResolvedPreset {
        name: name.to_string(),
        path: global_path,
        source: PresetSource::Bundled,
    })
}

/// Copy a preset file to the runtime's active `etc/php.ini`.
pub fn activate_preset(runtime_dir: &Utf8Path, preset_path: &Utf8Path) -> Result<()> {
    let etc_dir = runtime_dir.join("etc");
    fs::create_dir_all(&etc_dir)
        .with_context(|| format!("Failed to create etc directory {}", etc_dir))?;

    let active_ini = runtime_metadata::active_php_ini(runtime_dir);
    let preset_contents = fs::read_to_string(preset_path)
        .with_context(|| format!("Failed to read profile preset {}", preset_path))?;
    let filtered = filter_non_extension_ini_lines(&preset_contents);
    fs::write(&active_ini, filtered)
        .with_context(|| format!("Failed to write active php.ini {}", active_ini))?;
    Ok(())
}

fn filter_non_extension_ini_lines(ini_contents: &str) -> String {
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

/// Apply a preset to a dynamic runtime by writing a generated profile scan-dir snippet.
pub fn activate_dynamic_preset(
    runtime_dir: &Utf8Path,
    preset_path: &Utf8Path,
    entry: &ManifestEntry,
) -> Result<Vec<String>> {
    ensure_dynamic_ini_layout(runtime_dir, entry)?;

    let preset_contents = fs::read_to_string(preset_path)
        .with_context(|| format!("Failed to read profile preset {}", preset_path))?;
    let enabled = parse_enabled_extensions(&preset_contents);
    validate_dynamic_extensions(&enabled, entry)?;

    let mut generated = vec![
        "; Generated by phpvm from the active profile preset.".to_string(),
        format!("; Source: {preset_path}"),
        String::new(),
    ];

    for line in preset_contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("extension=") || trimmed.starts_with("zend_extension=") {
            continue;
        }
        generated.push(line.to_string());
    }

    if !generated.last().is_some_and(|line| line.is_empty()) {
        generated.push(String::new());
    }

    for name in &enabled {
        if DEFAULT_DYNAMIC_EXTENSIONS.contains(&name.as_str()) {
            continue;
        }
        let ext = entry.extension_detail(name).ok_or_else(|| {
            anyhow::anyhow!(
                "Profile {} enables extension '{}' not bundled in PHP {}",
                preset_path,
                name,
                entry.php
            )
        })?;
        generated.push(dynamic_load_line(runtime_dir, ext));
    }
    generated.push(String::new());

    let profile_ini = runtime_metadata::conf_d_dir(runtime_dir).join("20-profile.ini");
    fs::write(&profile_ini, generated.join("\n"))
        .with_context(|| format!("Failed to write generated profile ini {}", profile_ini))?;

    Ok(enabled)
}

fn dynamic_load_line(runtime_dir: &Utf8Path, ext: &RuntimeExtension) -> String {
    let target = ext.load_target();
    let path = if target.contains('/') || target.contains('\\') {
        runtime_dir.join(target)
    } else {
        runtime_metadata::extension_dir(runtime_dir).join(target)
    };
    format!("{}={}", ext.load_directive(), path)
}

fn ensure_dynamic_ini_layout(runtime_dir: &Utf8Path, entry: &ManifestEntry) -> Result<()> {
    let etc_dir = runtime_dir.join("etc");
    let conf_d = runtime_metadata::conf_d_dir(runtime_dir);
    fs::create_dir_all(&conf_d).with_context(|| format!("Failed to create {}", conf_d))?;

    let php_ini = runtime_metadata::active_php_ini(runtime_dir);
    let ext_dir = runtime_metadata::extension_dir(runtime_dir);
    fs::create_dir_all(&etc_dir).with_context(|| format!("Failed to create {}", etc_dir))?;

    let contents = if php_ini.exists() {
        fs::read_to_string(&php_ini)
            .with_context(|| format!("Failed to read base php.ini {}", php_ini))?
    } else {
        "; Generated by phpvm for this dynamic runtime.\n".to_string()
    };
    let contents = upsert_ini_setting(&contents, "extension_dir", &format!("\"{}\"", ext_dir));
    fs::write(&php_ini, contents)
        .with_context(|| format!("Failed to write base php.ini {}", php_ini))?;

    let default_ini = conf_d.join("00-default.ini");
    let mut default_lines = vec![
        "; Generated by phpvm for dynamic runtime defaults.".to_string(),
        "; Required for bundled Composer and secure PHAR workflows.".to_string(),
    ];
    for name in DEFAULT_DYNAMIC_EXTENSIONS {
        if let Some(ext) = entry.extension_detail(name) {
            default_lines.push(dynamic_load_line(runtime_dir, ext));
        }
    }
    default_lines.push(String::new());
    fs::write(&default_ini, default_lines.join("\n"))
        .with_context(|| format!("Failed to write default extension ini {}", default_ini))?;

    Ok(())
}

fn upsert_ini_setting(contents: &str, key: &str, value: &str) -> String {
    let mut replaced = false;
    let mut lines = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(';') && trimmed.starts_with(key) {
            let rest = trimmed[key.len()..].trim_start();
            if rest.starts_with('=') {
                lines.push(format!("{key} = {value}"));
                replaced = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }
    if !replaced {
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("{key} = {value}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn validate_dynamic_extensions(enabled: &[String], entry: &ManifestEntry) -> Result<()> {
    for name in enabled {
        let Some(ext) = entry.extension_detail(name) else {
            anyhow::bail!(
                "Profile enables extension '{}' but PHP {} does not bundle it",
                name,
                entry.php
            );
        };
        validate_dynamic_extension_file(ext)?;
    }
    Ok(())
}

fn validate_dynamic_extension_file(ext: &RuntimeExtension) -> Result<()> {
    if ext.load_target().is_empty() {
        anyhow::bail!("Extension '{}' has an empty load target", ext.name);
    }
    Ok(())
}

/// Parse enabled extension names from ini contents.
pub fn parse_enabled_extensions(ini_contents: &str) -> Vec<String> {
    let mut extensions = Vec::new();
    for line in ini_contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("extension=") {
            let ext = normalize_extension_token(rest);
            if !ext.is_empty() && !extensions.iter().any(|e| e == &ext) {
                extensions.push(ext);
            }
        } else if let Some(rest) = trimmed.strip_prefix("zend_extension=") {
            let ext = normalize_extension_token(rest);
            if !ext.is_empty() && !extensions.iter().any(|e| e == &ext) {
                extensions.push(ext);
            }
        }
    }
    extensions
}

fn normalize_extension_token(value: &str) -> String {
    let token = value
        .split_whitespace()
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    let file_name = token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .strip_suffix(".so")
        .or_else(|| {
            token
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(token)
                .strip_suffix(".dll")
        })
        .or_else(|| {
            token
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(token)
                .strip_suffix(".dylib")
        })
        .unwrap_or_else(|| token.rsplit(['/', '\\']).next().unwrap_or(token));
    file_name.to_string()
}

/// Parse enabled extensions from a preset file on disk.
pub fn parse_enabled_extensions_from_file(path: &Utf8Path) -> Result<Vec<String>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("Failed to read preset {}", path))?;
    Ok(parse_enabled_extensions(&contents))
}

/// Warn when a preset enables extensions not compiled into the runtime catalog.
pub fn validate_preset_extensions(preset_path: &Utf8Path, catalog: &[String]) -> Result<()> {
    if catalog.is_empty() {
        return Ok(());
    }

    let enabled = parse_enabled_extensions_from_file(preset_path)?;
    let catalog_set: BTreeSet<&str> = catalog.iter().map(String::as_str).collect();
    let missing: Vec<&str> = enabled
        .iter()
        .filter_map(|ext| {
            let base = ext.strip_suffix(".so").unwrap_or(ext.as_str());
            if catalog_set.contains(base) || catalog_set.contains(ext.as_str()) {
                None
            } else {
                Some(base)
            }
        })
        .collect();

    if !missing.is_empty() {
        output::warn(&format!(
            "Preset {} enables extensions not in this runtime: {}",
            preset_path,
            missing.join(", ")
        ));
    }
    Ok(())
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

/// Create a new preset file in project or global directory.
pub fn create_preset(
    name: &str,
    project_dir: &Utf8Path,
    global: bool,
    from_template: Option<&str>,
    manifest: Option<&Manifest>,
    catalog: &[String],
) -> Result<Utf8PathBuf> {
    validate_profile_name(name)?;
    if let Some(template) = from_template {
        validate_profile_name(template)?;
    }

    let dir = if global {
        global_profiles_dir()?
    } else {
        project_profiles_dir(project_dir)
    };
    let dest = preset_file_path(&dir, name);
    if dest.exists() {
        anyhow::bail!("Profile preset already exists: {}", dest);
    }

    let template_name = from_template.unwrap_or("minimal");
    let content = starter_content(template_name, manifest, catalog)?;
    let header = format!("; PHPVM profile preset: {name}");
    let body = content
        .lines()
        .skip_while(|line| line.starts_with("; PHPVM profile preset:"))
        .collect::<Vec<_>>()
        .join("\n");
    let final_content = if body.is_empty() {
        header
    } else {
        format!("{header}\n{body}")
    };
    materialize_starter_if_missing(&dest, &final_content)?;
    Ok(dest)
}

/// Fork an existing preset into the project profiles directory.
pub fn fork_preset(
    src: &str,
    dst: &str,
    project_dir: &Utf8Path,
    runtime_dir: Option<&Utf8Path>,
    manifest: Option<&Manifest>,
    catalog: &[String],
) -> Result<Utf8PathBuf> {
    validate_profile_name(src)?;
    validate_profile_name(dst)?;

    let resolved = resolve_preset(
        src,
        project_dir,
        runtime_dir.unwrap_or_else(|| Utf8Path::new("/nonexistent")),
        manifest,
        catalog,
    )?;
    let dest_path = preset_file_path(&project_profiles_dir(project_dir), dst);
    if dest_path.exists() {
        anyhow::bail!("Profile preset already exists: {}", dest_path);
    }
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&resolved.path, &dest_path)
        .with_context(|| format!("Failed to copy {} to {}", resolved.path, dest_path))?;
    Ok(dest_path)
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
    fn filter_non_extension_ini_lines_strips_load_directives() {
        let ini = "; preset\nextension=curl\nmemory_limit = 256M\nzend_extension=opcache\n";
        let filtered = filter_non_extension_ini_lines(ini);
        assert!(filtered.contains("; preset"));
        assert!(filtered.contains("memory_limit = 256M"));
        assert!(!filtered.contains("extension=curl"));
        assert!(!filtered.contains("zend_extension=opcache"));
    }

    #[test]
    fn parse_enabled_extensions_reads_extension_lines() {
        let ini = r#"
; comment
extension=curl
extension=mbstring
;extension=gd
zend_extension=xdebug
"#;
        let exts = parse_enabled_extensions(ini);
        assert!(exts.contains(&"curl".to_string()));
        assert!(exts.contains(&"mbstring".to_string()));
        assert!(!exts.contains(&"gd".to_string()));
        assert!(exts.contains(&"xdebug".to_string()));
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

        let resolved = resolve_preset("custom", &project_dir, &runtime_dir, None, &[])?;
        assert_eq!(resolved.source, PresetSource::Project);
        assert_eq!(resolved.path, project_ini);
        Ok(())
    }

    #[test]
    fn activate_dynamic_preset_refreshes_base_ini_paths() -> Result<()> {
        let runtime = TempDir::new()?;
        let runtime_dir = utf8(&runtime);
        let etc_dir = runtime_dir.join("etc");
        fs::create_dir_all(etc_dir.join("conf.d"))?;
        fs::write(
            etc_dir.join("php.ini"),
            "display_errors = On\nextension_dir = \"/tmp/staging/ext\"\n",
        )?;

        let preset = runtime_dir.join("preset.ini");
        fs::write(&preset, "extension=mbstring\nextension=curl\n")?;

        let entry = ManifestEntry {
            php: "8.3.31".into(),
            composer: "2.9.2".into(),
            profile: None,
            runtime_type: crate::manifest::RuntimeType::Dynamic,
            abi: None,
            thread_safety: None,
            extension_api: None,
            extensions: vec![
                RuntimeExtension::from_name("openssl".into()),
                RuntimeExtension::from_name("phar".into()),
                RuntimeExtension::from_name("mbstring".into()),
                RuntimeExtension::from_name("curl".into()),
            ],
            url: "https://example.com/php-8.3.31.tar.gz".into(),
            sha256: "00000000000000000000000000000000000000000000000000000000000000ab".into(),
            artifacts: None,
        };

        let enabled = activate_dynamic_preset(&runtime_dir, &preset, &entry)?;

        assert_eq!(enabled, vec!["mbstring".to_string(), "curl".to_string()]);

        let base_ini = fs::read_to_string(runtime_metadata::active_php_ini(&runtime_dir))?;
        assert!(base_ini.contains(&format!(
            "extension_dir = \"{}\"",
            runtime_metadata::extension_dir(&runtime_dir)
        )));
        assert!(!base_ini.contains("/tmp/staging/ext"));

        let default_ini =
            fs::read_to_string(runtime_metadata::conf_d_dir(&runtime_dir).join("00-default.ini"))?;
        assert!(default_ini.contains("openssl.so"));
        assert!(default_ini.contains("phar.so"));
        assert!(default_ini.contains("mbstring.so"));

        let profile_ini =
            fs::read_to_string(runtime_metadata::conf_d_dir(&runtime_dir).join("20-profile.ini"))?;
        assert!(profile_ini.contains("curl.so"));
        assert!(!profile_ini.contains("mbstring.so"));

        Ok(())
    }
}
