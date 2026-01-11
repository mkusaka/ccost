#!/usr/bin/env bash
set -euo pipefail

# ---- Config ----
CCUSAGE_ROOT="${CCUSAGE_ROOT:-$HOME/src/github.com/ryoppippi/ccusage}"
CCUSAGE_BIN="${CCUSAGE_BIN:-}"
CCUSAGE_BIN_ARGS=()
CCOST_BIN="${CCOST_BIN:-$PWD/target/release/ccost}"

PROJECT="${PROJECT:--Users-masatomo-kusaka}" # empty to skip --project
TIMEZONE="${TIMEZONE:-UTC}"
SINCE="${SINCE:-20251201}"
UNTIL="${UNTIL:-20251231}"

RUNS="${RUNS:-10}"
WARMUP="${WARMUP:-2}"

# ---- Helpers ----
join_cmd() {
  local out=""
  for arg in "$@"; do
    out+="$(printf '%q ' "$arg")"
  done
  printf '%s' "$out"
}

run_case() {
  local label="$1"; shift
  local -a args=("$@")
  local -a common=(--offline --timezone "$TIMEZONE")
  if [ -n "$PROJECT" ]; then
    common+=(--project="$PROJECT")
  fi

  local ccusage_cmd
  local ccost_cmd
  ccusage_cmd="$(join_cmd "$CCUSAGE_BIN" "${CCUSAGE_BIN_ARGS[@]}" "${args[@]}" "${common[@]}")"
  ccost_cmd="$(join_cmd "$CCOST_BIN" "${args[@]}" "${common[@]}")"

  echo ""
  echo "== ${label} =="
  hyperfine --warmup "$WARMUP" --runs "$RUNS" --style basic \
    "${ccusage_cmd} > /dev/null" \
    "${ccost_cmd} > /dev/null"
}

# ---- Checks ----
if [ -z "$CCUSAGE_BIN" ]; then
  CCUSAGE_DIST="${CCUSAGE_ROOT}/apps/ccusage/dist/index.js"
  if [ -f "$CCUSAGE_DIST" ]; then
    CCUSAGE_BIN="bun"
    CCUSAGE_BIN_ARGS=("$CCUSAGE_DIST")
  else
    echo "ccusage dist not found: $CCUSAGE_DIST"
    echo "run: (cd \"$CCUSAGE_ROOT\" && pnpm --filter ccusage build)"
    exit 1
  fi
fi
if [ ! -x "$CCOST_BIN" ]; then
  echo "ccost binary not found: $CCOST_BIN"
  exit 1
fi

# ---- filtered (since/until) ----
run_case "daily json (filtered)"    daily   --json  --since "$SINCE" --until "$UNTIL"
run_case "daily table (filtered)"   daily           --since "$SINCE" --until "$UNTIL"
run_case "monthly json (filtered)"  monthly --json  --since "$SINCE" --until "$UNTIL"
run_case "monthly table (filtered)" monthly         --since "$SINCE" --until "$UNTIL"

# ---- full (no since/until) ----
run_case "daily json (full)"        daily   --json
run_case "daily table (full)"       daily
run_case "monthly json (full)"      monthly --json
run_case "monthly table (full)"     monthly
