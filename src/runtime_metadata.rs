use std::fs;

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::config;
use crate::manifest::{ManifestEntry, RuntimeExtension, RuntimeType};
use crate::profile_preset::{self, PresetSource};

/// Metadata written alongside an installed runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetadata {
    pub php: String,
    pub composer: String,
    /// Active profile preset (ini config).
    #[serde(alias = "profile")]
    pub active_profile: String,
    /// Path or source label for the active preset.
    #[serde(default)]
    pub preset_source: Option<String>,
    /// Runtime packaging model.
    #[serde(default)]
    pub runtime_type: RuntimeType,
    /// Extension ABI metadata for dynamic runtimes.
    #[serde(default)]
    pub abi: Option<String>,
    #[serde(default)]
    pub thread_safety: Option<String>,
    #[serde(default)]
    pub extension_api: Option<String>,
    /// Extension descriptors available in this runtime.
    #[serde(default)]
    pub extension_catalog: Vec<RuntimeExtension>,
    /// Extension names available in the runtime (legacy-friendly summary).
    #[serde(default)]
    pub available_extensions: Vec<String>,
    /// Extensions currently enabled via the active profile ini.
    #[serde(default, alias = "extensions")]
    pub enabled_extensions: Vec<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
}

impl RuntimeMetadata {
    pub fn read(resolved: &str) -> Result<Option<Self>> {
        let path = config::runtimes_dir()?.join(resolved).join("metadata.json");
        if !path.exists() {
            return Ok(None);
        }
        let contents =
            fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path))?;
        let metadata: Self =
            serde_json::from_str(&contents).with_context(|| format!("Failed to parse {}", path))?;
        Ok(Some(metadata))
    }

    pub fn read_active_profile(resolved: &str) -> Result<Option<String>> {
        Ok(Self::read(resolved)?.map(|m| m.active_profile))
    }

    pub fn write(&self, runtime_dir: &camino::Utf8Path) -> Result<()> {
        let path = runtime_dir.join("metadata.json");
        let json = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize runtime metadata")?;
        fs::write(&path, json).with_context(|| format!("Failed to write {}", path))?;
        Ok(())
    }

    pub fn from_install(
        entry: &ManifestEntry,
        profile_name: &str,
        preset: &profile_preset::ResolvedPreset,
        catalog: &[String],
    ) -> Self {
        let enabled =
            profile_preset::parse_enabled_extensions_from_file(&preset.path).unwrap_or_default();
        Self {
            php: entry.php.clone(),
            composer: entry.composer.clone(),
            active_profile: profile_name.to_string(),
            preset_source: Some(format_preset_source(preset)),
            runtime_type: entry.runtime_type.clone(),
            abi: entry.abi.clone(),
            thread_safety: entry.thread_safety.clone(),
            extension_api: entry.extension_api.clone(),
            extension_catalog: entry.extensions.clone(),
            available_extensions: catalog.to_vec(),
            enabled_extensions: enabled,
            installed_at: Some(iso8601_now()),
        }
    }

    pub fn update_active_preset(
        &mut self,
        profile_name: &str,
        preset: &profile_preset::ResolvedPreset,
    ) {
        self.active_profile = profile_name.to_string();
        self.preset_source = Some(format_preset_source(preset));
        if let Ok(enabled) = profile_preset::parse_enabled_extensions_from_file(&preset.path) {
            self.enabled_extensions = enabled;
        }
    }
}

fn format_preset_source(preset: &profile_preset::ResolvedPreset) -> String {
    match preset.source {
        PresetSource::Project | PresetSource::Global | PresetSource::Runtime => {
            preset.path.to_string()
        }
        PresetSource::Bundled => preset.source.as_str().to_string(),
    }
}

/// Return the current UTC time as an ISO 8601 string.
pub fn iso8601_now() -> String {
    use std::time::SystemTime;

    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => {
            let secs = dur.as_secs();
            let days = secs / 86400;
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;
            let seconds = time_of_day % 60;

            let (y, m, d) = civil_from_days(days as i64);

            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                y, m, d, hours, minutes, seconds
            )
        }
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// Return the path to the active php.ini for a runtime.
pub fn active_php_ini(runtime_dir: &camino::Utf8Path) -> Utf8PathBuf {
    runtime_dir.join("etc").join("php.ini")
}

/// Return the additional ini scan directory for generated profile/extension snippets.
pub fn conf_d_dir(runtime_dir: &camino::Utf8Path) -> Utf8PathBuf {
    runtime_dir.join("etc").join("conf.d")
}

/// Return the directory holding profile ini presets.
pub fn profiles_ini_dir(runtime_dir: &camino::Utf8Path) -> Utf8PathBuf {
    runtime_dir.join("etc").join("profiles")
}

/// Return the path to a named profile preset ini file.
pub fn profile_ini_path(runtime_dir: &camino::Utf8Path, profile_name: &str) -> Utf8PathBuf {
    profiles_ini_dir(runtime_dir).join(format!("{profile_name}.ini"))
}

/// Directory under which phpvm writes a copy of the active profile preset for
/// static runtimes (outside the runtime tree so the tarball stays minimal).
/// `~/.phpvm/ini/`. Used to drive PHPRC for bare `php` after `phpvm use`.
pub fn phpvm_managed_ini_dir() -> Result<Utf8PathBuf> {
    Ok(config::data_dir()?.join("ini"))
}

/// Path to the phpvm-managed ini file for a resolved PHP version (static model).
/// e.g. `~/.phpvm/ini/8.3.31.ini`. The parent dir is created on first materialize.
pub fn managed_ini_for_version(version: &str) -> Result<Utf8PathBuf> {
    phpvm_managed_ini_dir().map(|d| d.join(format!("{version}.ini")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_metadata_deserializes_legacy_profile_field() {
        let json = r#"{
            "php": "8.3.23",
            "composer": "2.9.2",
            "profile": "wordpress",
            "extensions": ["curl", "mbstring"]
        }"#;
        let metadata: RuntimeMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.active_profile, "wordpress");
        assert_eq!(metadata.enabled_extensions, vec!["curl", "mbstring"]);
    }

    #[test]
    fn managed_ini_paths_are_under_data_dir_ini() {
        // We can't easily assert the exact home without env hacking, but we can
        // assert structure and that it is absolute + contains "ini".
        let dir = phpvm_managed_ini_dir().unwrap();
        assert!(dir.as_str().contains("ini"));
        let ver = managed_ini_for_version("8.3.31").unwrap();
        assert!(ver.as_str().ends_with("8.3.31.ini"));
        assert!(ver.as_str().contains("ini"));
    }
}
