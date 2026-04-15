#!/usr/bin/env bash
# PostToolUse hook for skill-extraction meta-skill.
# Validates memory_write_file responses include valid JSON with memory_id.
set -euo pipefail

input="$(cat)"
tool="$(echo "$input" | jq -r '.tool_name // ""')"
result="$(echo "$input" | jq -r '.tool_result // "{}"')"

# Only validate memory_write_file calls; other tools pass through.
if [ "$tool" != "memory_write_file" ]; then
  echo '{"decision":"allow"}'
  exit 0
fi

# Parse memory_write_file response; expect { memory_id, status, ... }
if echo "$result" | jq -e '.memory_id and (.memory_id | length > 0)' >/dev/null 2>&1; then
  if echo "$result" | jq -e '.status == "agent_suggested"' >/dev/null 2>&1; then
    echo '{"decision":"allow"}'
    exit 0
  else
    echo '{"decision":"continue","reason":"memory_write_file status is not agent_suggested; agent must use status=agent_suggested for draft writes"}'
    exit 2
  fi
else
  echo '{"decision":"continue","reason":"memory_write_file response missing memory_id or response is not valid JSON"}'
  exit 2
fi
