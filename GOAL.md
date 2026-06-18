# Tusk — a fast PHP toolchain in Rust

> **Codename:** `tusk` (placeholder — rename freely). Pronounced like the elephant tusk. Short, native-feeling on the CLI.
>
> **This document is the single source of truth for coding agents.** Read it fully before writing any code. When in doubt, prefer the rule written here over your own instinct. If a decision is missing, add it to this file (section _Open Decisions_) rather than guessing silently in code.

---

## 1. Mission & North Star

Be the **Bun of PHP**: one cohesive, blazing-fast CLI that replaces the slow, fragmented PHP tooling experience — **without** reimplementing PHP itself.

The north-star demo, the thing we benchmark and brag about:

```
$ tusk install      # a real Laravel/Symfony project, cold cache
```

…finishes in a fraction of `composer install` time, with a nicer progress UI and better error messages. If `tusk install` is not dramatically faster than Composer on a real project, the project has failed its first milestone. Everything else is secondary.

### The one-sentence strategy

> Use the real, battle-tested **Zend engine for execution** (embedded later, via `ext-php-rs`); win on **tooling speed, startup, and developer experience** written in Rust. Never try to out-correctness Zend.

---

## 2. The Core Architectural Decision (read this twice)

**We do NOT reimplement the PHP language or the Zend engine.** Projects that try (e.g. `phprs`) drown in extension/semantics compatibility and never ship something production-usable. That road is explicitly **out of scope** and any agent proposing it should stop and re-read this section.

**We DO** build a single Rust binary that is an all-in-one toolchain:

| Phase | Component | Needs PHP/Zend? | Why this order |
|-------|-----------|------------------|----------------|
| **1 (MVP)** | **Package manager** (Composer-compatible install/update/require) | ❌ No — pure Rust | Highest leverage, most winnable, fully testable in isolation, the `bun install` moment |
| **2** | **Runtime / app server** (embed Zend via `ext-php-rs`, worker mode, FPM-style SAPI) | ✅ Yes | The execution story; learn from `pasir` / `FerrumPHP` / FrankenPHP |
| **3** | **Script & task runner** (`tusk run`, `tusk test`) | ✅ Yes | Cohesive DX layer on top of the runtime |
| **4 (optional)** | **AOT for hot static modules** (compile a static PHP subset to native, à la `elephc`) | partial | Only if profiling proves a real CPU bottleneck worth it |

**Phase 1 is the entire current scope of this spec.** Phases 2–4 are sketched at the end so the architecture doesn't paint us into a corner, but **agents must not start Phase 2 until Phase 1 meets its Definition of Done.**

### Why package-manager-first

- It's the slowest, most painful part of real PHP DX → biggest visible win.
- It's **pure Rust**: dependency resolution, parallel HTTP, archive extraction, autoloader generation. No FFI, no PHP semantics, no `unsafe`.
- It's independently useful even if nothing else ever ships — a drop-in faster `composer install`.
- It is *extremely* amenable to TDD: every component has clean, deterministic inputs and outputs.

---

## 3. Scope of Phase 1 (the package manager)

### In scope
- Parse `composer.json` (require, require-dev, autoload/autoload-dev, repositories, config, minimum-stability, prefer-stable).
- Read and write `composer.lock` **in a format Composer itself accepts** (interop is a hard requirement — a user must be able to use `tusk` and `composer` on the same repo).
- Resolve dependency graphs from **Packagist** metadata API (`https://repo.packagist.org/p2/{vendor}/{package}.json`), honoring Composer version constraints and stability flags.
- Download `dist` archives, verify their `shasum`, extract into `vendor/{vendor}/{package}/`.
- Generate a **Composer-compatible autoloader** (`vendor/autoload.php` + `vendor/composer/autoload_{psr4,psr0,classmap,files,namespaces,static}.php`) that real frameworks load without modification.
- Platform requirement checks (`php` version, `ext-*`) — read from a config-provided platform map (do **not** require a PHP binary in Phase 1; allow `--platform` override and a config file).
- Commands: `tusk install`, `tusk update`, `tusk require <pkg>`, `tusk remove <pkg>`.
- Global content-addressed cache (`~/.cache/tusk/`) so repeat installs are near-instant.

