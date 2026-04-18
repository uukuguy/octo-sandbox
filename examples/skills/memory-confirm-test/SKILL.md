---
name: memory-confirm-test
version: 0.1.0
author: eaasp-phase3
runtime_affinity:
  preferred: grid-runtime
  compatible:
    - grid-runtime
    - claude-code-runtime
    - nanobot-runtime
    - pydantic-ai-runtime
access_scope: "org:eaasp-phase3"
scoped_hooks:
  PreToolUse:
    - name: block_unconfirmed_write
      type: command
      command: "${SKILL_DIR}/hooks/block_unconfirmed_write.sh"
  PostToolUse:
    - name: verify_confirm_ack
      type: command
      command: "${SKILL_DIR}/hooks/verify_confirm_ack.sh"
  Stop:
    - name: check_confirm_audit
      type: command
      command: "${SKILL_DIR}/hooks/check_confirm_audit.sh"
dependencies:
  - mcp:eaasp-l2-memory
workflow:
  required_tools:
    - l2:memory.search
    - l2:memory.read
    - l2:memory.confirm
    - l2:memory.write_anchor
---

# Memory Confirm Test Skill

This skill validates the `memory.confirm` state-machine transition
(agent_suggested → confirmed) as an end-to-end behavioral assertion.

## Purpose

Used exclusively in Phase 3 E2E test suite (B5+B6) to verify:

1. The agent calls `memory.search` to locate a candidate memory entry
2. The agent calls `memory.read` to retrieve its full content
3. The agent calls `memory.confirm` to transition status to `confirmed`
4. The PreToolUse hook blocks any direct `memory.write_*` calls before
   confirmation (enforces the review-before-commit pattern)
5. The PostToolUse hook verifies `memory.confirm` returned a valid ack
6. The Stop hook asserts at least one confirmation occurred in the session

## Workflow

```
memory.search("candidate")
  → memory.read(memory_id)
  → memory.confirm(memory_id, verdict="approved", evidence_refs=[...])
  → memory.write_anchor(memory_id)
```

## Hook Contracts

### block_unconfirmed_write.sh
- **Scope**: PreToolUse
- **Fires on**: any `memory.write_*` call
- **Denies**: calls where the target memory_id has status != "confirmed"
- **Allows**: all other tool calls

### verify_confirm_ack.sh
- **Scope**: PostToolUse
- **Fires on**: `memory.confirm` result
- **Validates**: result contains `status == "confirmed"` and non-empty `memory_id`
- **Denies**: if result is malformed or status != "confirmed"

### check_confirm_audit.sh
- **Scope**: Stop
- **Validates**: at least one `memory.confirm` call was made in this session
- **Injects**: reminder prompt if no confirmation occurred (re-entry)
