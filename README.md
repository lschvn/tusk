# Tusk — A fast PHP toolchain in Rust

> A Composer-compatible package manager, written in Rust, designed to be the **Bun of PHP** — a single blazing-fast CLI that replaces the slow, fragmented PHP tooling experience **without reimplementing PHP itself**.

## What is it?

`Tusk` (working name) is a pure-Rust package manager that aims to drop into any PHP project and make `tusk install` dramatically faster than `composer install`, with a nicer progress UI and human-readable resolution error messages.

It does **not** run PHP. It does **not** reimplement the Zend engine. It generates a `composer.lock` and `vendor/autoload.php` that real Composer and real PHP frameworks consume without modification.

For the full mission, architecture, and Phase-1 scope, see **[GOAL.md](./GOAL.md)**.

## Status

**Phase 1 — the package manager.** See [ROADMAP.md](./ROADMAP.md) for the work breakdown.

## Install

Install the latest release with a single command — works on Linux and macOS,
x86_64 and arm64:

```bash
curl -fsSL https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash
```

Prefer `wget`?

```bash
wget -qO- https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash
```

The installer downloads the prebuilt binary for your platform into `~/.tusk/bin`
and adds it to your `PATH`. Restart your shell (or `source` your shell rc) and:

```bash
tusk --help
```

<details>
<summary>Advanced options</summary>

```bash
# Install a specific version
curl -fsSL https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash -s v0.1.0

# Install to a custom directory (default: ~/.tusk)
TUSK_INSTALL=/opt/tusk curl -fsSL https://raw.githubusercontent.com/lschvn/tusk/main/install.sh | bash
```

If no prebuilt binary exists for your platform, the installer falls back to
building from source with your local Rust toolchain (install Rust from
<https://rustup.rs>). To uninstall, just remove the install directory
(`rm -rf ~/.tusk`) and the `# tusk` block from your shell rc file.

</details>

## Quick start (once built)

```bash
# In any PHP project that has a composer.json:
$ tusk install              # produces composer.lock + vendor/

# Add a dependency:
$ tusk require vendor/pkg

# Drop-in compatible with Composer:
$ composer install          # works against the lock file tusk produced
```

## Development

```bash
# Run the full test suite
$ cargo test --all

# Lint (warnings are errors — see GOAL.md §4, §6)
$ cargo clippy --all-targets -- -D warnings

# Format check
$ cargo fmt --all -- --check
```

## Architecture

A Cargo workspace with one crate per concern:

| Crate              | Role                                                         |
|--------------------|--------------------------------------------------------------|
| `tusk-semver`      | Composer constraint + version grammar (the algorithmic heart)|
| `tusk-manifest`    | `composer.json` / `composer.lock` parse + serialize          |
| `tusk-registry`    | Packagist client behind a `Registry` trait (mockable)        |
| `tusk-resolver`    | PubGrub adapter -> resolved dependency set                   |
| `tusk-installer`   | Parallel download, shasum verify, atomic extract, content cache |
| `tusk-autoload`    | Generate Composer-compatible autoloader files                |
| `tusk-cli`         | `clap` binary wiring all of the above                        |

See [GOAL.md §5](./GOAL.md) for the full design rationale.

## License

MIT OR Apache-2.0
