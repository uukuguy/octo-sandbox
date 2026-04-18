"""D78 — EventEmbeddingIndex tests.

Tests cover:
  1. add() with mock embedding — writes to isolated events dir
  2. add() with no embedding provider — graceful degrade (no raise)
  3. search() returns EventHit list via mock HNSW
  4. search() with no provider — returns empty list (no raise)
  5. ingest_event API endpoint — 200 + {indexed: true}
  6. ingest_event API endpoint — embedding failure still returns 200
"""

from __future__ import annotations

import os
import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
import pytest_asyncio

from eaasp_l2_memory_engine.event_index import EventEmbeddingIndex


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _mock_embedder(dim: int = 4, model_id: str = "mock-model") -> MagicMock:
    emb = MagicMock()
    emb.model_id = model_id
    emb.dimension = dim
    emb.embed = AsyncMock(return_value=[0.1] * dim)
    return emb


# ---------------------------------------------------------------------------
# Unit tests for EventEmbeddingIndex
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_add_creates_event_in_index(tmp_path: Path) -> None:
    """add() embeds text and saves to events HNSW dir."""
    embedder = _mock_embedder()
    saved_ids: list[str] = []

    mock_hnsw = AsyncMock()
    mock_hnsw.add = AsyncMock(side_effect=lambda id, vec: saved_ids.append(id))
    mock_hnsw.save = AsyncMock()

    with (
        patch(
            "eaasp_l2_memory_engine.event_index.get_embedding_provider",
            return_value=embedder,
        ),
        patch(
            "eaasp_l2_memory_engine.event_index.HNSWVectorIndex",
            return_value=mock_hnsw,
        ),
    ):
        idx = EventEmbeddingIndex(octo_root=str(tmp_path))
        await idx.add("evt-001", "tool bash.execute called")

    assert "evt-001" in saved_ids
    embedder.embed.assert_awaited_once_with("tool bash.execute called")
    mock_hnsw.save.assert_awaited_once()


@pytest.mark.asyncio
async def test_add_graceful_degrade_no_provider(tmp_path: Path) -> None:
    """add() with unavailable embedding provider must not raise."""
    with patch(
        "eaasp_l2_memory_engine.event_index.get_embedding_provider",
        side_effect=RuntimeError("no provider"),
    ):
        idx = EventEmbeddingIndex(octo_root=str(tmp_path))
        # Should not raise
        await idx.add("evt-002", "some payload")


@pytest.mark.asyncio
async def test_search_returns_event_hits(tmp_path: Path) -> None:
    """search() returns EventHit list from HNSW results."""
    from eaasp_l2_memory_engine.vector_index import Hit

    embedder = _mock_embedder()
    mock_hnsw = AsyncMock()
    mock_hnsw.search = AsyncMock(return_value=[Hit(id="evt-010", score=0.92)])

    with (
        patch(
            "eaasp_l2_memory_engine.event_index.get_embedding_provider",
            return_value=embedder,
        ),
        patch(
            "eaasp_l2_memory_engine.event_index.HNSWVectorIndex",
            return_value=mock_hnsw,
        ),
    ):
        idx = EventEmbeddingIndex(octo_root=str(tmp_path))
        hits = await idx.search("bash execution", top_k=5)

    assert len(hits) == 1
    assert hits[0].event_id == "evt-010"
    assert hits[0].score == pytest.approx(0.92)


@pytest.mark.asyncio
async def test_search_graceful_degrade_no_provider(tmp_path: Path) -> None:
    """search() returns [] when embedding provider is unavailable."""
    with patch(
        "eaasp_l2_memory_engine.event_index.get_embedding_provider",
        side_effect=RuntimeError("no provider"),
    ):
        idx = EventEmbeddingIndex(octo_root=str(tmp_path))
        hits = await idx.search("anything")

    assert hits == []


@pytest.mark.asyncio
async def test_events_root_is_isolated(tmp_path: Path) -> None:
    """EventEmbeddingIndex uses .grid/embeddings/events/ not the memory HNSW dir."""
    idx = EventEmbeddingIndex(octo_root=str(tmp_path))
    events_root = idx._events_root()
    assert events_root == str(tmp_path / ".grid" / "embeddings" / "events")
    # Must not overlap with memory-file HNSW path (l2-memory/hnsw-*)
    assert "l2-memory" not in events_root


# ---------------------------------------------------------------------------
# API endpoint tests
# ---------------------------------------------------------------------------


@pytest_asyncio.fixture
async def app_client(db_path: str):  # type: ignore[no-untyped-def]
    from httpx import ASGITransport, AsyncClient

    from eaasp_l2_memory_engine.api import create_app

    application = create_app(db_path)
    transport = ASGITransport(app=application)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        yield client


@pytest.mark.asyncio
async def test_ingest_event_endpoint_returns_indexed(app_client) -> None:  # type: ignore[no-untyped-def]
    """POST /api/v1/events/ingest returns {indexed: true} even without embedder."""
    with patch(
        "eaasp_l2_memory_engine.event_index.get_embedding_provider",
        side_effect=RuntimeError("no provider in test"),
    ):
        resp = await app_client.post(
            "/api/v1/events/ingest",
            json={"event_id": "evt-api-001", "payload_text": "tool call result"},
        )
    assert resp.status_code == 200
    data = resp.json()
    assert data["event_id"] == "evt-api-001"
    assert data["indexed"] is True


@pytest.mark.asyncio
async def test_ingest_event_endpoint_with_mock_embedder(app_client) -> None:  # type: ignore[no-untyped-def]
    """POST /api/v1/events/ingest embeds when provider is available."""
    embedder = _mock_embedder()
    mock_hnsw = AsyncMock()
    mock_hnsw.add = AsyncMock()
    mock_hnsw.save = AsyncMock()

    with (
        patch(
            "eaasp_l2_memory_engine.event_index.get_embedding_provider",
            return_value=embedder,
        ),
        patch(
            "eaasp_l2_memory_engine.event_index.HNSWVectorIndex",
            return_value=mock_hnsw,
        ),
    ):
        resp = await app_client.post(
            "/api/v1/events/ingest",
            json={"event_id": "evt-api-002", "payload_text": "memory write anchor"},
        )
    assert resp.status_code == 200
    assert resp.json()["indexed"] is True
    mock_hnsw.add.assert_awaited_once()
    mock_hnsw.save.assert_awaited_once()
