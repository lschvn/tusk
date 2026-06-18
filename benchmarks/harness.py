#!/usr/bin/env python3
"""
tusk vs composer benchmark harness.

Runs both `tusk install` and `composer install` against a fixture project
(composer.json) and times three scenarios for each tool:

  1. cold_no_lockfile  — caches cleared AND composer.lock removed.
                         The tool must resolve dependencies (no fast path).
  2. cold_with_lockfile — caches cleared, composer.lock KEPT (created
                         by the prior cold_no_lock run). The lockfile
                         fast path skips the resolver; only download
                         and extract remain. This is the realistic CI
                         scenario: you `git pull` an unchanged lockfile
                         and reinstall into a fresh container.
  3. warm              — N re-runs against a warm archive cache.

Writes a JSON result file with per-tool timings, success flag, package
count, and vendor size.

Usage:
    python3 harness.py <fixture_dir> --output <json_path> [--runs N]

Stdlib only — no external deps.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# All external tool paths. Override with env vars if needed.
TUSK_BIN = os.environ.get("TUSK_BIN", "/mnt/data/cargo-target/release/tusk")
PHP_BIN = os.environ.get("PHP_BIN", "/home/louis/php/php")
COMPOSER_BIN = os.environ.get("COMPOSER_BIN", "/home/louis/php/composer")

# Default cache locations. Cleared before each cold run.
TUSK_CACHE_DIR = os.environ.get("TUSK_CACHE_DIR", str(Path.home() / ".cache" / "tusk"))
COMPOSER_CACHE_DIR = os.environ.get(
    "COMPOSER_CACHE_DIR", str(Path.home() / ".composer" / "cache")
)
# Composer's own metadata + dist cache (separate from the per-project
# `vendor/`). Composer uses `COMPOSER_HOME/cache/files` for downloaded
# zips and `COMPOSER_HOME/cache/repo` for metadata.
COMPOSER_HOME_DIR = os.environ.get(
    "COMPOSER_HOME_DIR", str(Path.home() / ".composer")
)

# Per-run timeout. Some large installs can take a while on slow networks.
INSTALL_TIMEOUT_SECONDS = 600


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def clear_cache_dir(path: str, preserve: list[str] | None = None) -> None:
    """Recursively delete a cache directory. Errors are ignored on purpose:
    a missing directory is not an error (the cache may not exist yet).

    `preserve` is an optional list of basenames (e.g. ["composer.lock"]) that
    should be kept if present in the tree. Note: in practice, the lockfile
    lives in the per-fixture work_dir, not the cache, so this is here for
    future use (e.g. a composer-style key+value cache)."""
    if not path:
        return
    p = Path(path)
    if not p.exists():
        return
    if not preserve:
        shutil.rmtree(p, ignore_errors=True)
        return
    # Move preserved entries out, rmtree, then move them back.
    sandbox = p.parent / f".{p.name}_preserve_{os.getpid()}"
    sandbox.mkdir(exist_ok=True)
    moved: list[tuple[Path, Path]] = []
    for name in preserve:
        src = p / name
        if src.exists():
            dst = sandbox / name
            shutil.move(str(src), str(dst))
            moved.append((src, dst))
    shutil.rmtree(p, ignore_errors=True)
    p.mkdir(parents=True, exist_ok=True)
    for src, dst in moved:
        if dst.exists():
            shutil.move(str(dst), str(src))
    shutil.rmtree(sandbox, ignore_errors=True)


def copy_fixture(fixture_dir: Path, work_dir: Path) -> None:
    """Copy the fixture's composer.json (and any other files) into work_dir."""
    if not fixture_dir.is_dir():
        raise FileNotFoundError(f"fixture dir not found: {fixture_dir}")
    cj = fixture_dir / "composer.json"
    if not cj.is_file():
        raise FileNotFoundError(f"composer.json not found in {fixture_dir}")
    shutil.copy2(cj, work_dir / "composer.json")


