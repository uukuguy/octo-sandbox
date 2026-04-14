"""S2.T4 — State-machine invariants for the versioned memory file store.

Nine behaviours + one parametrized guard (10 test definitions, 11 collected)
that lock down the ``agent_suggested → confirmed → archived`` state machine
from three angles:

1. ``file_store.write(..., status=X)`` can forward-transition directly without
   needing ``.confirm()`` / ``.archive()`` helpers — the M4 guard only fires
   on illegal edges, not on legal ones.
2. Version, ``created_at`` and ``evidence_refs`` are preserved as invariants
   across the full walk of the state machine.
3. The REST and MCP dispatcher surfaces agree with the low-level store on
   error codes (``409 invalid_transition`` / ``404 not_found``) and on the
   set of columns visible to callers (including S2.T3 embedding metadata).

The tests deliberately avoid HNSW / Ollama — fixtures give us an in-memory
SQLite DB and the default mock embedding provider, both deterministic.
"""

from __future__ import annotations

import aiosqlite
import pytest
from httpx import AsyncClient

from eaasp_l2_memory_engine.files import (
    InvalidStatusTransition,
    MemoryFileIn,
    MemoryFileStore,
)
from eaasp_l2_memory_engine.mcp_tools import McpToolDispatcher

pytestmark = pytest.mark.asyncio


# --- Test 1: forward write() suggested → confirmed --------------------------


async def test_write_forward_transition_suggested_to_confirmed_via_write_direct(
    file_store: MemoryFileStore,
) -> None:
    """``write(status="confirmed")`` on an agent_suggested memory must bump
    version and flip status without needing the ``.confirm()`` helper."""
    v1 = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="v1")
    )
    assert v1.version == 1
    assert v1.status == "agent_suggested"

    v2 = await file_store.write(
        MemoryFileIn(
            memory_id=v1.memory_id,
            scope="s",
            category="c",
            content="v1",
            status="confirmed",
        )
    )
    assert v2.memory_id == v1.memory_id
    assert v2.version == 2
    assert v2.status == "confirmed"


# --- Test 2: forward write() confirmed → archived ---------------------------


async def test_write_forward_transition_confirmed_to_archived_via_write_direct(
    file_store: MemoryFileStore,
) -> None:
    """Chain write() → confirm() → write(status="archived"): the direct
    forward edge from confirmed to archived must succeed (not only via
    ``.archive()``)."""
    v1 = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="v1")
    )
    v2 = await file_store.confirm(v1.memory_id)
    assert v2.version == 2
    assert v2.status == "confirmed"

    v3 = await file_store.write(
        MemoryFileIn(
            memory_id=v1.memory_id,
            scope="s",
            category="c",
            content="v1",
            status="archived",
        )
    )
    assert v3.version == 3
    assert v3.status == "archived"


# --- Test 3: version monotonicity across the whole walk ---------------------


async def test_version_monotonicity_all_versions_persist(
    file_store: MemoryFileStore,
) -> None:
    """After write → confirm → archive all three rows must persist with
    strictly increasing versions and the expected status progression.
    Reads the raw ``memory_files`` table so we can see history, not only
    ``read_latest``."""
    v1 = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="walk")
    )
    await file_store.confirm(v1.memory_id)
    await file_store.archive(v1.memory_id)

    async with aiosqlite.connect(file_store.db_path) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT version, status FROM memory_files "
            "WHERE memory_id = ? ORDER BY version ASC",
            (v1.memory_id,),
        )
        rows = await cur.fetchall()

    assert [r["version"] for r in rows] == [1, 2, 3]
    assert [r["status"] for r in rows] == [
        "agent_suggested",
        "confirmed",
        "archived",
    ]
    # Strictly increasing (no duplicates, no gaps). ``versions`` and
    # ``versions[1:]`` differ in length by 1, so ``strict=True`` would raise
    # ValueError. The zip trims to the shorter list — the intended behavior.
    versions = [r["version"] for r in rows]
    for prev, curr in zip(versions, versions[1:]):
        assert curr > prev


# --- Test 4: timestamps — created_at stable, updated_at non-decreasing ------


