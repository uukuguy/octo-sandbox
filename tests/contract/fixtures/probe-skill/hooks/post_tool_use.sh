#!/usr/bin/env bash
# Contract-harness PostToolUse probe — dumps stdin envelope + GRID_* env.
set -u

out_dir="${GRID_CONTRACT_PROBE_OUT:-/tmp}"
mkdir -p "$out_dir" 2>/dev/null || true

cat >"$out_dir/post_tool_use.envelope.json" || true

{
  echo "{"
  first=1
  for var in GRID_SESSION_ID GRID_TOOL_NAME GRID_SKILL_ID GRID_EVENT GRID_AGENT_ID GRID_TURN GRID_WORKING_DIR GRID_SANDBOX_MODE GRID_SANDBOX_PROFILE GRID_MODEL GRID_AUTONOMY_LEVEL GRID_TOTAL_TOOL_CALLS GRID_CURRENT_ROUND; do
    if [ -n "${!var+x}" ]; then
      if [ $first -eq 0 ]; then echo ","; fi
      val="${!var//\\/\\\\}"
      val="${val//\"/\\\"}"
      printf '  "%s": "%s"' "$var" "$val"
      first=0
    fi
  done
  echo ""
  echo "}"
} >"$out_dir/post_tool_use.env.json" || true

echo '{"decision":"allow"}'
exit 0
