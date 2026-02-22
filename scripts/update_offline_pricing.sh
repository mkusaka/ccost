#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLAUDE_OUTPUT_FILE="${CLAUDE_OUTPUT_FILE:-${ROOT_DIR}/assets/claude_pricing.json}"
CODEX_OUTPUT_FILE="${CODEX_OUTPUT_FILE:-${ROOT_DIR}/assets/codex_pricing.json}"
PRICING_URL="${PRICING_URL:-https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json}"

export CLAUDE_OUTPUT_FILE
export CODEX_OUTPUT_FILE
export PRICING_URL

python3 - <<'PY'
import json
import os
import sys
from urllib.request import urlopen

url = os.environ.get("PRICING_URL")
claude_output = os.environ.get("CLAUDE_OUTPUT_FILE")
codex_output = os.environ.get("CODEX_OUTPUT_FILE")
if not url or not claude_output or not codex_output:
    raise SystemExit("PRICING_URL, CLAUDE_OUTPUT_FILE, and CODEX_OUTPUT_FILE are required")

with urlopen(url) as resp:
    dataset = json.load(resp)

def write_filtered(output_path: str, prefixes: tuple[str, ...]) -> int:
    filtered = {k: v for k, v in dataset.items() if k.startswith(prefixes)}
    sorted_items = dict(sorted(filtered.items(), key=lambda item: item[0]))
    payload = json.dumps(sorted_items, indent=2, ensure_ascii=True)
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(payload)
        f.write("\n")
    return len(sorted_items)

claude_prefixes = ("claude-", "anthropic.claude-", "anthropic/claude-")
codex_prefixes = (
    "gpt-5",
    "openai/gpt-5",
    "azure/gpt-5",
    "openrouter/openai/gpt-5",
    "gemini-3-pro",
    "gemini/gemini-3-pro",
    "openrouter/google/gemini-3-pro",
    "vertex_ai/gemini-3-pro",
    "gmi/google/gemini-3-pro",
    "azure_ai/kimi-k2.5",
    "openrouter/moonshotai/kimi-k2.5",
    "moonshot/kimi-k2.5",
    "moonshotai.kimi-k2.5",
)

claude_count = write_filtered(claude_output, claude_prefixes)
codex_count = write_filtered(codex_output, codex_prefixes)

print(f"Wrote {claude_count} Claude models to {claude_output}")
print(f"Wrote {codex_count} Codex models to {codex_output}")
PY
