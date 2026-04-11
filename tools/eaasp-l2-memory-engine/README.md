# eaasp-l2-memory-engine

EAASP v2.0 L2 Memory Engine — Cross-session evidence anchors and versioned memory files.

**Spec:** `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` §3.3
**Plan:** `docs/plans/2026-04-11-v2-mvp-phase0-plan.md` S3.T2
**Status:** Phase 0 MVP — implemented in S3.T2

## Responsibilities (Ring-2 MVP scope)

Three-layer SQLite storage:

1. **Layer 1 — Evidence Anchor Store** (`anchors` table, append-only immutable)
   - anchor_id / event_id / session_id / type / data_ref / snapshot_hash
   - source_system / tool_version / model_version / rule_version / created_at / metadata

2. **Layer 2 — File-based Memory** (`memory_files` table, versioned)
   - memory_id / scope / category / content / evidence_refs
   - status state machine: `agent_suggested` → `confirmed` → `archived`

3. **Layer 3 — Hybrid Retrieval Index** (`memory_fts` FTS5 virtual table)
   - Keyword search + time-decay weighting (semantic deferred to Phase 2)

## 6 MCP Tools

| Tool | Purpose |
|------|---------|
| `memory_search` | Hybrid keyword + time-decay ranked search |
| `memory_read` | Read a specific memory_id |
| `memory_write_anchor` | Append-only evidence anchor write |
| `memory_write_file` | Create or bump version of a memory file |
| `memory_list` | List memory files by scope/category |
| `memory_archive` | Transition status → archived |

## REST API

- `GET /tools` — MCP tool manifest (REST facade)
- `POST /tools/{name}/invoke` — Invoke an MCP tool
- `POST /api/v1/memory/search` — L4 context assembly entry point
- `GET /api/v1/memory/anchors?event_id=X` — Evidence chain traceback

## Out of scope (deferred)

- HNSW index / semantic retrieval (Phase 2)
- Memory compression / eviction policy
- Cross-user memory sharing / RBAC (Phase 3)

## Stack

Python 3.12+, FastAPI, aiosqlite, pydantic v2.

## Dev quickstart

```bash
cd tools/eaasp-l2-memory-engine
uv sync --extra dev
pytest
uvicorn eaasp_l2_memory_engine.main:app --port 18085
```
