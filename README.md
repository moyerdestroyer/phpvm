<p align="center">
  <img src="assets/phpvm-header.jpg" alt="phpvm - PHP Compatibility Manager" width="100%">
</p>

# phpvm

phpvm installs prebuilt PHP + Composer runtimes and lets you run a project against a
specific PHP version without relying on host PHP.

It is meant for compatibility work: install a runtime, run Composer or tests with it,
check a project, and test across the versions you support.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
```

The installer downloads the latest phpvm release, verifies it, installs the CLI to
`~/.local/bin/phpvm` by default, and can add shell integration when prompted. It does not
require root, PHP, Composer, or Rust.

## Use

```bash
phpvm ls-remote
phpvm install 8.4
phpvm run 8.4 php -v
phpvm run 8.4 composer install
```

For an interactive shell:

```bash
phpvm use 8.4
php -v
composer -V
```

For project checks:

```bash
phpvm doctor
phpvm matrix composer test
phpvm release-check
```

Project defaults can live in `.phpvm.toml`:

```toml
version = "8.4"
profile = "laravel"
matrix = ["8.2", "8.3", "8.4", "8.5"]
```

## Profiles

Profiles are plain `php.ini` presets for settings such as memory limits, upload limits,
opcache, and error reporting.

```bash
phpvm install 8.4 --profile=laravel
phpvm profile list
phpvm profile use minimal
```

Profiles do not install extensions for static runtimes. Extensions are compiled into the
downloaded runtime and can be inspected with:

```bash
phpvm info 8.4
```

## Scope

phpvm is for running PHP and Composer under known runtimes, testing version compatibility,
and checking Composer PHP / `ext-*` requirements.

phpvm is not a production service manager, web server manager, Docker replacement, or
local PHP compiler. It does not try to make dependency resolution identical across PHP
versions.

## Uninstall

```bash
rm ~/.local/bin/phpvm
rm -rf ~/.phpvm
```

Remove the phpvm shell integration block from your shell config if the installer added
one.

## Feedback

phpvm is still being shaped around real project workflows. If you try it, open an issue
with the project type, PHP versions, command you ran, and what felt confusing or missing:

https://github.com/moyerdestroyer/phpvm/issues
