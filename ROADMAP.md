# Tusk — Roadmap

This decomposes [GOAL.md](./GOAL.md) into trackable steps. Each step is a TDD work package: **tests first, then minimum code, then refactor, then commit**.

## Phase 1 — Package Manager (current scope)

### Step 0 — Workspace + CI
- [x] Workspace scaffold (all 7 crates as members, pinned deps, `#![forbid(unsafe_code)]`)
- [x] `GOAL.md`, `README.md`, `ROADMAP.md`
- [ ] GitHub Actions: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`

### Step 1 — `tusk-semver` (the algorithmic heart, build first)
Spec: [GOAL.md §7.1](./GOAL.md)
- [ ] `Version` parsing: `1.2.3`, `1.2.3.4`, `1.2.3-alpha`, `1.2.3-beta`, `1.2.3-RC1`, `1.2.3-dev`, `v1.2.3`, `dev-main`
- [ ] `Constraint` parser: exact, `^1.2`, `~1.2`, `1.2.*`, `*`, `>=1.2`, `>1.2`, `<2.0`, `<=2.0`, `!=1.5`, hyphen ranges `1.2 - 3.4`, OR `||`, AND `,`
- [ ] `Constraint::matches(&Version) -> bool` with a fixture table from Composer's docs
- [ ] Stability ordering + `minimum-stability` + `@dev`/`@stable` flags
- [ ] Property test: version parse → display round-trips
- [ ] `pubgrub` integration: build a `Version` adapter that pubgrub can sort + range over

### Step 2 — `tusk-manifest`
Spec: [GOAL.md §7.2](./GOAL.md)
- [ ] `composer.json`: deserialize Laravel, Symfony, tiny lib fixtures
- [ ] `composer.lock` serialize, golden-roundtrip against a real Composer lock
- [ ] Edge cases: `require-dev`, multi-path PSR-4, `files`, `classmap`
- [ ] Preserve key order in `composer.lock` (uses `indexmap`)

### Step 3 — `tusk-registry`
Spec: [GOAL.md §7.3](./GOAL.md)
- [ ] `Registry` trait + `PackagistRegistry` impl + `MockRegistry` for tests
- [ ] Parse p2 metadata (versions, dist url + shasum, requires)
- [ ] Handle dev/stable split (p2 file is metadata-only, versions live in `packages.json` for dev/master)
- [ ] In-process metadata cache (one HTTP call per package per run, asserted via wiremock)
- [ ] Typed error types for network/HTTP failures

### Step 4 — `tusk-resolver`
Spec: [GOAL.md §7.4](./GOAL.md)
- [ ] Wrap `pubgrub` with a Composer version/constraint adapter
- [ ] Resolve a mocked set of packages → assert exact resolved version
- [ ] Force a conflict → snapshot the human-readable error message
- [ ] `require-dev` included in dev installs, excluded with `--no-dev`
- [ ] `minimum-stability` / `prefer-stable` respected
- [ ] Determinism test: identical inputs → identical resolution, twice

### Step 5 — `tusk-installer`
Spec: [GOAL.md §7.5](./GOAL.md)
- [ ] Parallel dist download via `reqwest` + `tokio`
- [ ] SHA1 verification, tampered archive **rejected**
- [ ] Extract zip into `vendor/{vendor}/{pkg}/`
- [ ] Content-addressed cache: second install of same version → 0 HTTP calls
- [ ] Atomic extract: write to `vendor/{vendor}/{pkg}.tmp-{hash}/`, rename on success

### Step 6 — `tusk-autoload`
Spec: [GOAL.md §7.6](./GOAL.md)
- [ ] Generate `vendor/autoload.php` (entry point)
- [ ] `vendor/composer/autoload_psr4.php`
- [ ] `vendor/composer/autoload_psr0.php` (legacy)
- [ ] `vendor/composer/autoload_classmap.php`
- [ ] `vendor/composer/autoload_files.php` (eager-required)
- [ ] `vendor/composer/autoload_namespaces.php` (legacy)
- [ ] `vendor/composer/autoload_static.php` (the static registry used by Composer's `autoload.php`)
- [ ] `vendor/composer/autoload_real.php` (the `ComposerAutoloaderInit*` class)
- [ ] Golden snapshots (committed fixtures) — sorted keys, deterministic

### Step 7 — `tusk-cli`
Spec: [GOAL.md §7.7](./GOAL.md)
- [ ] `tusk install` (end-to-end on a fixture, exit 0, sensible stdout)
- [ ] `tusk update`
- [ ] `tusk require vendor/pkg` (mutates `composer.json`, re-resolves, updates lock)
- [ ] `tusk remove vendor/pkg`
- [ ] `--no-dev`, `--platform`, `--quiet` flags
- [ ] `indicatif` progress UI for download/extract
- [ ] Conflict error UX: human-readable, non-zero exit
- [ ] Source-only package error UX: clear "dist-only in MVP" message

### Step 8 — Phase 1 Definition of Done
- [ ] `tusk install` works on ≥ 3 real projects (small lib, Symfony, Laravel)
- [ ] `composer install` accepts tusk's lock (interop proven)
- [ ] Committed benchmark: tusk faster than composer, cold + warm
- [ ] CI green: tests, clippy `-D warnings`, fmt `--check`
- [ ] `#![forbid(unsafe_code)]` holds across the workspace
- [ ] README install + benchmark table

## Phase 2+ (sketch only, do NOT start until Phase 1 is Done)

See [GOAL.md §9](./GOAL.md).

- **Phase 2:** `tusk serve` / `tusk run` — embed Zend via `ext-php-rs`, SAPI, worker mode (study pasir, FerrumPHP, FrankenPHP)
- **Phase 3:** `tusk test` (PHPUnit/Pest wrapper), `tusk run <script>`
- **Phase 4 (optional, profile-gated):** AOT compile a static PHP subset to native (the `elephc` model)

## Engineering rules (from GOAL.md §6)

- **TDD discipline is non-negotiable.** No production code without a failing test demanding it.
- **No weakening tests** to make them pass.
- **Determinism** is a first-class concern: golden files must be byte-stable.
- **Clippy `-D warnings`** is part of the Definition of Done.
- One behavior per commit, commit at each green point.
