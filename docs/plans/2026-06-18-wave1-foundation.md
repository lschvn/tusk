# Wave 1 — Foundation crates (tusk-semver, tusk-manifest, tusk-registry)

> **For Hermes:** Use subagent-driven-development (salvo mode) to implement this plan with 3 parallel subagents.
>
> **Goal:** Bring the 3 foundation crates from `unimplemented!()` stubs to fully tested, clippy-clean implementations. After this wave: `tusk install` will be able to (a) parse `composer.json`, (b) talk to a `Registry` (mocked for tests), and (c) reason about Composer version constraints.

**Architecture:** Each crate's public API is already defined in the scaffold. The subagent's job is to write tests first, then fill in the bodies. The subagents MUST NOT change the public API — only implement it.

**Tech Stack:** Rust 1.96, edition 2021, `#![forbid(unsafe_code)]` workspace-wide, `pubgrub` 0.2, `serde` + `serde_json` + `indexmap` (preserve_order), `tokio` + `reqwest` (rustls) + `wiremock`.

---

## House rules — read these FIRST

1. **TDD Iron Law (from the TDD skill):** No production code without a failing test first. Watch it fail. Write minimum code. Watch it pass. Refactor. Commit.
2. **The scaffold's public API is fixed.** Stubs in `crates/<crate>/src/*.rs` already define the public types and method signatures. DO NOT rename, reorder, or change them — other subagents and crates depend on the shape. You may add private helpers, but the public surface is the contract.
3. **The scaffold has `unimplemented!()` bodies.** Your job is to replace them with real implementations, one test at a time.
4. **Tool pitfalls (read_file returns `     N|` line-number prefixes; do NOT pass that into `write_file` — it will embed the numbers as file content).** Use `patch()` for targeted edits, or `terminal("cat path")` for raw content. Read with `read_file`, write with `write_file` (full overwrite) or `patch` (fuzzy match).
5. **You MUST actually write the files** using `write_file` or `patch`. Do NOT just describe what you would do. (GLM models sometimes fall into a "read-only" mode — make sure you call write_file/patch for real.)
6. **You MUST verify your work on disk at the end.** `wc -l crates/<crate>/src/*.rs` and `cat crates/<crate>/src/lib.rs` to confirm.
7. **Workspace-wide lints:** `clippy::pedantic` is enabled; `unsafe_code = "forbid"`. Treat clippy warnings as errors.
8. **Commit messages:** one TDD cycle (or closely related cycles) per commit. Format: `feat: <thing>` or `test: <thing>`.

## Test command

```bash
cd ~/work/tusk && source "$HOME/.cargo/env" \
  && cargo test -p <crate> --all-features -- --nocapture
```

## Lint + format gate (must pass before declaring done)

```bash
cd ~/work/tusk && source "$HOME/.cargo/env" \
  && cargo fmt --all -- --check \
  && cargo clippy -p <crate> --all-targets -- -D warnings
```

---

## Subagent A — `tusk-semver` (the algorithmic heart, build first)

**Spec reference:** `GOAL.md` §7.1. This is the most subtle crate. The Composer constraint grammar is the spec: <https://getcomposer.org/doc/articles/versions.md>.

**Files you own (only these):**
- `crates/tusk-semver/Cargo.toml` (already correct, do not edit)
- `crates/tusk-semver/src/lib.rs` (do not edit the `mod` declarations or pub use lines)
- `crates/tusk-semver/src/version.rs`
- `crates/tusk-semver/src/constraint.rs`
- `crates/tusk-semver/tests/` (create new test files here)

**Public API to preserve (defined in stubs, do not change):**
- `pub enum Stability { Dev, Alpha, Beta, Rc, Stable }`
- `pub struct Version { major, minor, patch, tweak: Option<u32>, stability, stability_n: Option<u32>, dev_branch: Option<String>, is_v_prefixed: bool }`
- `pub struct Constraint { branches: Vec<Branch> }`
- `pub struct Branch { atoms: Vec<Atom> }`
- `pub enum Atom { Exact(Version), Caret { lower: Version }, Tilde { lower: Version }, Cmp { op: CmpOp, version: Version }, Hyphen { lower: Version, upper: Version }, StabilityFlag(StabilityFlag) }`
- `pub enum CmpOp { Gt, Ge, Lt, Le, Ne }`
- `pub enum StabilityFlag { Dev, Alpha, Beta, Rc, Stable }`
- `Stability::from_suffix(&str) -> Option<Self>`, `Stability::as_str(self) -> &'static str`
- `Version::parse(&str) -> Result<Self, VersionError>`, `Version::to_composer_string(&self) -> String`
- `Constraint::parse(&str) -> Result<Self, ConstraintError>`, `Constraint::matches(&self, &Version) -> bool`
- `ConstraintParser::parse(&str) -> Result<Constraint, ConstraintError>`

