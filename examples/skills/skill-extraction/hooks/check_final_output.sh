#!/usr/bin/env bash
# Stop hook for skill-extraction meta-skill.
# Requires final output to contain draft_memory_id and evidence_anchor_id.
set -euo pipefail

input="$(cat)"

if echo "$input" | jq -e '(.output // {}) | has("draft_memory_id") and (.draft_memory_id != null) and (.draft_memory_id != "") and has("evidence_anchor_id") and (.evidence_anchor_id != null) and (.evidence_anchor_id != "")' >/dev/null 2>&1; then
  echo '{"decision":"allow"}'
  exit 0
fi

echo '{"decision":"continue","reason":"Output missing draft_memory_id or evidence_anchor_id; skill must write anchor and memory file, then return their IDs"}'
exit 2
