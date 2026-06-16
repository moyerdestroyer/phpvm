use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};

use crate::config;

/// A project-local PHP version declaration discovered while walking up from CWD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPin {
    pub version_spec: String,
    pub project_dir: Utf8PathBuf,
    pub source: ProjectPinSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPinSource {
    PhpvmVersion,
    PhpvmToml,
}

/// Walk up from `start` looking for `.phpvm-version` or `version` in `.phpvm.toml`.
pub fn find_project_pin(start: &Utf8Path) -> Result<Option<ProjectPin>> {
    let mut dir = start.to_path_buf();

    loop {
        let version_file = dir.join(".phpvm-version");
        if version_file.is_file() {
            let version_spec = read_phpvm_version_file(&version_file)?;
            return Ok(Some(ProjectPin {
                version_spec,
                project_dir: dir,
                source: ProjectPinSource::PhpvmVersion,
            }));
        }

        let toml = dir.join(".phpvm.toml");
        if toml.is_file() {
            let cfg = config::load_config(&dir)?;
            if let Some(version_spec) = cfg.version.filter(|v| !v.is_empty()) {
                return Ok(Some(ProjectPin {
                    version_spec,
                    project_dir: dir,
                    source: ProjectPinSource::PhpvmToml,
                }));
            }
        }

        if !dir.pop() {
            break;
        }
    }

    Ok(None)
}

fn read_phpvm_version_file(path: &Utf8Path) -> Result<String> {
    let content = std::fs::read_to_string(path.as_std_path())
        .with_context(|| format!("Failed to read {}", path))?;

    for line in content.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    bail!("{path} is empty (expected a PHP version specifier, e.g. 8.3)");
}

/// Resolve which version specifier `phpvm use` should activate.
pub fn resolve_use_spec(explicit: Option<&str>) -> Result<String> {
    if let Some(spec) = explicit.filter(|s| !s.is_empty()) {
        return Ok(spec.to_string());
    }

    let project_dir = config::current_project_dir()?;
    if let Some(pin) = find_project_pin(&project_dir)? {
        return Ok(pin.version_spec);
    }

    if let Some(current) = config::get_current_version() {
        return Ok(current);
    }

    bail!(
        "No PHP version specified. Pass a version (e.g. `phpvm use 8.3`), add a \
         `.phpvm-version` file, set `version` in `.phpvm.toml`, or run `phpvm use` after \
         activating a version once."
    );
}

/// Build the shell export snippet for activating a specific resolved runtime.
pub fn build_activation_snippet(resolved: &str, runtime_path: &Utf8Path) -> Result<String> {
    let bin_dir = runtime_path.join("bin");
    let composer_home = crate::version::composer_home_for(resolved)?;
    let global_bin = composer_home.join("vendor").join("bin");
    let data_dir = config::data_dir()?;
    let runtimes_dir = config::runtimes_dir()?;
    let composer_homes_dir = data_dir.join("composer-homes");

    let phprc_export = effective_phprc(runtime_path, resolved)
        .map(|dir| format!("export PHPRC={}\n", shell_quote(&dir)))
        .unwrap_or_default();
    let scan_dir_export = effective_scan_dir(runtime_path)
        .map(|dir| format!("export PHP_INI_SCAN_DIR={}\n", shell_quote(&dir)))
        .unwrap_or_default();

    Ok(format!(
        r#"export PHPVM_VERSION={}
export COMPOSER_HOME={}
__phpvm_runtime_bin={}
__phpvm_composer_bin={}
__phpvm_runtimes_dir={}
__phpvm_composer_homes_dir={}
__phpvm_old_path="${{PATH:-}}"
__phpvm_new_path=""
__phpvm_old_ifs="$IFS"
case "$-" in
  *f*) __phpvm_had_noglob=1 ;;
  *) __phpvm_had_noglob=0; set -f ;;
esac
IFS=":"
for __phpvm_entry in $__phpvm_old_path; do
  case "$__phpvm_entry" in
    "$__phpvm_runtimes_dir"/*/bin|"$__phpvm_composer_homes_dir"/*/vendor/bin) ;;
    "$HOME/.composer/vendor/bin"|"$HOME/.config/composer/vendor/bin") ;;
    *) __phpvm_new_path="${{__phpvm_new_path:+$__phpvm_new_path:}}$__phpvm_entry" ;;
  esac
