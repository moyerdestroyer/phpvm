# PHPVM

**PHP Compatibility Manager**

Version: 0.1.0
Status: Planning

---

# Vision

PHPVM is a compatibility testing and execution environment manager for PHP.

Most PHP tooling focuses on answering:

> Which PHP version am I currently using?

PHPVM focuses on answering:

> Does my application actually work on the PHP environments I claim to support?

PHPVM enables developers to test applications, libraries, plugins, and frameworks against specific PHP runtimes without modifying their local environment.

A PHPVM Runtime consists of:

* PHP
* Composer
* Extensions
* php.ini configuration
* Runtime metadata

All runtime components are versioned, isolated, and reproducible.

---

# Problem Statement

PHP developers regularly support multiple PHP versions but typically develop against only one.

This creates several problems:

* Compatibility claims are often unverified
* Different PHP versions resolve different dependency graphs
* Extensions vary between environments
* Customer environments are difficult to reproduce
* CI often catches issues that local development misses

Existing tools primarily focus on version switching:

* Herd
* phpenv
* phpbrew
* asdf
* mise

These tools are valuable, but they solve a different problem.

PHPVM focuses on compatibility verification.

---

# Design Principles

## Host Independence

If a PHPVM command behaves differently because of software installed on the host machine, PHPVM should consider that a bug.

PHPVM should never require:

* Host PHP
* Host Composer
* Host extensions

---

## Reproducibility

The same Runtime should behave identically on every machine.

---

## Compatibility First

PHPVM optimizes for compatibility testing, not version switching.

Version switching is a capability.

Compatibility verification is the product.

---

## Explicit Over Magic

The selected Runtime should always be visible and predictable.

Users should always know:

* PHP version
* Composer version
* Extension profile
* Runtime source

---

# Primary Use Cases

## Verify Compatibility Claims

A developer claims support for:

```text
PHP 8.1+
```

PHPVM verifies that claim.

```bash
phpvm release-check
```

---

## Test Against Multiple PHP Versions

```bash
phpvm matrix composer test
```

Output:

```text
PHP Compatibility Matrix

8.1.latest PASS
8.2.latest PASS
8.3.latest PASS
8.4.latest FAIL
```

---

## Reproduce Customer Environments

```bash
phpvm run 8.2.14 php artisan test
```

---

## Execute Commands Against Specific PHP Versions

```bash
phpvm run 8.3 composer install
```

---

# User Personas

## WordPress Plugin Developer

Needs confidence that a plugin works across supported PHP versions.

Example:

```bash
phpvm release-check
```

---

## Laravel Developer

Needs confidence before deployment.

Example:

```bash
phpvm matrix php artisan test
```

---

## Package Maintainer

Needs confidence that Composer packages support advertised PHP versions.

Example:

```bash
phpvm matrix phpunit
```

---

# Core Concepts

## Runtime

A Runtime is the smallest unit managed by PHPVM.

A Runtime contains:

* PHP Version
* Composer Version
* Extension Profile
* php.ini Configuration
* Platform Metadata

Example:

```text
PHP 8.3.23
Composer 2.9.x
Profile: wordpress
```

---

## Profile

Profiles define bundled extensions and runtime defaults.

Examples:

* wordpress
* laravel
* minimal

---

## Matrix

A Matrix is a collection of PHP runtimes tested against a command.

Example:

```text
8.1.latest
8.2.latest
8.3.latest
8.4.latest
```

---

# Version Resolution

Supported version formats:

```text
8.3
8.3.latest
8.3.min
8.3.12
```

Examples:

```bash
phpvm run 8.3 php -v
```

Resolves to:

```text
latest available 8.3.x
```

---

# Runtime Model

PHPVM manages complete execution environments.

A Runtime contains:

```text
~/.phpvm/
├── runtimes/                 # one directory per *exact* installed patch
│   ├── 8.3.23/
│   │   ├── bin/
│   │   │   ├── php
│   │   │   └── composer
│   │   ├── extensions/
│   │   └── ...
│   └── 8.4.11/
│       └── ...
│
└── composer-homes/           # user global packages, one bucket per minor series
    ├── 8.3/
    │   └── (this becomes COMPOSER_HOME — contains vendor/, config.json, cache/...)
    │       └── vendor/bin/   # tools from `composer global require`
    └── 8.4/
        └── ...
```

Composer is a required Runtime component.

`phpvm use 8.3` (evaluated in your shell) makes the runtime's `bin/` and its global tools available as bare `php` / `composer` / global binaries for the current session. Global Composer packages are isolated per **minor series** (all 8.3.x patches share the same `composer-homes/8.3` bucket), while the PHP runtime itself remains per exact patch. Explicit `phpvm run` commands honor the same isolation.

PHPVM must never rely on a host Composer installation.

Example:

```bash
phpvm run 8.3 composer install
```

must use Runtime Composer, not system Composer.

---

# Composer Requirements

Dependency resolution occurs inside the selected Runtime.

Example:

```bash
phpvm run 8.1 composer install
phpvm run 8.4 composer install
```

