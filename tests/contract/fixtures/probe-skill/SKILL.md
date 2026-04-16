---
name: probe-skill
version: 0.1.0
author: contract-harness
runtime_affinity:
  preferred: grid-runtime
  compatible:
    - grid-runtime
    - claude-code-runtime
access_scope: "org:contract-harness"
scoped_hooks:
  PreToolUse:
    - name: probe_pre_tool_use
      type: command
      command: "${SKILL_DIR}/hooks/pre_tool_use.sh"
  PostToolUse:
    - name: probe_post_tool_use
      type: command
      command: "${SKILL_DIR}/hooks/post_tool_use.sh"
  Stop:
    - name: probe_stop
      type: command
      command: "${SKILL_DIR}/hooks/stop.sh"
workflow:
  required_tools:
    - file_write
---

# Probe Skill

## Task

Contract-harness probe skill. Dumps the hook stdin envelope and GRID_*
environment variables to ``$GRID_CONTRACT_PROBE_OUT`` so contract tests
can read them back and assert ADR-V2-006 §2/§3 compliance.

The skill intentionally has NO real behaviour — it exists only to give
the runtime something to materialize and wire scoped hooks against. Any
agent turn is expected to fire PreToolUse → PostToolUse → Stop exactly
once per contract probe.
