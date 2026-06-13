# PHPVM Requirements

## Product

PHPVM is a PHP compatibility manager. It enables developers to test applications, libraries, plugins, and frameworks against specific PHP runtimes without modifying their local environment.

### Core Principle

> PHPVM manages complete PHP execution environments, not just PHP binaries.

Each runtime includes:

- PHP
- Composer
- Extensions
- php.ini
- metadata.json

### Host Independence

If a PHPVM command behaves differently because of host-installed software, that is a bug. PHPVM must never require host PHP, host Composer, or host extensions.

### Primary Commands

```bash
phpvm install 8.3.23          # Install a runtime
phpvm run 8.3 composer install # Run a command in a runtime
phpvm matrix composer test      # Run across multiple runtimes
phpvm doctor                    # Inspect project and recommend matrix
phpvm release-check             # Verify compatibility before release
phpvm versions                  # List installed runtimes
```

### Version Resolution

Supported formats:

- `8.3` → latest available 8.3.x
- `8.3.latest` → latest available 8.3.x
- `8.3.min` → minimum available 8.3.x
- `8.3.12` → exact version

### Profiles

Profiles are named `.ini` preset files (full php.ini configs). One full binary is installed per PHP version; `phpvm profile use <name>` copies a preset to the active `etc/php.ini`.

Preset locations (resolution order): `.phpvm/profiles/`, `~/.phpvm/profiles/`, runtime `etc/profiles/`, then bundled starters.

Use `phpvm profile use <name>` to switch without reinstalling. Edit presets with `phpvm profile edit`.

- `wordpress` — curl, dom, gd, intl, mbstring, mysqli, openssl, pdo_mysql, xml, zip
- `laravel` — curl, intl, mbstring, openssl, pdo_mysql, tokenizer, xml, zip
- `minimal` — no extensions (bundled starter)

### Project Detection

PHPVM should auto-detect:

- **Composer Library**: `composer.json`
- **WordPress Plugin**: `Plugin Name:` header or standard WP structure
- **Laravel Application**: `artisan` + `bootstrap/app.php`

---

## V1 Constraints

- Use prebuilt/static PHP runtimes only
- Bundle Composer per runtime
- Support exact PHP patch versions
- Support profiles (wordpress, laravel, minimal)
- Use a remote manifest for runtime URLs and checksums
- Verify checksums before install
- Do NOT compile PHP locally
- Do NOT manage web servers, PHP-FPM, nginx, or Apache

---

## Technical

### Language & Tooling

- Rust stable, edition 2021
- Max line width: 100 characters
- Required checks: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`
- Future: `cargo nextest run`

### Architecture

- Single-crate CLI
- Provider pattern for runtime backends (static_php, docker, local)
- Manifest-driven (no hardcoded URLs)
- `camino::Utf8PathBuf` for file paths

### Testing

- Unit tests for version resolution, config parsing, manifest handling
- CLI integration tests with `assert_cmd`
- Fixtures for WordPress plugin, Laravel app, Composer library
- Snapshot tests for output formatting with `insta`

### Key Workflows to Test

```bash
phpvm run 8.3 php -v
phpvm run 8.3 composer install
phpvm matrix composer test
phpvm doctor
phpvm release-check
```