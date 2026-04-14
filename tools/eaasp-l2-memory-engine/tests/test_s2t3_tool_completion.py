"""S2.T3 — memory_read embedding visibility, memory_list pagination,
and the memory_confirm MCP tool.

These tests run against the shared ``dispatcher`` / ``file_store`` fixtures
declared in ``conftest.py`` so they inherit the same tmp SQLite DB and the
same per-test isolation. They cover three code changes:

1. ``MemoryFileOut`` now surfaces ``embedding_model_id`` and
   ``embedding_dim`` (not the raw blob).
2. ``memory_list`` accepts an ``offset`` parameter (default 0, clamped
   non-negative at the dispatcher boundary).
3. ``memory_confirm`` is a first-class MCP tool with parity to
   ``memory_archive`` (same error taxonomy, same state-machine guards).
"""

from __future__ import annotations

from collections.abc import Generator
from typing import Any

import pytest

from eaasp_l2_memory_engine.embedding import reset_embedding_provider
from eaasp_l2_memory_engine.files import MemoryFileStore
from eaasp_l2_memory_engine.mcp_tools import McpToolDispatcher, ToolError

pytestmark = pytest.mark.asyncio


@pytest.fixture(autouse=True)
def _mock_embedding_env(monkeypatch: pytest.MonkeyPatch) -> Generator[None, None, None]:
    """Force every test in this module to use the deterministic
    ``MockEmbedding`` provider with a known model_id, and start each test
    with a fresh singleton so earlier tests cannot leak a different
    provider through ``get_embedding_provider``."""
    monkeypatch.setenv("EAASP_EMBEDDING_PROVIDER", "mock")
    monkeypatch.setenv("EAASP_EMBEDDING_MODEL", "mock-bge-m3:fp16")
    reset_embedding_provider()
    yield
    reset_embedding_provider()


# --- Test 1: memory_read surfaces embedding metadata ------------------------


async def test_memory_read_exposes_embedding_metadata(
    dispatcher: McpToolDispatcher,
) -> None:
    """Happy path: mock embedding is computed during write() and memory_read
    exposes model_id + dim but NOT the raw vector blob."""
    written = await dispatcher.invoke(
        "memory_write_file",
        {"scope": "s", "category": "c", "content": "salary_floor=50000"},
    )
    memory_id = written["memory_id"]

    read = await dispatcher.invoke("memory_read", {"memory_id": memory_id})
    assert read["embedding_model_id"] == "mock-bge-m3:fp16"
    assert read["embedding_dim"] == 1024
    # Must never leak the internal f32 blob to clients — it's HNSW plumbing.
    assert "embedding_vec" not in read


# --- Test 2: memory_read gracefully degrades when embedding provider fails --