### Out of scope for Phase 1 (do not build yet)
- Running PHP, embedding Zend, any `ext-php-rs` work.
- VCS/`source` installs (git clones). **Dist-only** for the MVP; error clearly on source-only packages.
- Private repositories / auth, Composer plugins, scripts/hooks execution.
- `composer.json` schema validation beyond what install needs.

### Non-goals (ever)
- Reimplementing PHP semantics or the Zend VM.
- Bit-for-bit reproduction of Composer's solver *internals* — we match its **outputs** (resolved versions, lock format, autoloader), not its algorithm.

---

## 4. Tech Stack (pin these; justify any deviation in a PR)

- **Language:** Rust (stable, edition 2021+). `#![forbid(unsafe_code)]` for the whole Phase-1 workspace.
- **Async runtime:** `tokio`.
- **HTTP:** `reqwest` (rustls, HTTP/2, connection pooling) for parallel package downloads.
- **Dependency resolution:** the **`pubgrub`** crate (PubGrub algorithm — fast, and produces *human-readable* "because X requires Y…" conflict explanations, which is a headline DX feature). We wrap it with a Composer-flavored version/constraint adapter.
- **Semver:** **do NOT use the `semver` crate as-is.** Composer constraints differ (`^1.2`, `~1.2`, `1.2.*`, `>=1.2 <2.0 || >=3.0`, `dev-main`, stability flags `@dev`/`@stable`). Build a `composer_constraint` module that parses and evaluates Composer's grammar. Reference the official spec: <https://getcomposer.org/doc/articles/versions.md>.
- **Serialization:** `serde` + `serde_json`. Preserve key order where the lock format requires it.
- **Archives:** `zip` + `flate2` for dist extraction.
- **Hashing:** `sha1`/`sha256` for shasum verification and the content-addressed cache.
- **CLI:** `clap` (derive).
- **Progress UI:** `indicatif`.
- **Errors:** `thiserror` for library crates, `anyhow` at the binary boundary.
- **Testing:** built-in `#[test]`, `assert_cmd` + `predicates` for CLI integration, `wiremock` for mocking Packagist, `insta` for snapshot tests of generated autoloader files and lock files.

---

## 5. Repository Structure

A Cargo workspace, split so the solver and parsers are testable without touching the network or filesystem:

```
tusk/
├── Cargo.toml                 # workspace
├── crates/
│   ├── tusk-cli/              # binary: clap commands, wiring, progress UI
│   ├── tusk-manifest/         # composer.json + composer.lock parse/serialize
│   ├── tusk-semver/           # composer_constraint: version + constraint grammar
│   ├── tusk-resolver/         # pubgrub adapter -> resolved dependency set
│   ├── tusk-registry/         # Packagist client (trait-based, mockable)
│   ├── tusk-installer/        # download, verify, extract, cache
│   └── tusk-autoload/         # generate Composer-compatible autoloader files
├── fixtures/                  # real composer.json/lock samples, golden outputs
└── tests/                     # cross-crate integration tests
```

**Key design rule:** `tusk-registry` is defined behind a trait (e.g. `Registry`) so resolver/installer tests run fully offline against fixtures and `wiremock`. No test may hit the real network.

---

## 6. TDD Workflow — rules for agents (non-negotiable)

This project is built **test-first**. For every unit of behavior:

1. **RED** — write a failing test that pins the desired behavior. Run it; confirm it fails for the *right* reason.
2. **GREEN** — write the minimum code to pass. No extra features.
3. **REFACTOR** — clean up with tests green.
4. Commit at each green point. One behavior per commit where practical.

