#!/usr/bin/env bash
# Contract-harness PreToolUse probe.
# Dumps stdin envelope JSON + relevant GRID_* env vars to ${GRID_CONTRACT_PROBE_OUT}/pre_tool_use.{envelope.json,env.json}
# and always returns allow. Fail-open: if GRID_CONTRACT_PROBE_OUT is unset, still exit 0 allow.
set -u

out_dir="${GRID_CONTRACT_PROBE_OUT:-/tmp}"
mkdir -p "$out_dir" 2>/dev/null || true

# stdin envelope — write raw bytes
cat >"$out_dir/pre_tool_use.envelope.json" || true

# env dump — only GRID_* vars for deterministic asserts
{
  echo "{"
  first=1
  for var in GRID_SESSION_ID GRID_TOOL_NAME GRID_SKILL_ID GRID_EVENT GRID_AGENT_ID GRID_TURN GRID_WORKING_DIR GRID_SANDBOX_MODE GRID_SANDBOX_PROFILE GRID_MODEL GRID_AUTONOMY_LEVEL GRID_TOTAL_TOOL_CALLS GRID_CURRENT_ROUND; do
    if [ -n "${!var+x}" ]; then
      if [ $first -eq 0 ]; then echo ","; fi
      # JSON-escape: replace backslash + double-quote only (values are shell strings, not code).
      val="${!var//\\/\\\\}"
      val="${val//\"/\\\"}"
      printf '  "%s": "%s"' "$var" "$val"
      first=0
    fi
  done
  echo ""
  echo "}"
} >"$out_dir/pre_tool_use.env.json" || true

echo '{"decision":"allow"}'
exit 0
