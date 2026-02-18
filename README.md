# ccost

Fast Rust reimplementation of the daily/monthly reporting parts of
[ccusage](https://github.com/ryoppippi/ccusage) for Claude Code + Codex usage data.
Most of the daily/monthly implementation is a direct port of ccusage into Rust.

This tool focuses on:
- Daily and monthly token/cost aggregation
- Table and JSON output
- Fast JSONL parsing and file traversal

Everything else from ccusage is out of scope for now.

## Features

- Daily and monthly reports
- JSON and table output
- Per-model breakdowns
- Project/instance grouping for daily
- Claude Code and Codex source aggregation
- Offline pricing data bundled at build time (Claude + Codex)

## Install

Build from source:

```bash
cargo build --release
```

Run the binary:

```bash
./target/release/ccost daily
```

Install from a local clone:

```bash
git clone https://github.com/mkusaka/ccost.git
cd ccost
cargo install --path . --force
```

Install via Cargo:

```bash
cargo install --git https://github.com/mkusaka/ccost
```

## Usage

Daily:

```bash
ccost daily
ccost daily --json
ccost daily --breakdown
ccost daily --instances
ccost daily --project my-project
ccost daily --since 20250101 --until 20250131
ccost daily --timezone UTC
```

Monthly:

```bash
ccost monthly
ccost monthly --json
ccost monthly --breakdown
ccost monthly --since 20250101 --until 20250131
ccost monthly --timezone UTC
```

Common flags:

- `--json`: JSON output
- `--breakdown`: per-model breakdown
- `--mode`: `auto` | `calculate` | `display`
- `--offline`: use bundled pricing data (default; set `--offline=false` to fetch live pricing)
- `--codex`: include Codex usage data (default `true`, set `--codex=false` to disable)
- `--claudecode`: include Claude Code usage data (default `true`, set `--claudecode=false` to disable)
- `--order`: `asc` | `desc`
- `--since` / `--until`: date filters in `YYYYMMDD`
- `--timezone`: grouping timezone (e.g., `UTC`, `America/New_York`)

## Data discovery

ccost looks for usage data from both Claude Code and Codex.

Claude Code default locations (checked in order):
- `$XDG_CONFIG_HOME/claude` or `~/.config/claude`
- `~/.claude`

You can override with `CLAUDE_CONFIG_DIR` (comma-separated):

```bash
export CLAUDE_CONFIG_DIR="$HOME/.claude,$HOME/.config/claude"
```

Codex default location:
- `${CODEX_HOME:-~/.codex}/sessions`

## Pricing

Cost calculation modes:
- `auto`: use `costUSD` when present, otherwise calculate from tokens
- `calculate`: always calculate from tokens
- `display`: always use `costUSD`

When `--offline` is set, ccost uses embedded pricing snapshots derived from
LiteLLM’s model pricing dataset (Claude and Codex subsets).

To update the embedded snapshot:

```bash
scripts/update_offline_pricing.sh
```

A scheduled GitHub Action periodically refreshes the snapshot and opens a PR
requesting review from `mkusaka`.

## Compatibility

The output for `daily` and `monthly` (JSON + table) is intended to match
ccusage for those commands.

## Development

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt
```

## Acknowledgements

This project is heavily based on ccusage, and its daily/monthly logic is
ported to Rust. Huge thanks to ryoppippi and the ccusage contributors for
the original implementation and ongoing work.