async def test_memory_read_null_embedding_when_provider_unavailable(
    dispatcher: McpToolDispatcher, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Regression on the S2.T1 graceful-degrade path: when the embedding
    provider raises, the row is still inserted but with NULL embedding
    columns. ``memory_read`` must therefore return ``None`` for both
    metadata fields rather than crashing or returning stale data."""
    import eaasp_l2_memory_engine.embedding as emb_mod

    class _BoomEmbedding:
        model_id = "boom-model"
        dimension = 1024

        async def embed(self, text: str) -> list[float]:
            raise RuntimeError("simulated embed failure")

        async def embed_batch(self, texts: list[str]) -> list[list[float]]:
            raise RuntimeError("simulated embed failure")

    monkeypatch.setattr(emb_mod, "get_embedding_provider", lambda: _BoomEmbedding())

    written = await dispatcher.invoke(
        "memory_write_file",
        {"scope": "s", "category": "c", "content": "no-embed content"},
    )
    memory_id = written["memory_id"]

    read = await dispatcher.invoke("memory_read", {"memory_id": memory_id})
    assert read["embedding_model_id"] is None
    assert read["embedding_dim"] is None


# --- Test 3: memory_list pagination via offset ------------------------------


async def test_memory_list_pagination_offset(
    dispatcher: McpToolDispatcher,
) -> None:
    """Three non-overlapping pages cover all 10 rows exactly once."""
    for i in range(10):
        await dispatcher.invoke(
            "memory_write_file",
            {"scope": "page-scope", "category": "c", "content": f"entry {i}"},
        )

    page_one = await dispatcher.invoke(
        "memory_list", {"scope": "page-scope", "limit": 3, "offset": 0}
    )
    page_two = await dispatcher.invoke(
        "memory_list", {"scope": "page-scope", "limit": 3, "offset": 3}
    )
    page_three = await dispatcher.invoke(
        "memory_list", {"scope": "page-scope", "limit": 3, "offset": 9}
    )

    assert len(page_one["memories"]) == 3
    assert len(page_two["memories"]) == 3
    assert len(page_three["memories"]) == 1  # 10th row only

    # Pages must not overlap on memory_id — each of the 10 writes gets a
    # unique memory_id, so pages one+two cover 6 distinct ids, three gives
    # the 10th, and the missing two rows (index 6/7 in updated_at DESC)
    # sit on the unrequested page between offset=6 and offset=9.
    ids_one = {m["memory_id"] for m in page_one["memories"]}
    ids_two = {m["memory_id"] for m in page_two["memories"]}
    ids_three = {m["memory_id"] for m in page_three["memories"]}
    assert ids_one.isdisjoint(ids_two)
    assert ids_two.isdisjoint(ids_three)
    assert ids_one.isdisjoint(ids_three)

    # Ordering invariant: updated_at strictly non-increasing within a page.
    def _non_increasing(items: list[dict[str, Any]]) -> bool:
        return all(
            items[i]["updated_at"] >= items[i + 1]["updated_at"]
            for i in range(len(items) - 1)
        )

    assert _non_increasing(page_one["memories"])
    assert _non_increasing(page_two["memories"])


# --- Test 4: memory_list default offset is zero -----------------------------


async def test_memory_list_offset_default_is_zero(
    dispatcher: McpToolDispatcher,
) -> None:
    """Omitting ``offset`` must be indistinguishable from ``offset=0``."""
    for i in range(5):
        await dispatcher.invoke(
            "memory_write_file",
            {"scope": "default-offset", "category": "c", "content": f"e{i}"},
        )

    without_offset = await dispatcher.invoke(
        "memory_list", {"scope": "default-offset", "limit": 3}
    )
    with_explicit_zero = await dispatcher.invoke(
        "memory_list", {"scope": "default-offset", "limit": 3, "offset": 0}
    )

    ids_without = [m["memory_id"] for m in without_offset["memories"]]
    ids_with = [m["memory_id"] for m in with_explicit_zero["memories"]]
    assert ids_without == ids_with


# --- Test 5: memory_confirm transitions agent_suggested → confirmed ---------


async def test_memory_confirm_state_transition(
    dispatcher: McpToolDispatcher,
) -> None:
    """memory_confirm bumps version and flips status; memory_read reflects
    both changes immediately (latest version only)."""
    written = await dispatcher.invoke(
        "memory_write_file",
        {"scope": "s", "category": "c", "content": "v1"},
    )
    memory_id = written["memory_id"]
    assert written["status"] == "agent_suggested"
    assert written["version"] == 1

    confirmed = await dispatcher.invoke(
        "memory_confirm", {"memory_id": memory_id}
    )
    assert confirmed["memory_id"] == memory_id
    assert confirmed["status"] == "confirmed"
    assert confirmed["version"] == 2

    read = await dispatcher.invoke("memory_read", {"memory_id": memory_id})
    assert read["status"] == "confirmed"
    assert read["version"] == 2

    # Reviewer nit: confirmed → confirmed self-loop must raise invalid_transition.
    # _ALLOWED_TRANSITIONS["confirmed"] = {"archived"} — no idempotent re-confirm.
    with pytest.raises(ToolError) as exc_info:
        await dispatcher.invoke("memory_confirm", {"memory_id": memory_id})
    assert exc_info.value.code == "invalid_transition"


# --- Test 6: memory_confirm refuses archived → confirmed --------------------


async def test_memory_confirm_invalid_transition_from_archived(
    dispatcher: McpToolDispatcher,
) -> None:
    """archived is terminal; reviving to confirmed is not allowed and must
    surface as ToolError("invalid_transition") (HTTP 409 via api.py)."""
    written = await dispatcher.invoke(
        "memory_write_file",
        {"scope": "s", "category": "c", "content": "x"},
    )
    await dispatcher.invoke("memory_archive", {"memory_id": written["memory_id"]})

    with pytest.raises(ToolError) as exc:
        await dispatcher.invoke(
            "memory_confirm", {"memory_id": written["memory_id"]}
        )
    assert exc.value.code == "invalid_transition"


# --- Test 7: memory_confirm on unknown id ----------------------------------


async def test_memory_confirm_not_found(dispatcher: McpToolDispatcher) -> None:
    """Unknown memory_id must surface as ToolError("not_found") — same code
    the archive tool uses, so L1 / CLI clients can handle both uniformly."""
    with pytest.raises(ToolError) as exc:
        await dispatcher.invoke(
            "memory_confirm", {"memory_id": "mem_nonexistent"}
        )
    assert exc.value.code == "not_found"


# --- Test 8: full state machine walk ---------------------------------------


async def test_full_state_machine_suggested_confirmed_archived(
    dispatcher: McpToolDispatcher, file_store: MemoryFileStore
) -> None:
    """End-to-end: write → confirm → archive yields three versions with the
    expected statuses. This doubles as prep for S2.T4 (history queries)."""
    written = await dispatcher.invoke(
        "memory_write_file",
        {"scope": "s", "category": "c", "content": "state-machine-walk"},
    )
    memory_id = written["memory_id"]

    confirmed = await dispatcher.invoke(
        "memory_confirm", {"memory_id": memory_id}
    )
    assert confirmed["version"] == 2
    assert confirmed["status"] == "confirmed"

    archived = await dispatcher.invoke(
        "memory_archive", {"memory_id": memory_id}
    )
    assert archived["version"] == 3
    assert archived["status"] == "archived"

    # Drop through the file_store to confirm all three versions persist with
    # the expected statuses. dispatcher.memory_list only returns the latest
    # version, so we query the underlying DB via the store helper.
    import aiosqlite

    async with aiosqlite.connect(file_store.db_path) as db:
        db.row_factory = aiosqlite.Row
        cur = await db.execute(
            "SELECT version, status FROM memory_files WHERE memory_id = ? "
            "ORDER BY version ASC",
            (memory_id,),
        )
        rows = await cur.fetchall()

    assert [r["version"] for r in rows] == [1, 2, 3]
    assert [r["status"] for r in rows] == [
        "agent_suggested",
        "confirmed",
        "archived",
    ]