done
IFS="$__phpvm_old_ifs"
if [ "$__phpvm_had_noglob" = 0 ]; then
  set +f
fi
export PATH="$__phpvm_runtime_bin:$__phpvm_composer_bin${{__phpvm_new_path:+:$__phpvm_new_path}}"
unset __phpvm_runtime_bin __phpvm_composer_bin __phpvm_runtimes_dir
unset __phpvm_composer_homes_dir __phpvm_old_path __phpvm_new_path
unset __phpvm_old_ifs __phpvm_had_noglob __phpvm_entry
hash -r 2>/dev/null || true
{}{}"#,
        shell_quote(resolved),
        shell_quote(composer_home.as_str()),
        shell_quote(bin_dir.as_str()),
        shell_quote(global_bin.as_str()),
        shell_quote(runtimes_dir.as_str()),
        shell_quote(composer_homes_dir.as_str()),
        phprc_export,
        scan_dir_export
    ))
}

/// Build the shell snippet that undoes `phpvm use` in the current shell.
pub fn build_deactivation_snippet() -> Result<String> {
    let data_dir = config::data_dir()?;
    let runtimes_dir = config::runtimes_dir()?;
    let composer_homes_dir = data_dir.join("composer-homes");

    Ok(format!(
        r#"__phpvm_runtimes_dir={}
__phpvm_composer_homes_dir={}
__phpvm_old_path="${{PATH:-}}"
__phpvm_new_path=""
__phpvm_old_ifs="$IFS"
case "$-" in
  *f*) __phpvm_had_noglob=1 ;;
  *) __phpvm_had_noglob=0; set -f ;;
esac
IFS=":"
for __phpvm_entry in $__phpvm_old_path; do
  case "$__phpvm_entry" in
    "$__phpvm_runtimes_dir"/*/bin|"$__phpvm_composer_homes_dir"/*/vendor/bin) ;;
    *) __phpvm_new_path="${{__phpvm_new_path:+$__phpvm_new_path:}}$__phpvm_entry" ;;
  esac
done
IFS="$__phpvm_old_ifs"
if [ "$__phpvm_had_noglob" = 0 ]; then
  set +f
fi
export PATH="${{__phpvm_new_path:-}}"
unset PHPVM_VERSION COMPOSER_HOME PHPRC PHP_INI_SCAN_DIR
unset __phpvm_runtimes_dir __phpvm_composer_homes_dir __phpvm_old_path
unset __phpvm_new_path __phpvm_old_ifs __phpvm_had_noglob __phpvm_entry
hash -r 2>/dev/null || true
"#,
        shell_quote(runtimes_dir.as_str()),
        shell_quote(composer_homes_dir.as_str()),
    ))
}

/// Emit warnings when host PHP / Composer settings may conflict with phpvm activation.
pub fn warn_activation_conflicts(resolved: &str, silent: bool) {
    if silent {
        return;
    }

    let data_dir = match config::data_dir() {
        Ok(dir) => dir,
        Err(_) => return,
    };

    if let Ok(composer_home) = std::env::var("COMPOSER_HOME") {
        if !composer_home.is_empty() && !composer_home.starts_with(data_dir.as_str()) {
            eprintln!(
                "warning: COMPOSER_HOME is set to {composer_home} (outside phpvm). \
                 `phpvm use {resolved}` will replace it with the phpvm-managed home."
            );
        }
    }

    if let Ok(phprc) = std::env::var("PHPRC") {
        let runtimes_dir = data_dir.join("runtimes");
        if !phprc.is_empty() && !phprc.starts_with(runtimes_dir.as_str()) {
            eprintln!(
                "warning: PHPRC is set to {phprc} (outside phpvm). \
                 `phpvm use {resolved}` will replace it for this shell."
            );
        }
    }

    if let Ok(path) = std::env::var("PATH") {
        let host_bins = [
            format!("{}/.composer/vendor/bin", home_dir_display()),
            format!("{}/.config/composer/vendor/bin", home_dir_display()),
        ];
        for host_bin in host_bins {
            if path.split(':').any(|entry| entry == host_bin) {
                eprintln!(
                    "warning: host Composer global bin `{host_bin}` is on PATH; \
                     phpvm removes it while active (use `phpvm deactivate` to restore)."
                );
            }
        }
    }
}

fn home_dir_display() -> String {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_string_lossy().to_string())
        .unwrap_or_else(|| "~".to_string())
}