def count_vendor_packages(work_dir: Path) -> int:
    """Count top-level vendor/{vendor}/{package} directories.

    A successful composer/tusk install lays out packages as
    `vendor/<vendor>/<package>/`. We count <package> entries across all
    vendor subdirectories and exclude Composer's own `composer` dir
    (which contains the installer package, not a dependency)."""
    vendor = work_dir / "vendor"
    if not vendor.is_dir():
        return 0
    total = 0
    for vendor_entry in vendor.iterdir():
        if not vendor_entry.is_dir():
            continue
        if vendor_entry.name == "composer":
            # The bundled `composer/installers` and `composer/...` are
            # auxiliary, but we still count them — they were installed.
            pass
        for pkg_entry in vendor_entry.iterdir():
            if pkg_entry.is_dir():
                total += 1
    return total


def disk_usage_bytes(path: Path) -> int:
    """Return total size of `path` in bytes (recursive), or 0 if missing.

    Uses `du -sb` to follow symlinks and account for sparse files, which
    matches what `du` reports in the shell. Falls back to a Python walk
    if `du` is unavailable."""
    if not path.exists():
        return 0
    try:
        result = subprocess.run(
            ["du", "-sb", str(path)],
            capture_output=True,
            text=True,
            timeout=60,
            check=True,
        )
        # Output is `<size>\t<path>`, take the first field.
        return int(result.stdout.split("\t", 1)[0])
    except (subprocess.SubprocessError, FileNotFoundError, ValueError):
        # Fallback: sum of file sizes (may be slower for huge trees).
        total = 0
        for root, _dirs, files in os.walk(path, followlinks=True):
            for f in files:
                try:
                    total += (Path(root) / f).stat().st_size
                except OSError:
                    pass
        return total


