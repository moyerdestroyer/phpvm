<p align="center">
  <img src="assets/phpvm-header.jpg" alt="phpvm - PHP Compatibility Manager" width="100%">
</p>

# phpvm

**PHP Compatibility Manager** — run and test PHP apps across real versions without host PHP or Composer.

## Install & update

Install and update are the same command. Re-running replaces the CLI binary; runtimes in `~/.phpvm/` stay put.

```bash
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
```

Installs to `~/.local/bin/phpvm` (override with `PHPVM_INSTALL_DIR`). Supports Linux x86_64 and macOS (Intel + Apple Silicon). Needs only `curl`, `tar`, and `sha256sum`/`shasum`. Always fetches the latest published release.

```bash
# Build from source (Rust)
cargo install --git https://github.com/moyerdestroyer/phpvm
```

Manual downloads: [GitHub Releases](https://github.com/moyerdestroyer/phpvm/releases)

## Quickstart

One full PHP binary per version. **Profiles** are named `.ini` presets (extensions, memory, opcache, etc.) — switch instantly, no reinstall. 

```bash
phpvm install 8.3
phpvm profile use laravel          # use the "laravel" preset php.ini
phpvm profile edit wordpress       # open preset in $EDITOR
phpvm profile new debug --from minimal
phpvm profiles                     # list presets + paths

phpvm run 8.3 php -v
phpvm run 8.3 composer install
phpvm matrix composer test
phpvm doctor
phpvm release-check
```

Set the default profile in `.phpvm.toml`:

```toml
profile = "wordpress"
```

Preset lookup: `.phpvm/profiles/<name>.ini` → `~/.phpvm/profiles/` → bundled starters (`wordpress`, `laravel`, `minimal`). Commit project presets; phpvm never overwrites existing files.

For daily dev, enable the shell integration when the installer asks. It adds one shell rc block that points at the installed binary and runs `phpvm env`, which makes `phpvm` available as a shell function and makes `phpvm use 8.3 --profile=laravel` activate bare `php`/`composer`. Prefer `run` and `matrix` for reproducible checks.

## Uninstall

```bash
rm ~/.local/bin/phpvm
# full cleanup: also rm -rf ~/.phpvm
```

## Learn more

- [AGENTS.md](AGENTS.md) — architecture and contribution guide
- [Project.md](Project.md) — full vision and design
