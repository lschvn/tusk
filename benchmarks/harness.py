#!/usr/bin/env python3
"""
tusk vs composer benchmark harness.

Runs both `tusk install` and `composer install` against a fixture project
(composer.json) and times cold and warm cache runs. Writes a JSON result
file with per-tool timings, success flag, package count, and vendor size.

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


def clear_cache_dir(path: str) -> None:
    """Recursively delete a cache directory. Errors are ignored on purpose:
    a missing directory is not an error (the cache may not exist yet)."""
    if not path:
        return
    p = Path(path)
    if p.exists():
        shutil.rmtree(p, ignore_errors=True)


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


def run_composer(
    work_dir: Path, env: dict[str, str]
) -> dict[str, Any]:
    """Run `composer install` in work_dir, then re-run N-1 times for warm.

    Returns a result dict with cold_seconds, warm_seconds (list), and
    metadata. On failure, captures the error and continues."""
    base = [COMPOSER_BIN, "install", "--no-interaction", "--no-progress"]
    # Cold run
    rc, cold_seconds, stderr = run_command(base, work_dir, env=env)
    if rc != 0:
        return {
            "success": False,
            "cold_seconds": round(cold_seconds, 3),
            "warm_seconds": [],
            "warm_avg_seconds": None,
            "warm_min_seconds": None,
            "warm_max_seconds": None,
            "packages_installed": 0,
            "disk_bytes": 0,
            "error": stderr.strip().splitlines()[-20:] if stderr else ["unknown error"],
        }
    # Warm runs (use the existing work_dir; vendor/ already populated)
    warm_times: list[float] = []
    for _ in range(N_WARM_RUNS):
        rc, secs, stderr = run_command(base, work_dir, env=env)
        if rc != 0:
            return {
                "success": False,
                "cold_seconds": round(cold_seconds, 3),
                "warm_seconds": [round(t, 3) for t in warm_times],
                "warm_avg_seconds": None,
                "warm_min_seconds": None,
                "warm_max_seconds": None,
                "packages_installed": count_vendor_packages(work_dir),
                "disk_bytes": disk_usage_bytes(work_dir / "vendor"),
                "error": (stderr.strip().splitlines()[-20:] if stderr else ["unknown"])
                + [f"(warm run failed after {len(warm_times)} successful warm runs)"],
            }
        warm_times.append(secs)
    return {
        "success": True,
        "cold_seconds": round(cold_seconds, 3),
        "warm_seconds": [round(t, 3) for t in warm_times],
        "warm_avg_seconds": round(sum(warm_times) / len(warm_times), 3)
        if warm_times
        else None,
        "warm_min_seconds": round(min(warm_times), 3) if warm_times else None,
        "warm_max_seconds": round(max(warm_times), 3) if warm_times else None,
        "packages_installed": count_vendor_packages(work_dir),
        "disk_bytes": disk_usage_bytes(work_dir / "vendor"),
        "error": None,
    }


def run_tusk(
    work_dir: Path, env: dict[str, str]
) -> dict[str, Any]:
    """Run `tusk install` in work_dir, then re-run N-1 times for warm."""
    base = [TUSK_BIN, "install", "--quiet"]
    rc, cold_seconds, stderr = run_command(base, work_dir, env=env)
    if rc != 0:
        return {
            "success": False,
            "cold_seconds": round(cold_seconds, 3),
            "warm_seconds": [],
            "warm_avg_seconds": None,
            "warm_min_seconds": None,
            "warm_max_seconds": None,
            "packages_installed": 0,
            "disk_bytes": 0,
            "error": stderr.strip().splitlines()[-20:] if stderr else ["unknown error"],
        }
    warm_times: list[float] = []
    for _ in range(N_WARM_RUNS):
        rc, secs, stderr = run_command(base, work_dir, env=env)
        if rc != 0:
            return {
                "success": False,
                "cold_seconds": round(cold_seconds, 3),
                "warm_seconds": [round(t, 3) for t in warm_times],
                "warm_avg_seconds": None,
                "warm_min_seconds": None,
                "warm_max_seconds": None,
                "packages_installed": count_vendor_packages(work_dir),
                "disk_bytes": disk_usage_bytes(work_dir / "vendor"),
                "error": (stderr.strip().splitlines()[-20:] if stderr else ["unknown"])
                + [f"(warm run failed after {len(warm_times)} successful warm runs)"],
            }
        warm_times.append(secs)
    return {
        "success": True,
        "cold_seconds": round(cold_seconds, 3),
        "warm_seconds": [round(t, 3) for t in warm_times],
        "warm_avg_seconds": round(sum(warm_times) / len(warm_times), 3)
        if warm_times
        else None,
        "warm_min_seconds": round(min(warm_times), 3) if warm_times else None,
        "warm_max_seconds": round(max(warm_times), 3) if warm_times else None,
        "packages_installed": count_vendor_packages(work_dir),
        "disk_bytes": disk_usage_bytes(work_dir / "vendor"),
        "error": None,
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
    """Run cold + warm installs for one tool against a fixture."""
    work_dir = temp_root / f"{tool_name}_work"
    work_dir.mkdir(parents=True, exist_ok=True)
    copy_fixture(fixture_dir, work_dir)

    # Clear caches before cold run. We do NOT clear between cold and warm
    # — warm runs are meant to exercise the cache.
    print(f"  [{tool_name}] clearing caches...", file=sys.stderr)
    if tool_name == "tusk":
        clear_cache_dir(TUSK_CACHE_DIR)
    else:
        clear_cache_dir(COMPOSER_CACHE_DIR)
        clear_cache_dir(COMPOSER_HOME_DIR + "/cache")

    print(f"  [{tool_name}] cold run...", file=sys.stderr)
    if tool_name == "tusk":
        result = run_tusk(work_dir, env)
    else:
        result = run_composer(work_dir, env)

    result["tool"] = tool_name
    result["tusk_cache_dir"] = TUSK_CACHE_DIR
    result["composer_cache_dir"] = COMPOSER_CACHE_DIR
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

    cold_speedup = (
        _safe_div(composer_r.get("cold_seconds"), tusk_r.get("cold_seconds"))
        if (composer_ok and tusk_ok)
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
        notes.append("composer install FAILED — see tools.composer.error")
    if not tusk_ok:
        notes.append("tusk install FAILED — see tools.tusk.error")

    results["summary"] = {
        "cold_speedup": cold_speedup,
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