Hard rules:
- **No production code is written without a failing test that demands it.** If you find yourself adding a function "because we'll need it," stop — add the test first or don't add the function.
- **Never weaken or delete a test to make the suite pass.** If a test is wrong, fix the test deliberately and say so in the commit message.
- Every public function in a `tusk-*` library crate has at least one unit test. Every CLI command has at least one `assert_cmd` integration test.
- Bugs get a **regression test first** that reproduces the bug, then the fix.
- `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check` must all pass before any task is considered done. Treat clippy warnings as errors.
- Determinism: resolution and autoloader generation must be **deterministic** given the same inputs. Add tests that run the operation twice and assert identical output.

### Test taxonomy
- **Unit** (in each crate): parsers, constraint matching, the solver adapter, autoloader rendering. Pure, fast, offline.
- **Integration** (`/tests`): wire crates together against fixtures + `wiremock` Packagist. e.g. "given this `composer.json` and these mocked package metadata responses, `install` produces this `composer.lock` and this `vendor/` tree."
- **Golden/snapshot** (`insta`): generated `composer.lock` and `autoload_*.php` files are byte-compared against committed golden files. These are the strongest interop guarantee.
- **Compatibility** (CI, gated): take a handful of real small libraries, run `tusk install`, then assert the generated autoloader can `require` and resolve their classes. (This crosses into needing PHP — keep it in a separate, optional CI job until Phase 2.)

---

## 7. Phase 1 — Detailed Component Specs (each is a TDD work package)

Agents: tackle these roughly in order. Each lists **behavioral specs to encode as tests first**, then the implementation surface.

