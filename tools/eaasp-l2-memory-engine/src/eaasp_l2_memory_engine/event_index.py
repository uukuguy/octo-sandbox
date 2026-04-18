"""D78 — EventEmbeddingIndex: HNSW vector index for ACP event payloads.

Stores event embeddings in a separate directory tree isolated from the
memory-file HNSW index:

    {octo_root}/.grid/embeddings/events/{safe_model_id}/

This keeps event vectors from polluting memory-file recall and allows
independent index migration without touching the memory store.

Usage:
    idx = EventEmbeddingIndex(octo_root="/path/to/root")
    await idx.add(event_id="evt-123", payload_text="tool bash.execute called with ...")
    hits = await idx.search(query="bash execution", top_k=5)
    # hits: list[EventHit(event_id, score)]
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import NamedTuple

from .embedding.provider import get_embedding_provider
from .vector_index import DimensionMismatchError, HNSWVectorIndex, ModelIdMismatchError

logger = logging.getLogger(__name__)

_EVENTS_SUBDIR = ".grid/embeddings/events"


class EventHit(NamedTuple):
    event_id: str
    score: float


class EventEmbeddingIndex:
    """Wraps HNSWVectorIndex to embed and search ACP event payloads.

    Graceful-degrade policy (inherits ADR-V2-015): if the embedding provider
    or HNSW index is unavailable, ``add`` logs a warning and returns without
    raising; ``search`` returns an empty list. This ensures event ingestion
    never fails due to a missing embedding backend.
    """

    def __init__(self, octo_root: str | None = None) -> None:
        self._octo_root = octo_root or os.getcwd()

    def _events_root(self) -> str:
        return str(Path(self._octo_root) / _EVENTS_SUBDIR)

    async def add(self, event_id: str, payload_text: str) -> None:
        """Embed ``payload_text`` and store under ``event_id`` in the event index.

        No-op (with WARNING) if the embedding provider is unavailable.
        """
        try:
            embedder = get_embedding_provider()
            vec = await embedder.embed(payload_text)
            idx = HNSWVectorIndex(
                model_id=embedder.model_id,
                octo_root=self._events_root(),
                dim=embedder.dimension,
            )
            await idx.add(event_id, vec)
            await idx.save()
        except Exception as exc:  # noqa: BLE001 — degrade, never fail ingest
            logger.warning("EventEmbeddingIndex.add skipped for %s: %s", event_id, exc)

    async def search(self, query: str, top_k: int = 10) -> list[EventHit]:
        """Semantic search over embedded event payloads.

        Returns an empty list (with WARNING) if the embedding provider or
        HNSW index is unavailable.
        """
        top_k = max(1, min(int(top_k), 100))
        try:
            embedder = get_embedding_provider()
            vec = await embedder.embed(query)
            idx = HNSWVectorIndex(
                model_id=embedder.model_id,
                octo_root=self._events_root(),
                dim=embedder.dimension,
            )
            raw_hits = await idx.search(vec, top_k)
            return [EventHit(event_id=h.id, score=h.score) for h in raw_hits]
        except (ModelIdMismatchError, DimensionMismatchError) as exc:
            logger.warning("EventEmbeddingIndex.search model/dim mismatch: %s", exc)
            return []
        except Exception as exc:  # noqa: BLE001 — degrade, never fail search
            logger.warning("EventEmbeddingIndex.search skipped: %s", exc)
            return []