/// Return PHPRC dir for the runtime.
///
/// Baseline (static): use the phpvm-managed ini under ~/.phpvm/ini/<ver>.ini
/// (materialized from the active profile preset).
///
/// Compat: if an old runtime still has etc/php.ini on disk (pre-minimal-static
/// layout), prefer that.
fn effective_phprc(runtime_path: &Utf8Path, resolved: &str) -> Option<String> {
    let php_ini = crate::runtime_metadata::active_php_ini(runtime_path);
    if php_ini.exists() {
        return php_ini.parent().map(|dir| dir.to_string());
    }
    if let Ok(managed) = crate::runtime_metadata::managed_ini_for_version(resolved) {
        if managed.exists() {
            return managed.parent().map(|dir| dir.to_string());
        }
    }
    None
}

/// SCAN_DIR is only relevant for old runtimes that had a conf.d/ layout.
fn effective_scan_dir(runtime_path: &Utf8Path) -> Option<String> {
    let conf_d = crate::runtime_metadata::conf_d_dir(runtime_path);
    if conf_d.exists() {
        Some(conf_d.to_string())
    } else {
        None
    }
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::env_lock::LOCK as ENV_LOCK;
    use std::ffi::OsString;
    use std::io::Write;

    struct EnvSnapshot {
        phpvm_home: Option<OsString>,
    }

    impl EnvSnapshot {
        fn capture() -> Self {
            Self {
                phpvm_home: std::env::var_os("PHPVM_HOME"),
            }
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            if let Some(value) = &self.phpvm_home {
                std::env::set_var("PHPVM_HOME", value);
            } else {
                std::env::remove_var("PHPVM_HOME");
            }
        }
    }

    fn write_file(dir: &camino::Utf8Path, name: &str, contents: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent.as_std_path()).unwrap();
        }
        let mut f = std::fs::File::create(path.as_std_path()).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn find_project_pin_reads_phpvm_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        write_file(&root, ".phpvm-version", "8.3\n");

        let pin = find_project_pin(&root).unwrap().unwrap();
        assert_eq!(pin.version_spec, "8.3");
        assert_eq!(pin.source, ProjectPinSource::PhpvmVersion);
    }

    #[test]
    fn find_project_pin_prefers_phpvm_version_over_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        write_file(&root, ".phpvm-version", "8.2\n");
        write_file(&root, ".phpvm.toml", r#"version = "8.3""#);

        let pin = find_project_pin(&root).unwrap().unwrap();
        assert_eq!(pin.version_spec, "8.2");
    }

    #[test]
    fn find_project_pin_reads_version_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        write_file(&root, ".phpvm.toml", r#"version = "8.4""#);

        let pin = find_project_pin(&root).unwrap().unwrap();
        assert_eq!(pin.version_spec, "8.4");
        assert_eq!(pin.source, ProjectPinSource::PhpvmToml);
    }

    #[test]
    fn activation_snippet_strips_host_composer_and_runs_hash_r() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture();
        let dir = tempfile::tempdir().unwrap();
        let home = Utf8PathBuf::from_path_buf(dir.path().join(".phpvm")).unwrap();
        std::env::set_var("PHPVM_HOME", home.as_str());

        let runtime = home.join("runtimes").join("8.3.12");
        std::fs::create_dir_all(runtime.join("bin")).unwrap();
        std::fs::create_dir_all(runtime.join("etc")).unwrap();
        std::fs::File::create(runtime.join("etc").join("php.ini")).unwrap();

        let snippet = build_activation_snippet("8.3.12", &runtime).unwrap();
        assert!(snippet.contains(r#""$HOME/.composer/vendor/bin""#));
        assert!(snippet.contains("hash -r"));
    }

    #[test]
    fn deactivation_snippet_unsets_env_and_strips_paths() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvSnapshot::capture();
        let dir = tempfile::tempdir().unwrap();
        let home = Utf8PathBuf::from_path_buf(dir.path().join(".phpvm")).unwrap();
        std::env::set_var("PHPVM_HOME", home.as_str());

        let snippet = build_deactivation_snippet().unwrap();
        assert!(snippet.contains("unset PHPVM_VERSION COMPOSER_HOME PHPRC PHP_INI_SCAN_DIR"));
        assert!(snippet.contains("hash -r"));
    }
}
