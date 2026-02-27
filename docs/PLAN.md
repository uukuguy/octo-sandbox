# Plan: Memory Recall/Forget Tools + REST API + Explorer Page

## Overview

Add `memory_recall` and `memory_forget` tools, expand the Memories REST API with CRUD endpoints, and build a Memory Explorer frontend page.

---

## Task 1: `memory_recall` Tool

**File:** `crates/octo-engine/src/tools/memory_recall.rs` (new)

A tool that retrieves a single memory by ID, with an option to include related memories. Differs from `memory_search` (which does query-based search) — this is a direct recall by ID.

- Parameters: `id` (required string), `include_related` (optional bool, default false)
- If `include_related` is true, use the recalled memory's content as a search query to find related entries (limit 5)
- Updates `accessed_at` and `access_count` on the recalled entry
- Returns the memory content, metadata, category, timestamps, and optionally related memories

**Register in:** `crates/octo-engine/src/tools/mod.rs` — add to `register_memory_tools()`. Only needs `Arc<dyn MemoryStore>` + `Arc<dyn Provider>` (for related search embeddings).

---

## Task 2: `memory_forget` Tool

**File:** `crates/octo-engine/src/tools/memory_forget.rs` (new)

A tool that deletes one or more memories by ID or by filter criteria.

- Parameters:
  - `id` (optional string) — delete a single memory
  - `category` (optional string) — delete all memories in a category
  - `before` (optional string, ISO 8601 date) — delete memories older than this date
  - At least one parameter must be provided
- For category/before bulk deletes, add a `count_by_filter` method to `MemoryStore` trait (or reuse `list` + `delete` in a loop)
- Returns count of deleted memories

**Approach for bulk delete:** Add `delete_by_filter(filter: MemoryFilter) -> Result<usize>` to `MemoryStore` trait + implement in `SqliteMemoryStore`. This avoids loading all entries into memory just to delete them.

**Register in:** `crates/octo-engine/src/tools/mod.rs`

---

## Task 3: Expand Memories REST API

**File:** `crates/octo-server/src/api/memories.rs` (edit existing)

Add these endpoints:

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/memories` | `list_memories` | List memories with filter params (existing `search_memories` becomes search-only when `q` is present, otherwise list) |
| GET | `/memories/{id}` | `get_memory` | Get single memory by ID |
| DELETE | `/memories/{id}` | `delete_memory` | Delete single memory |
| PUT | `/memories/{id}` | `update_memory` | Update memory content |

**File:** `crates/octo-server/src/api/mod.rs` (edit routes)

Add the new routes with proper method routing (get, delete, put).

---

## Task 4: Memory Explorer Frontend Page

### 4a. Add "Memory" tab

**Files to edit:**
- `web/src/atoms/ui.ts` — extend `TabId` union: `"chat" | "tools" | "debug" | "memory"`
- `web/src/components/layout/TabBar.tsx` — add `{ id: "memory", label: "Memory" }` to tabs array
- `web/src/App.tsx` — add `{activeTab === "memory" && <Memory />}` + import

### 4b. Memory Explorer page

**File:** `web/src/pages/Memory.tsx` (new)

Layout:
- Top bar: search input + category filter dropdown + refresh button
- Main area: memory cards list (scrollable)
- Each card shows: category badge, content preview (truncated), importance bar, timestamps, source type
- Click card → expand to show full content + metadata JSON
- Delete button on each card (with confirm)
- Edit button → inline edit content

API calls:
- `GET /api/memories?limit=50` — initial load (list all)
- `GET /api/memories?q=xxx` — search
- `DELETE /api/memories/{id}` — delete
- `PUT /api/memories/{id}` — update

Uses existing patterns from `ExecutionList.tsx` (fetch + state + render list).

---

## File Change Summary

| File | Action |
|------|--------|
| `crates/octo-engine/src/tools/memory_recall.rs` | Create |
| `crates/octo-engine/src/tools/memory_forget.rs` | Create |
| `crates/octo-engine/src/tools/mod.rs` | Edit (register new tools) |
| `crates/octo-engine/src/memory/store_traits.rs` | Edit (add `delete_by_filter`) |
| `crates/octo-engine/src/memory/sqlite_store.rs` | Edit (implement `delete_by_filter`) |
| `crates/octo-server/src/api/memories.rs` | Edit (add CRUD handlers) |
| `crates/octo-server/src/api/mod.rs` | Edit (add routes) |
| `web/src/atoms/ui.ts` | Edit (add "memory" tab) |
| `web/src/components/layout/TabBar.tsx` | Edit (add Memory tab) |
| `web/src/App.tsx` | Edit (render Memory page) |
| `web/src/pages/Memory.tsx` | Create |

## Execution Order

1. `store_traits.rs` + `sqlite_store.rs` (add `delete_by_filter`)
2. `memory_recall.rs` + `memory_forget.rs` (new tools)
3. `tools/mod.rs` (register)
4. `api/memories.rs` + `api/mod.rs` (REST endpoints)
5. Frontend files (tab + page)
6. `cargo check` to verify backend compiles
