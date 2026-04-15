---
name: skill-extraction
version: 0.1.0
author: eaasp-mvp
runtime_affinity:
  preferred: grid-runtime
  compatible:
    - grid-runtime
    - claude-code-runtime
access_scope: "org:eaasp-mvp"
scoped_hooks:
  PreToolUse: []
  PostToolUse:
    - name: verify_skill_draft
      type: command
      command: "${SKILL_DIR}/hooks/verify_skill_draft.sh"
  Stop:
    - name: check_final_output
      type: command
      command: "${SKILL_DIR}/hooks/check_final_output.sh"
dependencies:
  - mcp:eaasp-l2-memory
  - mcp:eaasp-skill-registry
workflow:
  required_tools:
    - memory_search
    - memory_read
    - memory_write_anchor
    - memory_write_file
---

# Skill Extraction

## Task

You are a meta-skill that distills a session's event cluster into a reusable
SKILL.md draft. Given a `cluster_id` or `session_id` pointing at a pre-packed
event trace in L2 memory, you analyze the sequence of tool calls, decision
points, and hook fires to infer the repeating pattern, then emit a draft
frontmatter + prose SKILL.md plus an evidence anchor tying the draft back to
the source events.

**IMPORTANT**: This is an autonomous workflow. You MUST complete all steps
(search → read → analyze → write anchor → write memory → emit JSON) without
stopping to ask the user for confirmation. The output is a draft, not a
production skill — human reviewers will inspect and submit it separately.
Execute the full workflow end-to-end and return the final JSON output directly.

## Workflow

1. Call `memory_search(query="<cluster_ref or session_id>", category="event_cluster",
   top_k=1)` to locate the input events. The hint is passed in the system
   context; take the top hit's `memory_id` as the cluster id.

2. Call `memory_read(memory_id=<cluster_id>)` to fetch the serialized event
   list. Parse the JSON into events — each has top-level `event_type`,
   `payload` (dict), `session_id`, `event_id`, `created_at` (unix epoch),
   plus optional `cluster_id` and `seq`. The `tool_name` (when present)
   lives inside `payload`, e.g. `payload["tool_name"]` for `PRE_TOOL_USE`
   and `POST_TOOL_USE` events. Full shape: PHASE1_EVENT_ENGINE_DESIGN.md §2.2.

3. Infer the skill pattern by traversing the events:
   - Which tools were called and in what order → `workflow.required_tools`.
   - What argument shapes each tool received → Tool Contract rows.
   - What decision logic the agent applied → Safety Envelope bullets.
   - What hooks fired (PreToolUse/PostToolUse/Stop) and what they enforced.
   - What output schema the final assistant turn produced → Output Contract.

4. Construct the SKILL.md draft body: assemble frontmatter (name, version
   `0.1.0`, author `eaasp-mvp`, runtime_affinity, access_scope inherited
   from the cluster, dependencies, scoped_hooks, workflow.required_tools)
   plus prose sections Task / Workflow / Tool Contract / Output Contract /
   Cross-session Behavior / Safety Envelope.

5. Call `memory_write_anchor(event_id=<generated>, session_id=<from cluster>,
   type="skill_extraction_source", data_ref=<cluster_id>, metadata={...})`
   to pin the event cluster as the source of this draft. Keep the returned
   `anchor_id`.

6. Call `memory_write_file(scope=<inherited>, category="skill_draft",
   content=<JSON-wrapped object>, evidence_refs=[<anchor_id>],
   status="agent_suggested")` to persist the draft. Content shape:
   ```json
   {
     "frontmatter_yaml": "---\nname: ...\n---\n",
     "prose": "# Title\n\n## Task\n...",
     "suggested_skill_id": "skill-extraction-variant-A",
     "suggested_name": "Skill Extraction (Option A)",
     "notes": "Inferred from 12 events over 45s in session abc123"
   }
   ```
   Keep the returned `memory_id`.

7. Emit the final JSON output. The `check_final_output` Stop hook validates
   that `draft_memory_id` and `evidence_anchor_id` are both present and
   non-empty; anything missing causes a re-prompt.

## Tool Contract

Required MCP tools (resolved at session initialize via `connectMCP`):

| Tool | Server | Purpose | Args |
|---|---|---|---|
| `memory_search` | `eaasp-l2-memory` | Find event cluster by query | `query` (str), `category` (optional, `"event_cluster"`), `top_k` (int, default 10) |
| `memory_read` | `eaasp-l2-memory` | Load full memory file | `memory_id` (str, required) |
| `memory_write_anchor` | `eaasp-l2-memory` | Write evidence anchor | `event_id` (str), `session_id` (str), `type` (str, `"skill_extraction_source"`), `data_ref` (str). Optional but recommended: `snapshot_hash` (sha256 of cluster JSON for integrity), `source_system` (`"skill-extraction"`), `tool_version` / `model_version` / `rule_version` (provenance), `metadata` (free-form object). |
| `memory_write_file` | `eaasp-l2-memory` | Write SKILL.md draft | `memory_id` (str, optional — supply on second invocation to bump the version of an existing draft), `scope` (str), `category` (str, `"skill_draft"`), `content` (str, JSON), `evidence_refs` (array of str), `status` (`"agent_suggested"` on first write, `"confirmed"` when approving a prior draft). |

Reference: `tools/eaasp-l2-memory-engine/src/eaasp_l2_memory_engine/mcp_tools.py`
lines 28-130 (MCP_TOOL_MANIFEST).

## Output Contract

Final assistant output MUST be a JSON object with at minimum:

```json
{
  "draft_memory_id": "mem_skill_draft_<session_id>_v<N>",
  "source_cluster_id": "<cluster_id>",
  "suggested_skill_id": "<inferred_skill_name>",
  "evidence_anchor_id": "<anchor_id_from_write_anchor>",
  "event_count": 12,
  "analysis_summary": "<one-sentence description of inferred skill>",
  "confidence_score": 0.87
}
```

The `check_final_output` Stop hook rejects outputs where `draft_memory_id` or
`evidence_anchor_id` is absent, null, or empty; the runtime then re-prompts.

## Cross-session Behavior

On a second invocation for the same candidate (user refines the cluster and
re-runs), L4 session create surfaces the previous `agent_suggested` draft via
`memory_search`. The skill may then either **confirm** the prior draft (call
`memory_write_file` with the existing `memory_id` + `status="confirmed"`) or
**revise** it (archive the old draft with `memory_archive`, write a fresh
`agent_suggested` version with a new anchor).

Draft lifecycle: `agent_suggested` → `confirmed` (human approved) →
`archived` (superseded by newer draft).

## Safety Envelope

- **Read-only to events**: calls `memory_search` / `memory_read` only; no
  writes to event tables.
- **Append-only evidence**: `memory_write_anchor` is immutable, never updated
  in place.
- **Versioned drafts**: each `memory_write_file` increments the version;
  prior versions remain queryable.
- **No autonomous submission**: the skill never calls `skill_submit_draft`.
  Submission to the skill-registry is a separate human approval step — this
  skill stops at draft emission per MVP_SCOPE N14. `required_tools` therefore
  lists only the four tools the agent actually invokes.
- **Confidence reporting**: if the event cluster is noisy or contradictory,
  `confidence_score` must reflect that (< 0.7 flags human escalation).
- **No hallucination**: the skill writes down only what is observable in the
  event cluster; ambiguous patterns become notes, not assertions.