**TDD task list (each task = 1 commit):**

1. **Version parse — basic numeric.** Test: `Version::parse("1.2.3")` → `Version { major:1, minor:2, patch:3, tweak:None, stability:Stable, ... }`. Implement, commit.
2. **Version parse — `v` prefix tolerated on input, stripped on output.** Test: `parse("v2.5.0")` succeeds; `to_composer_string` → `"2.5.0"`. Implement, commit.
3. **Version parse — 4-component.** Test: `parse("1.2.3.4")` → tweak=Some(4). Implement, commit.
4. **Version parse — stability suffix.** Tests for `-alpha`, `-alpha.2`, `-beta.1`, `-RC1`, `-dev`, `-pl3`. Implement, commit.
5. **Version parse — `dev-<branch>`.** Test: `parse("dev-main")` → `Version { stability:Dev, dev_branch:Some("main"), major:0, ... }`. Implement, commit.
6. **Stability ordering.** Tests: `Dev < Alpha < Beta < Rc < Stable` (this is the derive(Ord), already correct — just write the test, run it, commit).
7. **Constraint parse — exact.** Test: `Constraint::parse("1.2.3").matches(&v(1,2,3)) == true`; same for `1.2.4` → false. Implement, commit.
8. **Constraint parse — `^`.** Test: `^1.2` matches 1.2.0, 1.5.7, NOT 2.0.0, NOT 1.1.9. `^1.2.3` matches 1.2.3, 1.2.99, NOT 1.2.2, NOT 2.0.0. Implement, commit.
9. **Constraint parse — `~`.** Test: `~1.2` matches 1.2.0..1.3.0; `~1.2.3` matches 1.2.3..1.3.0. Implement, commit.
10. **Constraint parse — `*`, `1.2.*`.** Test: `1.2.*` matches 1.2.0, 1.2.99, NOT 1.3.0. Implement, commit.
11. **Constraint parse — `>=`, `>`, `<=`, `<`, `!=`.** Tests for each, with the `>` vs `>=` boundary cases. Implement, commit.
12. **Constraint parse — hyphen range `1.2 - 3.4`.** Test: matches 1.2.0..3.4.0 (inclusive both ends). Implement, commit.
13. **Constraint parse — OR `||` and AND `,`.** Test: `1.2 || >=2.0` matches 1.2.3 and 2.5.0 and 3.0.0. `>=1.0,<2.0` matches 1.5.0 not 2.0.0. Implement, commit.
14. **Constraint parse — `@dev`, `@stable`, `@alpha` stability flags.** Test: `1.2.3@dev` matches `1.2.3-dev` but not `1.2.3`. Implement, commit.
15. **Constraint parse — whitespace + ordering (lowest version first for `,`).** Test: `<2.0,>=1.0` works. Implement, commit.
16. **Property test (proptest).** For any version string, `parse(s).to_composer_string() == s` (modulo the `v` prefix and stability normalization). Implement, commit.

**Verify green:** `cargo test -p tusk-semver --all-features` — all pass. `cargo clippy -p tusk-semver --all-targets -- -D warnings` — clean.

**Out of scope:** Don't add `pubgrub::VersionLike` impl yet (that's a Tuskmover task). Don't add a Display impl beyond `to_composer_string`.

---

## Subagent B — `tusk-manifest` (composer.json + composer.lock)

**Spec reference:** `GOAL.md` §7.2.

**Files you own (only these):**
- `crates/tusk-manifest/src/composer_json.rs`
- `crates/tusk-manifest/src/composer_lock.rs`
- `crates/tusk-manifest/tests/` (create new test files)
- `crates/tusk-manifest/src/lib.rs` — DO NOT edit the mod declarations or pub use lines