def run_command(
    cmd: list[str],
    cwd: Path,
    env: dict[str, str] | None = None,
    timeout: int = INSTALL_TIMEOUT_SECONDS,
) -> tuple[int, float, str]:
    """Run a subprocess and return (returncode, wall_clock_seconds, stderr).

    The wall clock is measured with `time.time()` and includes subprocess
    startup, network I/O, and shutdown — exactly what a user experiences."""
    start = time.time()
    try:
        proc = subprocess.run(
            cmd,
            cwd=str(cwd),
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        elapsed = time.time() - start
        return proc.returncode, elapsed, proc.stderr or ""
    except subprocess.TimeoutExpired as e:
        elapsed = time.time() - start
        return -1, elapsed, f"timeout after {timeout}s: {e}"


# ---------------------------------------------------------------------------
# Per-tool runners
# ---------------------------------------------------------------------------


def build_env() -> dict[str, str]:
    """Build a clean environment for subprocesses.

    We start from the current process env and override only the vars we
    need. The PATH is extended with ~/php so `composer` resolves to the
    standalone install."""
    env = os.environ.copy()
    extra = f"{Path.home()}/php"
    env["PATH"] = f"{extra}:{env.get('PATH', '')}"
    return env


def tool_command(tool_name: str) -> list[str]:
    """Return the base install command for a given tool."""
    if tool_name == "tusk":
        return [TUSK_BIN, "install", "--quiet"]
    if tool_name == "composer":
        return [COMPOSER_BIN, "install", "--no-interaction", "--no-progress"]
    raise ValueError(f"unknown tool: {tool_name}")


def tool_cache_dirs(tool_name: str) -> list[str]:
    """Return the list of cache directories to clear before a cold run."""
    if tool_name == "tusk":
        return [TUSK_CACHE_DIR]
    if tool_name == "composer":
        return [COMPOSER_CACHE_DIR, str(Path(COMPOSER_HOME_DIR) / "cache")]
    raise ValueError(f"unknown tool: {tool_name}")


def run_install(
    cmd: list[str], work_dir: Path, env: dict[str, str]
) -> dict[str, Any]:
    """Run a single install command and return a small result dict.

    Always runs the install exactly once; the caller decides how many
    times to repeat for warm averaging. This is the unit of work that
    the orchestrator (`benchmark_tool`) composes into the three
    scenarios: cold_no_lockfile, cold_with_lockfile, warm."""
    rc, secs, stderr = run_command(cmd, work_dir, env=env)
    return {
        "rc": rc,
        "seconds": round(secs, 3) if rc >= 0 else round(secs, 3),
        "stderr_tail": (
            [ln for ln in stderr.strip().splitlines() if ln][-20:]
            if stderr
            else []
        ),
    }


# ---------------------------------------------------------------------------
# Top-level benchmark driver
# ---------------------------------------------------------------------------


# Global so the per-tool functions can see it. Set by main().
N_WARM_RUNS = 3


def benchmark_tool(
    tool_name: str,
    fixture_dir: Path,
    env: dict[str, str],
    temp_root: Path,
) -> dict[str, Any]:
    """Run cold_no_lock + cold_with_lock + warm installs for one tool.

    Three scenarios, in order:
      1. `cold_no_lockfile`   — clear cache + delete composer.lock. The
         tool has to resolve dependencies (no fast path).
      2. `cold_with_lockfile` — clear cache but KEEP composer.lock
         (created by scenario 1). The tool's lockfile fast path
         skips the resolver; only download + extract remain. This is
         the realistic CI scenario: you `git pull` an unchanged
         composer.lock and reinstall into a fresh container.
      3. `warm`               — re-run with a warm archive cache, N
         times. Mirrors the original benchmark.
    """
    work_dir = temp_root / f"{tool_name}_work"
    # Start from a known-clean state every invocation.
    if work_dir.exists():
        shutil.rmtree(work_dir, ignore_errors=True)
    work_dir.mkdir(parents=True, exist_ok=True)
    copy_fixture(fixture_dir, work_dir)

    cmd = tool_command(tool_name)
    cache_dirs = tool_cache_dirs(tool_name)
    lockfile = work_dir / "composer.lock"

    result: dict[str, Any] = {
        "tool": tool_name,
        "tusk_cache_dir": TUSK_CACHE_DIR,
        "composer_cache_dir": COMPOSER_CACHE_DIR,
    }
    notes: list[str] = []

    # ------------------------------------------------------------------
    # Scenario 1: cold_no_lockfile (the original "cold" run)
    # ------------------------------------------------------------------
    print(
        f"  [{tool_name}] clearing caches + lockfile (cold_no_lockfile)...",
        file=sys.stderr,
    )
    for c in cache_dirs:
        clear_cache_dir(c)
    if lockfile.is_file():
        lockfile.unlink()
    # Vendor/ should not exist yet (fresh work_dir), but be defensive.
    vendor = work_dir / "vendor"
    if vendor.exists():
        shutil.rmtree(vendor, ignore_errors=True)

    print(f"  [{tool_name}] cold run (no lockfile)...", file=sys.stderr)
    cold = run_install(cmd, work_dir, env)
    cold_no_lock_ok = cold["rc"] == 0
    result["cold_seconds"] = cold["seconds"] if cold_no_lock_ok else None
    result["cold_error"] = cold["stderr_tail"] if not cold_no_lock_ok else None
    if not cold_no_lock_ok:
        notes.append(
            f"{tool_name} cold_no_lockfile install FAILED — see tools.{tool_name}.cold_error"
        )

    # ------------------------------------------------------------------
    # Scenario 2: cold_with_lockfile (lockfile fast path)
    # ------------------------------------------------------------------
    result["cold_with_lockfile_seconds"] = None
    result["cold_with_lockfile_error"] = None
    if cold_no_lock_ok and lockfile.is_file():
        print(
            f"  [{tool_name}] clearing cache (keeping lockfile)...",
            file=sys.stderr,
        )
        for c in cache_dirs:
            clear_cache_dir(c)
        # Vendor/ already populated from scenario 1; the install will
        # re-extract on top of it, which is what we want to time.

        print(
            f"  [{tool_name}] cold run (with lockfile)...", file=sys.stderr
        )
        cold_lk = run_install(cmd, work_dir, env)
        cold_lk_ok = cold_lk["rc"] == 0
        result["cold_with_lockfile_seconds"] = (
            cold_lk["seconds"] if cold_lk_ok else None
        )
        if not cold_lk_ok:
            result["cold_with_lockfile_error"] = cold_lk["stderr_tail"]
            notes.append(
                f"{tool_name} cold_with_lockfile install FAILED — see tools.{tool_name}.cold_with_lockfile_error"
            )
    else:
        notes.append(
            f"{tool_name} cold_with_lockfile SKIPPED: no composer.lock was created by the cold_no_lockfile run"
        )

    # ------------------------------------------------------------------
    # Scenario 3: warm (warm cache, lockfile present)
    # ------------------------------------------------------------------
    print(
        f"  [{tool_name}] warm runs (x{N_WARM_RUNS})...", file=sys.stderr
    )
    warm_times: list[float] = []
    warm_error: list[str] | None = None
    for _ in range(N_WARM_RUNS):
        warm = run_install(cmd, work_dir, env)
        if warm["rc"] == 0:
            warm_times.append(warm["seconds"])
        else:
            warm_error = warm["stderr_tail"]
            break
    result["warm_seconds"] = [round(t, 3) for t in warm_times]
    result["warm_avg_seconds"] = (
        round(sum(warm_times) / len(warm_times), 3) if warm_times else None
    )
    result["warm_min_seconds"] = (
        round(min(warm_times), 3) if warm_times else None
    )
    result["warm_max_seconds"] = (
        round(max(warm_times), 3) if warm_times else None
    )
    if warm_error is not None:
        notes.append(
            f"{tool_name} warm install FAILED after {len(warm_times)} successful runs"
        )

    # Final metadata, captured at the very end so disk_bytes reflects
    # the final vendor/ layout (which scenario 2 may have re-populated).
    result["success"] = cold_no_lock_ok
    result["packages_installed"] = count_vendor_packages(work_dir)
    result["disk_bytes"] = disk_usage_bytes(work_dir / "vendor")
    result["notes"] = notes
    return result


def collect_environment() -> dict[str, Any]:
    """Snapshot the runtime environment for the result file."""
    env_info: dict[str, Any] = {}

    def _run(cmd: list[str], default: str = "unknown") -> str:
        try:
            r = subprocess.run(
                cmd, capture_output=True, text=True, timeout=10, check=True
            )
            return r.stdout.strip()
        except (subprocess.SubprocessError, FileNotFoundError) as e:
            return f"{default} ({e})"

    env_info["os"] = _run(["bash", "-c", ". /etc/os-release && echo $PRETTY_NAME"])
    env_info["kernel"] = _run(["uname", "-r"])
    env_info["cpu_model"] = (
        _run(["bash", "-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"]).strip()
    )
    env_info["cpu_count"] = _run(["nproc"])
    env_info["memory"] = _run(["bash", "-c", "free -h | head -2 | tail -1"])
    env_info["php_version"] = _run([PHP_BIN, "--version"], default=PHP_BIN)
    env_info["composer_version"] = _run(
        [COMPOSER_BIN, "--version", "--no-ansi"], default=COMPOSER_BIN
    )
    env_info["tusk_version"] = _run([TUSK_BIN, "--version"], default=TUSK_BIN)
    # git commit (best-effort; the tusk repo path is the cwd)
    env_info["tusk_commit"] = _run(
        ["git", "rev-parse", "HEAD"], default="(no git)"
    )
    env_info["tusk_commit_short"] = _run(
        ["git", "rev-parse", "--short", "HEAD"], default="(no git)"
    )
    return env_info


def main() -> int:
    parser = argparse.ArgumentParser(
        description="tusk vs composer benchmark harness"
    )
    parser.add_argument("fixture_dir", type=Path, help="path to fixture dir")
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="where to write the JSON result",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=3,
        help="number of warm runs per tool (default: 3)",
    )
    args = parser.parse_args()

    global N_WARM_RUNS
    N_WARM_RUNS = max(1, args.runs)

    fixture_dir: Path = args.fixture_dir.resolve()
    output_path: Path = args.output.resolve()

    # Sanity: required tools exist.
    for bin_path, name in [
        (TUSK_BIN, "tusk"),
        (PHP_BIN, "php"),
        (COMPOSER_BIN, "composer"),
    ]:
        if not Path(bin_path).exists():
            print(f"FATAL: {name} not found at {bin_path}", file=sys.stderr)
            return 2

    # Read fixture's direct dep count (we approximate total deps from results).
    fixture_cj = json.loads((fixture_dir / "composer.json").read_text())
    direct_deps = len(fixture_cj.get("require", {})) + len(
        fixture_cj.get("require-dev", {})
    )

    env = build_env()
    environment = collect_environment()

    print(
        f"Benchmarking fixture={fixture_dir.name} "
        f"({direct_deps} direct deps), warm_runs={N_WARM_RUNS}",
        file=sys.stderr,
    )

    results: dict[str, Any] = {
        "fixture": fixture_dir.name,
        "fixture_path": str(fixture_dir),
        "direct_deps": direct_deps,
        "warm_runs": N_WARM_RUNS,
        "environment": environment,
        "tools": {},
    }

    with tempfile.TemporaryDirectory(prefix="tusk_bench_") as temp_root_str:
        temp_root = Path(temp_root_str)
        # Always run composer first (more reliable, fills the playing field),
        # then tusk. Order doesn't affect correctness; both get a clean cache
        # and a fresh work dir.
        for tool in ("composer", "tusk"):
            results["tools"][tool] = benchmark_tool(
                tool, fixture_dir, env, temp_root
            )

    # Derive a total-deps count for each tool from `packages_installed`.
    for tool, r in results["tools"].items():
        if r.get("success"):
            r["total_deps_resolved"] = r["packages_installed"]
        else:
            r["total_deps_resolved"] = None

    # Derive speedup numbers (composer / tusk), with None when one side failed.
    composer_r = results["tools"].get("composer", {})
    tusk_r = results["tools"].get("tusk", {})

    def _safe_div(a, b):
        if a is None or b is None or a == 0:
            return None
        return round(b / a, 2)

    # Speedup is only meaningful when BOTH tools succeeded. If one tool
    # failed, its "seconds" value measures time-to-failure, not real work,
    # and a ratio would be misleading. In that case the summary reports
    # None and a "note" string.
    composer_ok = composer_r.get("success")
    tusk_ok = tusk_r.get("success")

    # cold_no_lockfile: classic "cold" — no lockfile, no cache, full
    # resolver path. Same as the original benchmark.
    cold_no_lock_speedup = (
        _safe_div(composer_r.get("cold_seconds"), tusk_r.get("cold_seconds"))
        if (composer_ok and tusk_ok)
        else None
    )
    # cold_with_lockfile: the new scenario — lockfile present, archive
    # cache empty. This is the CI/git-pull fast path.
    cold_with_lock_speedup = (
        _safe_div(
            composer_r.get("cold_with_lockfile_seconds"),
            tusk_r.get("cold_with_lockfile_seconds"),
        )
        if (
            composer_ok
            and tusk_ok
            and composer_r.get("cold_with_lockfile_seconds") is not None
            and tusk_r.get("cold_with_lockfile_seconds") is not None
        )
        else None
    )
    warm_speedup_avg = (
        _safe_div(
            composer_r.get("warm_avg_seconds"), tusk_r.get("warm_avg_seconds")
        )
        if (composer_ok and tusk_ok)
        else None
    )
    # For warm, worst-case composer vs best-case tusk: only meaningful
    # when both succeeded.
    warm_speedup_min = (
        _safe_div(
            composer_r.get("warm_max_seconds"), tusk_r.get("warm_min_seconds")
        )
        if (composer_ok and tusk_ok)
        else None
    )

    notes: list[str] = []
    if not composer_ok:
        notes.append("composer install FAILED — see tools.composer.cold_error")
    if not tusk_ok:
        notes.append("tusk install FAILED — see tools.tusk.cold_error")
    # Surface per-tool scenario notes too (informational).
    for tool, r in results["tools"].items():
        for n in r.get("notes", []):
            notes.append(f"{tool}: {n}")

    results["summary"] = {
        "cold_no_lockfile_speedup": cold_no_lock_speedup,
        "cold_with_lockfile_speedup": cold_with_lock_speedup,
        "warm_speedup_avg": warm_speedup_avg,
        "warm_speedup_min": warm_speedup_min,
        "composer_success": composer_ok,
        "tusk_success": tusk_ok,
        "notes": notes,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(results, indent=2, sort_keys=True))
    print(f"Wrote {output_path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