These commands may legitimately produce different dependency graphs.

This behavior is expected and desirable.

PHPVM should never attempt to normalize dependency resolution across PHP versions.

---

# Primary Commands

## Matrix

The primary PHPVM command.

```bash
phpvm matrix composer test
```

Runs a command across multiple runtimes.

Example:

```text
8.1.latest PASS
8.2.latest PASS
8.3.latest PASS
8.4.latest FAIL
```

---

## Release Check

High-level compatibility verification.

```bash
phpvm release-check
```

Example:

```text
Detected:
WordPress Plugin

PHP Constraint:
>=8.1

Testing:

8.1.min      PASS
8.1.latest   PASS
8.2.latest   PASS
8.3.latest   PASS
8.4.latest   FAIL

Result:
RELEASE BLOCKED
```

---

## Doctor

Project inspection and recommendations.

```bash
phpvm doctor
```

Outputs:

* Project type
* PHP constraints
* Extension requirements
* Composer requirements
* Recommended Matrix

Example:

```text
Project Type:
WordPress Plugin

PHP Constraint:
>=8.1

Recommended Matrix:

8.1.min
8.1.latest
8.2.latest
8.3.latest
8.4.latest
```

---

## Run

Execute a command against a specific Runtime.

```bash
phpvm run 8.3 composer install
```

---

## Install

Install a Runtime.

```bash
phpvm install 8.3.23
```

---

## Versions

```bash
phpvm versions
```

Lists installed runtimes.

---

# WordPress Support

WordPress is a first-class use case.

Future commands:

```bash
phpvm wp test
phpvm wp doctor
phpvm wp release-check
```

These commands may wrap existing functionality but should provide WordPress-specific defaults and reporting.

---

# Profiles

## wordpress

Extensions:

```text
curl
dom
gd
intl
mbstring
mysqli
openssl
pdo_mysql
xml
zip
```

---

## laravel

Extensions:

```text
curl
intl
mbstring
openssl
pdo_mysql
tokenizer
xml
zip
```

---

## minimal

Minimal extension set.

---

# Project Detection

PHPVM should automatically detect:

## Composer Library

Indicators:

```text
composer.json
```

---

## WordPress Plugin

Indicators:

```text
Plugin Name:
```

or standard WordPress plugin structure.

---

## Laravel Application

Indicators:

```text
artisan
bootstrap/app.php
```

---

# Manifest System

PHPVM downloads Runtime metadata from a Manifest.

The Manifest is the source of truth.

Example:

```json
{
  "php": "8.3.23",
  "composer": "2.9.2",
  "profile": "wordpress",
  "url": "https://...",
  "sha256": "..."
}
```

The CLI must never hardcode Runtime download URLs.

---

# Runtime Installation

Version 1 must use prebuilt binaries.

Version 1 must NOT compile PHP locally.

Installation flow:

```text
Resolve Version
↓
Download Manifest
↓
Verify Checksum
↓
Download Runtime
↓
Extract
↓
Cache
↓
Execute
```

---

# Reporting

Future output formats:

```bash
phpvm matrix --report=json
phpvm matrix --report=markdown
phpvm matrix --report=html
```

---

# Rust Requirements

Stable Rust only.

Required tooling:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Future:

```bash
cargo nextest
```

---

# Repository Structure

```text
phpvm/
├── src/
│   ├── cli.rs
│   ├── config.rs
│   ├── doctor.rs
│   ├── install.rs
│   ├── manifest.rs
│   ├── matrix.rs
│   ├── output.rs
│   ├── runner.rs
│   ├── version.rs
│   └── providers/
│       ├── static_php.rs
│       ├── docker.rs
│       └── local.rs
│
├── fixtures/
│   ├── wordpress-plugin/
│   ├── laravel-app/
│   └── composer-library/
│
├── tests/
│
├── Cargo.toml
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml
└── README.md
```

---

# Success Criteria

Version 1 is successful if a developer can:

```bash
phpvm install 8.3

phpvm run 8.3 composer install

phpvm matrix composer test

phpvm doctor

phpvm release-check
```

without requiring:

* Host PHP
* Host Composer
* Multiple PHP installations
* Extension compilation
* Local PHP source builds

If PHPVM makes compatibility testing boring, it has succeeded.

---

# Planned / Future Considerations

## Per-project `use` (lightweight project pinning)

Users have requested that `phpvm use` (especially with no argument) or shell integration can automatically select the appropriate runtime when inside a project directory.

This should be driven by a project-local file. Candidates include:
- A simple `.phpvm-version` file (just a version specifier such as `8.3`, `latest`, or `8.4.11`), similar to `.nvmrc` / `.node-version`.
- Or (preferred for power users) the existing `.phpvm.toml` (which already supports `php_constraint`, `profile` (built-in or custom), matrix, etc.).

Key requirement (from user feedback): the mechanism must be able to specify the *extension profile*, not just the PHP version.

See the detailed TODO comment in `src/version.rs` near the `activate` and `print_env` functions.

This feature is intentionally deferred so we can get the global `use` + persistent behavior solid first.