**Public API to preserve (defined in stubs):**
- `pub struct ComposerJson` with `name: Option<String>`, `require: RequireMap`, `require_dev: RequireMap`, `autoload: Autoload`, `autoload_dev: AutoloadDev`, `repositories: Vec<serde_json::Value>`, `config: serde_json::Value`, `minimum_stability: Option<String>`, `prefer_stable: bool`. Method: `from_str(&str) -> Result<Self, ManifestError>`.
- `pub struct ComposerLock` with `readme: Option<Vec<String>>`, `content_hash: Option<String>`, `packages: Vec<LockedPackage>`, `packages_dev: Vec<LockedPackage>`, `aliases: IndexMap<String,String>`, `minimum_stability: String`, `stability_flags: IndexMap<String,String>`, `prefer_stable: bool`, `prefer_lowest: bool`, `platform: IndexMap<String,String>`, `platform_dev: IndexMap<String,String>`.
- `pub struct LockedPackage` and `pub struct Dist` (shapes already in stub).
- `pub type RequireMap = IndexMap<String, String>`.

**Fixtures to add (in `fixtures/manifest/`):**
- `minimal.json` — `{ "name": "foo/bar", "require": {"php": "^8.1"} }`
- `laravel.json` — copy a real Laravel skeleton's composer.json (use a small representative one; if you can't reach github, hand-craft one with the same shape)
- `symfony.json` — hand-crafted or fixture
- `multipath-psr4.json` — composer.json with multiple PSR-4 namespaces
- `autoload-files.json` — composer.json with `"autoload": { "files": ["src/helpers.php"] }`
- `composer.lock` — a real Composer lock file you can find in any open-source PHP project's repo. Commit it as the golden file.

**TDD task list (each = 1 commit):**

1. **`ComposerJson::from_str` — minimal.** Test: `from_str(minimal.json)?` succeeds, `require["php"] == "^8.1"`. Implement, commit.
2. **Parse `require-dev`.** Test: minimal + `require-dev` round-trips with same key. Implement, commit.
3. **Parse multi-path PSR-4.** Test: multipath fixture deserializes both PSR-4 namespaces. Implement, commit.
4. **Parse `autoload.files`.** Test: `autoload.files` is a `Vec<String>`. Implement, commit.
5. **Parse `classmap` and `psr-0`.** Tests. Implement, commit.
6. **Parse Laravel-style with `repositories`, `config`, `minimum-stability`, `prefer-stable`.** Test using the laravel fixture. Implement, commit.
7. **`ComposerLock` deserialization — round-trip with the golden file.** Test: `from_str(golden_lock) == golden_lock`. Snapshot with `insta` for byte stability. Implement, commit.
8. **Key order in `packages` is preserved** (using `indexmap` + `serde_json(preserve_order)`). Test: parse the golden file, re-serialize, byte-compare. Implement, commit.
9. **Error reporting: missing required `name` field** should NOT fail for `require`-only manifests (Composer allows them). Test: minimal.json has no `name` and parses fine. Implement, commit.
10. **Unknown top-level fields are ignored** (forward compat — Composer adds fields often). Test: a JSON with `"extra-weird-field": "x"` parses without error. Implement, commit.

**Verify green:** `cargo test -p tusk-manifest --all-features` — all pass. `cargo clippy -p tusk-manifest --all-targets -- -D warnings` — clean.

---

## Subagent C — `tusk-registry` (Packagist client + MockRegistry)

**Spec reference:** `GOAL.md` §5, §7.3.

**Files you own (only these):**
- `crates/tusk-registry/src/client.rs` — `Registry` trait, `RegistryError`, `PackagistClient` impl
- `crates/tusk-registry/src/packagist.rs` — `PackagistClient` construction + p2 metadata parsing
- `crates/tusk-registry/src/mock.rs` — `MockRegistry` (already partially written, fill it in)
- `crates/tusk-registry/tests/` — create new test files
- `crates/tusk-registry/src/lib.rs` — DO NOT edit mod declarations

