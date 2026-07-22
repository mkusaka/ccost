---
name: ccost
description: Use when Codex needs to inspect, summarize, compare, or troubleshoot local Claude Code, Codex, or OpenCode usage costs with the ccost CLI. Covers daily/monthly token and cost reports, JSON or table output, per-model breakdowns, project grouping, agent filtering, date ranges, timezone grouping, and offline pricing behavior.
---

# ccost

## Overview

Use `ccost` to report local Claude Code, Codex, and OpenCode token usage and costs. Prefer the installed `ccost` binary on `PATH`; when working inside the ccost repository and no binary is installed, use `cargo run -- <args>`.

Do not read or print raw session JSONL, local config, tokens, cookies, authorization headers, or other secret-bearing files unless the user explicitly asks for the raw source and the output has been reviewed for secrets.

## Quick Start

Run the smallest command that answers the user's question:

```bash
ccost daily
ccost monthly
ccost daily --json
ccost daily --kmb
ccost monthly --json
ccost monthly --kmb
```

If the user omits a period, start with `daily`. Use JSON when the user asks for data to process, precise totals, or a machine-readable answer; use table output for human-readable summaries.

## Common Tasks

Summarize recent usage:

```bash
ccost daily --since 20260101 --until 20260131
ccost monthly --since 20260101 --until 20261231
```

Filter by agent source:

```bash
ccost daily --agent codex
ccost daily --agent claudecode
ccost monthly --agent codex,opencode
```

Show model-level details:

```bash
ccost daily --breakdown
ccost monthly --breakdown --json
```

Group daily output by project or filter one project:

```bash
ccost daily --instances
ccost daily --project my-project
```

Control ordering and timezone grouping:

```bash
ccost daily --order desc
ccost daily --timezone UTC
ccost monthly --timezone America/New_York
```

## Data Locations

Let `ccost` discover default locations first. Override only when the user gives a non-default location or asks to diagnose missing data.

- Claude Code: `$XDG_CONFIG_HOME/claude`, `~/.config/claude`, then `~/.claude`; override with comma-separated `CLAUDE_CONFIG_DIR`.
- Codex: `${CODEX_HOME:-~/.codex}/sessions`.
- OpenCode: `${OPENCODE_DATA_DIR:-~/.local/share/opencode}/opencode.db`, with legacy fallback under `storage/message`.

## Pricing

By default, `ccost` uses bundled offline pricing snapshots. Use live pricing only when the user asks for it or when investigating pricing drift:

```bash
ccost daily --offline=false
ccost monthly --offline=false --json
```

Cost modes:

- `--mode auto`: use recorded `costUSD` when present, otherwise calculate from tokens.
- `--mode calculate`: always calculate from token counts.
- `--mode display`: always use recorded `costUSD`.

## Interpreting Results

When reporting back to the user, lead with the total cost, token count, date range, agent filter, and pricing mode used. Mention if no usage data was found and include the exact command that produced the result.

If `ccost` is unavailable:

1. Check whether `ccost` exists on `PATH`.
2. If inside the repository, use `cargo run -- <ccost-args>`.
3. Otherwise, tell the user to install it with Homebrew or Cargo rather than recreating ccost logic manually.
