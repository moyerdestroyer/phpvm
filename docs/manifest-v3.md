# Manifest v3 — Dynamic Runtime Bundles

Manifest v3 describes PHP runtime bundles that work like the official Windows
ZIP layout, but with only the files phpvm needs for CLI workflows.

Each runtime is one archive per PHP version and target. The archive contains one
PHP CLI binary, Composer, a loadable `ext/` directory, a stable `etc/php.ini`,
and an `etc/conf.d/` scan directory managed by phpvm.

## Runtime Layout

```text
bin/
  php
  composer
  composer.phar
ext/
  curl.so
  opcache.so
  xdebug.so
  ...
etc/
  php.ini
  conf.d/
  profiles/
lib/
include/php/        # optional; include only when custom source builds are supported
metadata/runtime.json
```

The base `etc/php.ini` should be stable and profile-neutral. phpvm sets `PHPRC`
to `etc/` and `PHP_INI_SCAN_DIR` to `etc/conf.d` when running or activating the
runtime. Release archives should not contain build-machine absolute paths; phpvm
refreshes `extension_dir` and required default extension snippets after
extraction.

## Schema

```json
{
  "schema": "3.0",
  "profiles": [
    {
      "name": "laravel",
      "extensions": ["curl", "intl", "mbstring", "pdo_mysql", "session", "simplexml", "tokenizer", "xml", "zip"]
    }
  ],
  "runtimes": [
    {
      "php": "8.4.22",
      "composer": "2.9.2",
      "runtime_type": "dynamic",
      "abi": "20240924",
      "thread_safety": "nts",
      "extension_api": "20240924",
      "extensions": [
        {
          "name": "curl",
          "type": "extension",
          "bundled": true,
          "default": false,
          "file": "ext/curl.so"
        },
        {
          "name": "opcache",
          "type": "zend_extension",
          "bundled": true,
          "default": false,
          "file": "ext/opcache.so"
        }
      ],
      "artifacts": {
        "x86_64-unknown-linux-gnu": {
          "url": "https://example.com/php-8.4.22-x86_64-unknown-linux-gnu.tar.gz",
          "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        }
      }
    }
  ]
}
```

## Rules

- `runtime_type = "dynamic"` means profiles and `phpvm ext` manage loadable
  extension snippets under `etc/conf.d/`.
- `extensions[].name` is the user-facing extension name.
- `extensions[].type` is either `extension` or `zend_extension`.
- `extensions[].file` is relative to the runtime root. phpvm writes absolute
  paths when generating INI snippets.
- Profiles are extension enablement presets plus ordinary INI tuning. Activating
  a profile writes `etc/conf.d/20-profile.ini`; it does not replace the base
  `etc/php.ini`.
- Custom extension installs are stored under `ext/custom/` and enabled through
  `etc/conf.d/30-extension-<name>.ini`.

## Compatibility

phpvm still accepts v1/v2/v2.1 manifests. Those runtimes default to
`runtime_type = "static"` and continue through the legacy profile path. Dynamic
extension commands reject static runtimes with an explicit error.