async def test_timestamps_invariant_across_versions(
    file_store: MemoryFileStore,
) -> None:
    """``created_at`` is an invariant of the memory_id (sourced from
    ``MIN(created_at)`` in ``files.py::write``) and must be equal across
    every version. ``updated_at`` reflects the per-row write clock and must
    be monotonically non-decreasing (``>=``, not ``>``, because a fast clock
    can produce same-millisecond timestamps)."""
    v1 = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="ts-walk")
    )
    await file_store.confirm(v1.memory_id)
    await file_store.archive(v1.memory_id)

    async with aiosqlite.connect(file_store.db_path) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT version, created_at, updated_at FROM memory_files "
            "WHERE memory_id = ? ORDER BY version ASC",
            (v1.memory_id,),
        )
        rows = await cur.fetchall()

    assert len(rows) == 3
    created = [r["created_at"] for r in rows]
    updated = [r["updated_at"] for r in rows]

    # created_at is stable across all versions of the same memory_id.
    assert created[0] == created[1] == created[2]

    # updated_at is non-decreasing (tolerant of same-ms fast-clock writes).
    assert updated[1] >= updated[0]
    assert updated[2] >= updated[1]


# --- Test 5: evidence_refs preserved across confirm + archive ---------------


async def test_evidence_refs_preserved_across_transitions(
    file_store: MemoryFileStore,
) -> None:
    """``evidence_refs`` is a JSON-encoded ``list[str]`` column; confirm()
    and archive() both copy it forward from the previous version. List
    equality (ordered) is the strongest assertion we can make — neither
    helper should reorder, dedupe, or drop entries."""
    refs = ["anc_1", "anc_2"]
    v1 = await file_store.write(
        MemoryFileIn(
            scope="s",
            category="c",
            content="e-refs",
            evidence_refs=refs,
        )
    )
    assert v1.evidence_refs == refs

    v2 = await file_store.confirm(v1.memory_id)
    assert v2.evidence_refs == refs

    v3 = await file_store.archive(v1.memory_id)
    assert v3.evidence_refs == refs

    # Cross-check every version from the DB directly — catches a bug where
    # a future write() variant might skip preserving evidence_refs for the
    # archived row specifically.
    async with aiosqlite.connect(file_store.db_path) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT version, evidence_refs FROM memory_files "
            "WHERE memory_id = ? ORDER BY version ASC",
            (v1.memory_id,),
        )
        rows = await cur.fetchall()

    import json

    decoded = [json.loads(r["evidence_refs"]) for r in rows]
    assert decoded == [refs, refs, refs]


# --- Test 6: archived blocks forward-transition writes ---------------------
#
# NOTE on scope: ``write()`` gates transitions only when the incoming status
# differs from ``latest_status`` (``files.py:133 — if memory.status !=
# latest_status``). Same-status rewrites are intentionally allowed — see
# ``test_review_fixes.test_write_same_status_is_allowed``. So the self-loop
# ``archived → archived`` is a no-op bump, not a forbidden transition, and is
# excluded from this parametrize. The two forbidden paths from terminal are
# reversals to a less-final status; those are what we assert here. (The
# ``archive()`` helper itself still rejects the self-loop because it compares
# ``"archived" in _ALLOWED_TRANSITIONS["archived"]`` which is empty — see the
# dedicated invalid-self-loop assertion in ``test_files.py``.)


@pytest.mark.parametrize("target_status", ["agent_suggested", "confirmed"])
async def test_archived_blocks_reversal_writes(
    file_store: MemoryFileStore, target_status: str
) -> None:
    """Once archived, ``write()`` must refuse any attempt to revert the
    status — both ``agent_suggested`` and ``confirmed`` are less-final and
    therefore reversals, forbidden by ``_ALLOWED_TRANSITIONS``."""
    v1 = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="x")
    )
    await file_store.archive(v1.memory_id)

    with pytest.raises(InvalidStatusTransition):
        await file_store.write(
            MemoryFileIn(
                memory_id=v1.memory_id,
                scope="s",
                category="c",
                content="x",
                status=target_status,  # type: ignore[arg-type]
            )
        )


# --- Test 7: list(status=...) filters by latest-version status -------------


async def test_list_filters_by_status(file_store: MemoryFileStore) -> None:
    """``list`` joins on latest version per memory_id (see files.py::list),
    so filtering by status returns exactly the memories whose latest row is
    in that status — never archived intermediate versions of a still-active
    memory, etc."""
    mem_a = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="a")
    )  # stays at agent_suggested
    mem_b = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="b")
    )
    await file_store.confirm(mem_b.memory_id)
    mem_c = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="c")
    )
    await file_store.archive(mem_c.memory_id)

    suggested = await file_store.list(status="agent_suggested")
    confirmed = await file_store.list(status="confirmed")
    archived = await file_store.list(status="archived")
    all_rows = await file_store.list()

    assert {m.memory_id for m in suggested} == {mem_a.memory_id}
    assert {m.memory_id for m in confirmed} == {mem_b.memory_id}
    assert {m.memory_id for m in archived} == {mem_c.memory_id}
    assert {m.memory_id for m in all_rows} == {
        mem_a.memory_id,
        mem_b.memory_id,
        mem_c.memory_id,
    }


