<p align="center">
  <img src="assets/phpvm-header.jpg" alt="phpvm - PHP Compatibility Manager" width="100%">
</p>

# phpvm

**PHP Compatibility Manager**

The simple way to run and test your PHP applications against multiple real versions — without touching (or needing) your host PHP or Composer.

## Install & update

phpvm is a single standalone binary. **Install and update use the same command** — re-running it downloads the latest release and replaces the existing binary. Your PHP runtimes in `~/.phpvm/` are left alone.

```bash
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
```

The script fetches a prebuilt binary from [GitHub Releases](https://github.com/moyerdestroyer/phpvm/releases), verifies its checksum, and installs to `~/.local/bin/phpvm`.

**Platforms:** Linux x86_64, macOS Intel, macOS Apple Silicon.

**Host requirements:** `curl`, `tar`, and `sha256sum` or `shasum` (standard on modern systems). No PHP, Composer, or Rust required.

**PATH:** If `phpvm` is not found after install, add `~/.local/bin` to your shell profile — the installer prints the exact line if needed.

**Pin a version** (install a specific release or downgrade):

```bash
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | PHPVM_VERSION=0.1.0 bash
```

**Custom install directory** (use the same value when updating):

```bash
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | PHPVM_INSTALL_DIR=$HOME/bin bash
```

**Build from source** (developers with Rust):

```bash
cargo install --git https://github.com/moyerdestroyer/phpvm
```

## Background

PHP developers routinely claim support for ranges like "PHP 8.1+". In practice they develop and test against a single version.

phpvm solves the compatibility problem:

- **Isolated runtimes**: each includes exact PHP + Composer + chosen extensions + php.ini
- **Host independence**: zero reliance on system PHP/Composer/extensions (a core design principle)
- **Reproducible**: manifest-driven downloads with checksums. No local compilation for V1.
- Focused on verification: matrix testing, pre-release checks, project inspection.

## Quickstart

Profiles are **named `.ini` preset files** — not extension lists in TOML. Author full php.ini configs in `.phpvm/profiles/` (project) or `~/.phpvm/profiles/` (global). Switch with `phpvm profile use <name>`.

```bash
phpvm install 8.3 --profile=wordpress   # downloads full binary once
phpvm profile use laravel               # switch ini preset instantly (no reinstall)
phpvm profile edit wordpress            # tweak memory, opcache, xdebug, etc.
phpvm profile new debug --from minimal  # create a project preset
phpvm profiles                          # list presets + source paths
phpvm run 8.3 php -m
phpvm run 8.3 composer install
phpvm matrix composer test
phpvm doctor
phpvm release-check

# Listing and inspection
phpvm ls
phpvm ls-remote
phpvm info 8.3
phpvm profiles

# Daily development (makes bare `php` and `composer` use a specific runtime)
# One-time setup (add to your shell rc):
#   eval "$(phpvm env)"
phpvm use 8.3 --profile=laravel  # activate version + profile in one step
php -v
composer --version
```

See `phpvm --help` and subcommand help for options (including JSON output).

`phpvm use <version>` sets your active runtime persistently (like fnm). After the one-time `eval "$(phpvm env)"` in your rc file, `phpvm use 8.3` immediately updates the current shell (the shell function wrapper applies the changes) and persists for new terminals. Bare `php`/`composer` + per-minor global packages then work directly. For reproducible verification, still prefer the explicit `run` / `matrix` commands.

### Planned

- Per-project version + profile selection (e.g. a `.phpvm-version` file or using the existing project `.phpvm.toml`). See TODOs in `src/version.rs` (near `activate` / `print_env`). Any such mechanism should carry the full extension profile, not just a PHP version string.

## Uninstall

Delete the CLI binary:

```bash
rm ~/.local/bin/phpvm
```

(Use your `PHPVM_INSTALL_DIR` path if you customized it.)

Or run the installer in uninstall mode:

```bash
export PHPVM_UNINSTALL=1
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
```

Downloaded PHP runtimes, profiles, and config live separately under `~/.phpvm/`. Remove that directory too if you want a full cleanup.

## Learn more

- [AGENTS.md](AGENTS.md) — contribution guide, architecture, and required checks
- [Project.md](Project.md) — full vision and design
- Primary workflows: `install`, `run`, `matrix`, `doctor`, `release-check`