### 7.1 `tusk-semver` (the algorithmic heart, build first)
**Test-first behaviors:**
- Parse a version string into comparable parts (major.minor.patch.tweak, plus stability suffixes `-alpha`/`-beta`/`-RC`/`-dev`).
- Parse constraint grammar: exact, `^`, `~`, `*` wildcards, ranges with `>=`,`>`,`<`,`<=`,`!=`, hyphen ranges, `||`/`,` combinations, and `dev-<branch>`.
- `constraint.matches(version) -> bool` for a table of known Composer cases (build a fixture table straight from Composer's own docs/examples — these become your spec).
- Stability ordering and `minimum-stability` / `@`-flag handling.
- Property test: any version parsed and re-displayed round-trips.

### 7.2 `tusk-manifest`
**Test-first behaviors:**
- Deserialize representative real `composer.json` files (Laravel, Symfony, a tiny lib) — committed in `fixtures/`.
- Serialize a `composer.lock` and assert it round-trips through Composer's expected shape (golden file).
- Edge cases: missing sections, `require-dev`, PSR-4 with multiple paths, `files` autoload, `classmap`.

### 7.3 `tusk-registry`
**Test-first behaviors (all against `wiremock`, never the live API):**
- Fetch and parse Packagist p2 metadata for a package into an internal model (all versions + their dist urls + shasums + their own requires).
- Handle the `p2` "dev" vs stable file split.
- Cache metadata responses; a second fetch within a run does no second HTTP call (assert via mock call count).
- Network/HTTP errors surface as typed errors, not panics.

### 7.4 `tusk-resolver` (PubGrub adapter)
**Test-first behaviors:**
- Given a set of mocked packages with known requires, produce the expected resolved version set.
- Reproduce a known **conflict** and assert the error message names the conflicting packages and constraints (this is a feature — snapshot the message).
- `require-dev` included in dev installs, excluded with `--no-dev`.
- Respect `minimum-stability` / `prefer-stable`.
- Determinism test: identical inputs → identical resolution, twice.

### 7.5 `tusk-installer`
**Test-first behaviors:**
- Given a resolved set + mocked dist archives, download in parallel, verify each `shasum` (assert a tampered archive is **rejected**), extract to `vendor/{vendor}/{pkg}/`.
- Content-addressed cache: second install of the same package version skips download (assert mock call count = 0).
- Partial-failure safety: a failed download leaves no half-written package in `vendor/` (write to temp, atomic rename).

### 7.6 `tusk-autoload`
**Test-first behaviors (golden files are king here):**
- Generate `vendor/autoload.php` + `autoload_psr4.php`, `autoload_classmap.php`, `autoload_files.php`, `autoload_namespaces.php`, `autoload_static.php`, `autoload_real.php` matching Composer's structure closely enough that frameworks boot.
- PSR-4, PSR-0, classmap, and `files` (eager-required) all represented correctly.
- Deterministic ordering of map entries (sorted) so golden files are stable.

### 7.7 `tusk-cli`
**Test-first behaviors (`assert_cmd`):**
- `tusk install` in a fixture project produces lock + vendor + autoloader; exit code 0; sensible stdout.
- `tusk require vendor/pkg` mutates `composer.json`, re-resolves, updates lock.
- `tusk remove`, `tusk update` behaviors.
- Error UX: a resolution conflict exits non-zero with the readable PubGrub explanation. A source-only package exits with a clear "dist-only in MVP" message (not a panic).
- `--no-dev`, `--platform`, `--quiet` flags.

---

## 8. Definition of Done — Phase 1

Phase 1 is done when **all** hold:
1. `tusk install` works end-to-end on at least **three real projects** (e.g. a small lib, a Symfony skeleton, a Laravel skeleton) and the apps boot using the generated autoloader.
2. The generated `composer.lock` is accepted by real Composer (`composer install` against tusk's lock succeeds) — proven in CI.
3. A committed benchmark shows `tusk install` is **meaningfully faster** than `composer install` on a real project, cold and warm cache (record both). This is the headline number.
4. `cargo test` (unit + integration + golden), `clippy -D warnings`, `fmt --check` all green in CI.
5. `#![forbid(unsafe_code)]` holds across the workspace.
6. README with install + the benchmark table.

---

## 9. Phase 2+ (sketch only — do not build until Phase 1 is Done)

- **Runtime (`tusk serve`/`tusk run`):** embed Zend via **`ext-php-rs`**, implement a SAPI, run a **worker mode** (persistent workers, like FrankenPHP/RoadRunner/pasir) to kill per-request bootstrap cost. Study `pasir` and `FerrumPHP` first; consider whether wrapping/learning-from FrankenPHP is faster than greenfield. Worker mode **exposes state-leak bugs** (globals, `$_SESSION`) that the shared-nothing FPM model hid — budget for that.
- **Task/test runner:** `tusk test` wrapping PHPUnit/Pest; `tusk run <script>`.
- **AOT (optional, profile-gated):** compile a *static subset* of hot PHP to native (the `elephc` model: keep the framework on Zend, peel off CPU-bound static modules). Only if profiling proves a real CPU bottleneck — most PHP apps are I/O-bound and this won't help them.

---

## 10. Prior art to study (don't reinvent; borrow patterns)
- **`ext-php-rs`** — the crate for embedding/extending Zend from Rust. The Phase-2 foundation.
- **`pasir`**, **`FerrumPHP`** — PHP application servers in Rust embedding Zend. Reference SAPI/worker design.
- **FrankenPHP** — Go-based modern runtime; the UX bar and worker-mode reference.
- **Mago** — Rust PHP toolchain (lint/format/analyze); proof the "fast Rust tooling for PHP" thesis works and is welcomed.
- **`elephc`** — Rust AOT compiler for a static PHP subset; the Phase-4 model.
- **PubGrub** (Dart pub, uv) — the resolution algorithm and the standard for good conflict messages.
- **Composer docs on versions** — the spec your `tusk-semver` must satisfy: <https://getcomposer.org/doc/articles/versions.md>

---

## 11. Open Decisions (append here instead of guessing in code)
- Exact `composer.lock` fields to emit for full Composer interop (enumerate against a real lock file; pin in a golden test).
- Whether Phase 2 wraps FrankenPHP or goes greenfield on `ext-php-rs`.
- Final project name (replace `tusk`).