**Public API to preserve:**
- `pub trait Registry: Send + Sync { async fn package_metadata(&self, vendor: &str, package: &str) -> Result<PackageMetadata, RegistryError>; }`
- `pub struct PackageMetadata { versions: Vec<PackageVersion> }`
- `pub struct PackageVersion { version: Version, dist: DistRef, require: RequireMap }`
- `pub struct DistRef { url: String, shasum: String, r#type: String }`
- `pub enum RegistryError { Network(String), Parse(String), NotFound(String) }`
- `PackagistClient::new(base_url: impl Into<String>) -> Self`
- `MockRegistry::new() -> Self`, `MockRegistry::with_package(self, name: &str, metadata: PackageMetadata) -> Self`

**TDD task list (each = 1 commit):**

1. **`MockRegistry::package_metadata` returns the inserted metadata.** Test: insert `{vendor:"acme", package:"foo"}` with one version; fetch; assert. The mock is the foundation for ALL other tests in the workspace — it must be solid. Implement, commit.
2. **`MockRegistry` returns `RegistryError::NotFound` for unknown packages.** Test: fetch `unknown/x` → `Err(NotFound(...))`. Implement, commit.
3. **`MockRegistry` is `Send + Sync` and supports concurrent fetch.** Test: spawn 4 tokio tasks each fetching the same package; assert all succeed. Implement, commit.
4. **`PackagistClient::new` accepts a custom `base_url`** (default: `https://repo.packagist.org`). Test: build with custom URL, assert the field. Implement, commit.
5. **`PackagistClient::package_metadata` against `wiremock`: parses a real-shape p2 response.** Test: spin up wiremock, return a JSON body matching the p2 schema (a JSON object with a `"packages"` key wrapping a `{vendor}/{name}` key whose value is a list of version objects). Assert it deserializes into `PackageMetadata`. Implement, commit.
6. **PackagistClient in-process cache: second fetch within a run = 0 HTTP calls.** Test: wiremock counts requests; call `package_metadata` twice; assert 1 request, not 2. Use `Arc<Mutex<HashMap>>` or `dashmap`. Implement, commit.
7. **PackagistClient surfaces HTTP errors as `RegistryError::Network`, not panic.** Test: wiremock returns 500; assert `Err(Network(...))`. Implement, commit.
8. **PackagistClient surfaces malformed JSON as `RegistryError::Parse`.** Test: wiremock returns `not json`; assert `Err(Parse(...))`. Implement, commit.
9. **DistRef is extracted correctly** (url, shasum, type="zip"). Test: response with dist.url, dist.shasum, dist.type. Implement, commit.
10. **Each version's `require` map is deserialized.** Test: response with `require: {php: "^8.1", ext-json: "*"}`; assert the IndexMap. Implement, commit.

**Verify green:** `cargo test -p tusk-registry --all-features` — all pass. `cargo clippy -p tusk-registry --all-targets -- -D warnings` — clean. **NO TEST may hit the real network** (per `GOAL.md` §5).

**Out of scope:** Don't implement `tusk-installer` here, even though it will use `DistRef`. Don't implement authentication / private repositories.

---

## Verification across the salvo

After all 3 subagents return, the parent session will run:

```bash
cd ~/work/tusk && source "$HOME/.cargo/env" \
  && cargo test --all --all-features \
  && cargo clippy --all-targets -- -D warnings \
  && cargo fmt --all -- --check
```

If any crate fails to compile because another subagent changed a public API, the parent session will fix the mismatch in 1-2 lines — do NOT have subagents cross-edit each other's files.

## Pitfalls / known traps

- **read_file returns `     N|` line-number prefixes.** Use `patch()` for edits. Never pipe read_file content into write_file.
- **GLM subagent read-only failure mode:** Some subagent models read but don't write. If you finish a task and `ls crates/<crate>/src/` shows the file unchanged, you didn't actually save — re-call `write_file` with the full content.
- **clippy::pedantic** will flag many stylistic things. Run `cargo clippy -p <crate> --all-targets` early and often. Add `#[allow(...)]` only when justified; pedantic lints are warnings, not errors, so genuine issues should be fixed.
- **Forbidding `unsafe_code`** is at the workspace lints level — `unsafe` blocks will be a hard error. There is no reason to need `unsafe` in any of these crates.
- **Cargo.lock is checked in** for the workspace. Don't `git rm` it.
- **`fixtures/` is at the workspace root**, not inside a crate. `git mv` is fine; just don't put it in `target/`.