# --- Test 8: REST layer maps invalid_transition to HTTP 409 ----------------


async def test_api_returns_409_on_invalid_transition(app: AsyncClient) -> None:
    """``/tools/memory_confirm/invoke`` on an already-archived memory must
    surface as HTTP 409 with ``code == "invalid_transition"``. FastAPI
    serializes ``HTTPException(detail={...})`` as
    ``{"detail": {"code": ..., "message": ...}}`` so we index through the
    ``detail`` envelope rather than the top level."""
    write_resp = await app.post(
        "/tools/memory_write_file/invoke",
        json={
            "args": {
                "scope": "s",
                "category": "c",
                "content": "terminal",
            }
        },
    )
    assert write_resp.status_code == 200
    memory_id = write_resp.json()["memory_id"]

    arch_resp = await app.post(
        "/tools/memory_archive/invoke",
        json={"args": {"memory_id": memory_id}},
    )
    assert arch_resp.status_code == 200

    confirm_resp = await app.post(
        "/tools/memory_confirm/invoke",
        json={"args": {"memory_id": memory_id}},
    )
    assert confirm_resp.status_code == 409
    assert confirm_resp.json()["detail"]["code"] == "invalid_transition"


# --- Test 9: REST layer maps not_found to HTTP 404 -------------------------


async def test_api_returns_404_on_missing_memory_id_confirm(
    app: AsyncClient,
) -> None:
    """``/tools/memory_confirm/invoke`` with an unknown memory_id must
    surface as HTTP 404 with ``code == "not_found"`` — same taxonomy the
    memory_archive tool uses, so CLI/L1 clients can handle both uniformly."""
    resp = await app.post(
        "/tools/memory_confirm/invoke",
        json={"args": {"memory_id": "mem_nonexistent_404_test"}},
    )
    assert resp.status_code == 404
    assert resp.json()["detail"]["code"] == "not_found"


# --- Test 10: full MCP dispatcher walk preserves embedding metadata --------


async def test_full_state_machine_via_mcp_dispatcher_preserves_embedding_metadata(
    dispatcher: McpToolDispatcher, file_store: MemoryFileStore
) -> None:
    """Full walk through the dispatcher (write → confirm → archive → read)
    lands in ``archived`` at version 3 with coherent embedding surfaces on
    the read path.

    **Scope note on asymmetric embedding surfacing**: ``MemoryFileStore.write``
    (see ``files.py:204-214``) returns a ``MemoryFileOut`` constructed inline
    from the input — it omits ``embedding_model_id`` / ``embedding_dim``,
    leaving them at the ``None`` default. Because ``confirm()`` and
    ``archive()`` both delegate to ``write()`` internally, their dispatcher
    responses inherit that omission. Only the ``memory_read`` path flows
    through ``_row_to_memory`` and therefore surfaces the persisted columns.
    This test intentionally does NOT assert cross-step equality of those
    fields — that would entangle with a known surface asymmetry belonging
    to future work. Instead we assert:

    1. The state machine walk produces exactly three persisted versions
       in strict order with the correct statuses.
    2. ``memory_read`` returns the latest row with ``version == 3`` and
       ``status == "archived"``.
    3. On the ``memory_read`` path, the embedding metadata fields are
       coherent — either both present or both absent (never half-filled)
       — and match whatever was persisted (checked via direct DB read)."""
    v1 = await dispatcher.invoke(
        "memory_write_file",
        {"scope": "s", "category": "c", "content": "embed-walk"},
    )
    memory_id = v1["memory_id"]
    assert v1["version"] == 1
    assert v1["status"] == "agent_suggested"

    v2 = await dispatcher.invoke("memory_confirm", {"memory_id": memory_id})
    assert v2["version"] == 2
    assert v2["status"] == "confirmed"

    v3 = await dispatcher.invoke("memory_archive", {"memory_id": memory_id})
    assert v3["version"] == 3
    assert v3["status"] == "archived"

    latest = await dispatcher.invoke("memory_read", {"memory_id": memory_id})
    assert latest["version"] == 3
    assert latest["status"] == "archived"

    # Read-path coherence — both populated or both None.
    mid = latest["embedding_model_id"]
    dim = latest["embedding_dim"]
    assert (mid is None) == (dim is None)

    # The dispatcher's read-path surface must match what is actually in the
    # DB row for the latest version. (Guards against _row_to_memory drift.)
    async with aiosqlite.connect(file_store.db_path) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT embedding_model_id, embedding_dim FROM memory_files "
            "WHERE memory_id = ? AND version = 3",
            (memory_id,),
        )
        row = await cur.fetchone()
    assert row is not None
    assert row["embedding_model_id"] == mid
    assert row["embedding_dim"] == dim
