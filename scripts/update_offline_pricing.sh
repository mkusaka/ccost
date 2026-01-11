#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_FILE="${OUTPUT_FILE:-${ROOT_DIR}/assets/claude_pricing.json}"
PRICING_URL="${PRICING_URL:-https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json}"

export OUTPUT_FILE
export PRICING_URL

python3 - <<'PY'
import json
import os
import sys
from urllib.request import urlopen

url = os.environ.get("PRICING_URL")
output = os.environ.get("OUTPUT_FILE")
if not url or not output:
    raise SystemExit("PRICING_URL and OUTPUT_FILE are required")

with urlopen(url) as resp:
    dataset = json.load(resp)

prefixes = ("claude-", "anthropic.claude-", "anthropic/claude-")
filtered = {k: v for k, v in dataset.items() if k.startswith(prefixes)}

sorted_items = dict(sorted(filtered.items(), key=lambda item: item[0]))
payload = json.dumps(sorted_items, indent=2, ensure_ascii=True)

os.makedirs(os.path.dirname(output), exist_ok=True)
with open(output, "w", encoding="utf-8") as f:
    f.write(payload)
    f.write("\n")

print(f"Wrote {len(sorted_items)} models to {output}")
PY
