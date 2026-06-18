# Tusk benchmarks

End-to-end performance comparison: **tusk** (Rust) vs **Composer** (PHP).

## What's here

```
benchmarks/
├── harness.py             # Python benchmark orchestrator (stdlib only)
├── fixtures/              # Three test project composer.json files
│   ├── small/             # 3 direct deps (psr/log, psr/container, php)
│   ├── medium/            # 18 direct deps (17 illuminate/* components)
│   └── large/             # 13 direct deps (12 symfony/* + twig)
├── results/               # JSON output of each benchmark run
│   ├── small.json
│   ├── medium.json
│   └── large.json
├── REPORT.md              # The actual benchmark results + analysis
└── README.md              # This file
```

## Requirements

- **Python 3.11+** (no external packages — stdlib only)
- **PHP 8.3+** at `~/php/php` (standalone install, not via apt)
- **Composer 2.x** at `~/php/composer` (standalone install)
- **tusk** release binary at `/mnt/data/cargo-target/release/tusk`
  (build with `cargo build --release` in the tusk repo root)

If your paths differ, edit `harness.py` — see the `TUSK_BIN`, `PHP_BIN`,
`COMPOSER_BIN` constants at the top of the file.

## How to run

```bash
# Add PHP/composer to PATH
export PATH="$HOME/php:$PATH"

# Build tusk if you don't have the release binary yet
cd /home/louis/work/tusk
cargo build --release

# Run all three benchmarks
python3 benchmarks/harness.py benchmarks/fixtures/small  --output benchmarks/results/small.json  --runs 3
python3 benchmarks/harness.py benchmarks/fixtures/medium --output benchmarks/results/medium.json --runs 3
python3 benchmarks/harness.py benchmarks/fixtures/large  --output benchmarks/results/large.json  --runs 3
```

Each run takes 30–120 seconds (network-bound). The `--runs N` flag
controls how many times the warm-cache run is repeated for averaging.

## What it measures

For each fixture and each tool (composer, tusk):

| Metric | What it is |
|--------|------------|
| `cold_seconds` | Wall-clock time for the first install, with all caches cleared |
| `warm_seconds` | Wall-clock time for the N subsequent installs (cached) |
| `packages_installed` | Count of dirs in `vendor/{vendor}/{pkg}/` |
| `disk_bytes` | `du -sb vendor/` |
| `success` | Did the install complete without error? |

The harness clears `~/.cache/tusk/` and `~/.composer/cache/` before
each cold run. Warm runs do **not** clear — they re-install into a
fresh tempdir but share the global cache.

## Results

See **[REPORT.md](REPORT.md)** for the actual numbers and analysis.

Headline finding: **tusk is faster on every successful benchmark**,
with the biggest win on **warm cache** (content-addressed archive
storage):

| Project | Cold speedup | Warm speedup (avg) |
|---------|-------------:|-------------------:|
| small | **1.27×** | **6.6×** |
| large | **1.10×** | **2.74×** |

The benchmark run also surfaced two real Phase-1 bugs in tusk
(missing User-Agent header, dev-branch parser error) that the
synthetic unit tests did not catch. Both are fixed in the commit
that generated these results.

## Platform support

- ✅ **Linux** (tested on Ubuntu 24.04.4 LTS)
- ❌ macOS — not yet tested
- ❌ Windows — not yet supported (tusk itself is cross-platform; the
  benchmark harness is Python 3 with no platform-specific code, but
  PHP/Composer install paths differ)
