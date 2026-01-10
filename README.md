# ccost

Fast Rust reimplementation of the daily/monthly reporting parts of
[ccusage](https://github.com/ryoppippi/ccusage) for Claude Code usage data.

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
- Offline pricing data bundled at build time

## Install

Build from source:

```bash
cargo build --release
```

Run the binary:

```bash
./target/release/ccost daily
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
- `--offline`: use bundled pricing data
- `--order`: `asc` | `desc`
- `--since` / `--until`: date filters in `YYYYMMDD`
- `--timezone`: grouping timezone (e.g., `UTC`, `America/New_York`)

## Data discovery

ccost looks for Claude Code usage data under a `projects` directory.

Default locations (checked in order):
- `$XDG_CONFIG_HOME/claude` or `~/.config/claude`
- `~/.claude`

You can override with `CLAUDE_CONFIG_DIR` (comma-separated):

```bash
export CLAUDE_CONFIG_DIR="$HOME/.claude,$HOME/.config/claude"
```

## Pricing

Cost calculation modes:
- `auto`: use `costUSD` when present, otherwise calculate from tokens
- `calculate`: always calculate from tokens
- `display`: always use `costUSD`

When `--offline` is set, ccost uses an embedded pricing snapshot derived from
LiteLLM’s model pricing dataset (Claude-only subset).

## Compatibility

The output for `daily` and `monthly` (JSON + table) is intended to match
ccusage for those commands.

## Development

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt
```
