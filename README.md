<p align="center">
  <img src="assets/phpvm-header.jpg" alt="phpvm - PHP Compatibility Manager" width="100%">
</p>

# phpvm

**PHP, without the drama.**

A lightweight tool for developers who need to hop between real PHP versions, run tests across them, and not think about it until they have to. Prebuilt static binaries, zero host PHP or Composer, and profiles that are just .ini files for the settings you actually care about.

Install in one curl. Switch versions instantly. Test your stuff. Get back to writing code.

## 30 seconds to up and running

```bash
curl -fsSL https://raw.githubusercontent.com/moyerdestroyer/phpvm/master/install.sh | bash
```

Then:

```bash
phpvm install 8.3
phpvm run 8.3 php -v
phpvm run 8.3 composer install
```

That's it. The runtime lives in `~/.phpvm/`, the CLI is in `~/.local/bin/phpvm` (or wherever you pointed the installer).

Want more versions?

```bash
phpvm install 8.1 8.2 8.4
phpvm matrix composer test
```

Checking a release is about to ship?

```bash
phpvm doctor
phpvm release-check
```

## Profiles = your settings, your way

Profiles are plain `.ini` files. They handle memory limits, opcache, error reporting, and whatever else you tweak. They never install, enable, or disable extensions: each runtime ships with its extension catalog compiled in.

```bash
phpvm install 8.3 --profile=laravel
phpvm profile use wordpress
phpvm profile edit my-team-rules
phpvm profile list
```

Drop team presets in `.phpvm/profiles/` in your repo. They get picked up automatically. phpvm never clobbers files you already have.

`extension=` and `zend_extension=` lines are not applied from profiles. Use `phpvm info <version>` to inspect the compiled catalog. `phpvm doctor` checks `composer.json` `ext-*` requirements against the installed runtime's actual `php -m` output.

Set a project default in `.phpvm.toml`:

```toml
profile = "wordpress"
```

## The shell integration (this is the good part)

Run the installer and say yes to shell integration, or add this to your `.zshrc`/`.bashrc`:

```bash
eval "$(phpvm env)"
```

Now you can do:

```bash
phpvm use 8.3 --profile=laravel
# boom — bare `php` and `composer` just work in this shell (and future shells)
```

`phpvm use system` drops you back to whatever your OS ships. `phpvm deactivate` if you want to undo for the current shell only.

Prefer explicit? `phpvm run 8.3 ...` always works and is great for scripts/CI.

## Uninstall

```bash
rm ~/.local/bin/phpvm
# full nuke (optional)
rm -rf ~/.phpvm
```

## We actually want your feedback

This thing exists because developers get tired of "but it works with 8.2 on my machine." We're optimizing for the workflows that matter: quick installs, easy matrix testing, `doctor` and `release-check` when you're about to ship, and not having to care about PHP until you do.

If you're kicking the tires:

- Does it feel fast and obvious?
- Is there a workflow you wish it just handled?
- Got a weird edge case or a feature that would make your life better?

Open an issue. Comment on a release. Tell us what sucks or what rules. User testing and real suggestions are how this gets better — we're not trying to build the perfect compatibility matrix in a vacuum.

Star the repo, try it on a side project, and let us know how it goes.

## Learn more

- [AGENTS.md](AGENTS.md) — how the thing is built and how to contribute
- [GitHub](https://github.com/moyerdestroyer/phpvm) — issues, releases, the usual

Happy hacking.
