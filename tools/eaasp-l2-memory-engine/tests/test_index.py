"""Layer 3 — Hybrid index (FTS5 + semantic + time-decay) tests."""

from __future__ import annotations

import os

import pytest

from eaasp_l2_memory_engine.files import MemoryFileIn, MemoryFileStore
from eaasp_l2_memory_engine.index import HybridIndex


pytestmark = pytest.mark.asyncio


async def test_keyword_search_hit(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    await file_store.write(
        MemoryFileIn(
            scope="user:alice",
            category="threshold",
            content="salary floor 50000 for engineers",
        )
    )
    await file_store.write(
        MemoryFileIn(
            scope="user:alice",
            category="preference",
            content="prefers remote work over office",
        )
    )

    hits = await index.search("salary")
    assert len(hits) == 1
    assert "salary" in hits[0].memory.content


async def test_search_scope_filter(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    await file_store.write(
        MemoryFileIn(scope="alice", category="c", content="python rocks")
    )
    await file_store.write(
        MemoryFileIn(scope="bob", category="c", content="python rocks")
    )

    alice_hits = await index.search("python", scope="alice")
    assert len(alice_hits) == 1
    assert alice_hits[0].memory.scope == "alice"


async def test_search_returns_latest_version_only(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    first = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="alpha beta gamma")
    )
    await file_store.write(
        MemoryFileIn(
            memory_id=first.memory_id,
            scope="s",
            category="c",
            content="alpha delta epsilon",
        )
    )

    hits = await index.search("alpha")
    assert len(hits) == 1
    assert hits[0].memory.version == 2
    assert "delta" in hits[0].memory.content


async def test_search_empty_query_returns_empty(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="anything")
    )
    hits = await index.search("   ")
    assert hits == []


async def test_time_decay_weights_recent_higher(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    # both match the same token, so fts_score is comparable; time decay decides order
    old = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="widget widget")
    )
    # Simulate an older memory by backdating updated_at directly
    import aiosqlite

    async with aiosqlite.connect(file_store.db_path) as db:
        old_ts = 0  # epoch
        await db.execute(
            "UPDATE memory_files SET updated_at = ? WHERE memory_id = ?",
            (old_ts, old.memory_id),
        )
        await db.commit()

    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="widget widget")
    )

    hits = await index.search("widget", top_k=5)
    assert len(hits) == 2
    # Newer entry should rank first due to time_decay
    assert hits[0].memory.memory_id != old.memory_id
    assert hits[0].time_decay > hits[1].time_decay


# ---------------------------------------------------------------------------
# S2.T2 — hybrid semantic + keyword + time-decay tests
# ---------------------------------------------------------------------------


async def test_semantic_score_field_populated_on_keyword_hit(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    """SearchHit returned from a keyword match has a valid semantic_score
    field. With MockEmbedding the query and memory embeddings are both
    computed, so semantic_score should be a float in [0, 1]."""
    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="test content")
    )

    hits = await index.search("test")
    assert len(hits) == 1
    assert hasattr(hits[0], "semantic_score")
    # Whatever the value, it must be a finite float inside [0, 1].
    assert isinstance(hits[0].semantic_score, float)
    assert 0.0 <= hits[0].semantic_score <= 1.0


async def test_weighted_fusion_keyword_only_ignores_semantic(
    file_store: MemoryFileStore, db_path: str
) -> None:
    """HybridIndex(weights=(1.0, 0.0)) should produce scores that equal
    fts_score * time_decay exactly, regardless of semantic_score value."""
    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="keyword rich content")
    )

    index = HybridIndex(db_path, weights=(1.0, 0.0))
    hits = await index.search("keyword")

    assert len(hits) == 1
    hit = hits[0]
    # With w_sem=0, semantic_score contribution is zero, so:
    #   score == w_fts * fts_score * decay == 1.0 * fts_score * decay
    expected = 1.0 * hit.fts_score * hit.time_decay
    assert abs(hit.score - expected) < 1e-9


async def test_weighted_fusion_semantic_only_ignores_keyword(
    file_store: MemoryFileStore, db_path: str
) -> None:
    """HybridIndex(weights=(0.0, 1.0)) should produce scores equal to
    semantic_score * time_decay exactly (keyword contribution zeroed)."""
    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="keyword rich content")
    )

    index = HybridIndex(db_path, weights=(0.0, 1.0))
    hits = await index.search("keyword")

    assert len(hits) == 1
    hit = hits[0]
    expected = 1.0 * hit.semantic_score * hit.time_decay
    assert abs(hit.score - expected) < 1e-9


async def test_graceful_degrade_on_embedder_error(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    """Monkey-patch the provider factory to raise → search still returns
    keyword-only results with semantic_score == 0.0."""
    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="test content")
    )

    from eaasp_l2_memory_engine.embedding import provider as prov_mod

    original_get = prov_mod.get_embedding_provider

    def broken_provider() -> object:
        class BrokenProvider:
            @property
            def model_id(self) -> str:
                return "mock-bge-m3:fp16"

            @property
            def dimension(self) -> int:
                return 1024

            async def embed(self, text: str) -> list[float]:
                raise RuntimeError("Embedding service down")

            async def embed_batch(self, texts: list[str]) -> list[list[float]]:
                raise RuntimeError("Embedding service down")

        return BrokenProvider()

    prov_mod.get_embedding_provider = broken_provider  # type: ignore[assignment]
    try:
        hits = await index.search("test")
        assert len(hits) == 1
        assert hits[0].semantic_score == 0.0
        # fts_score > 0 because keyword still matched
        assert hits[0].fts_score > 0.0
    finally:
        prov_mod.get_embedding_provider = original_get
        prov_mod.reset_embedding_provider()


async def test_graceful_degrade_on_model_id_mismatch(
    file_store: MemoryFileStore, db_path: str
) -> None:
    """Seed HNSW with default MockEmbedding, switch provider to a different
    model_id → degrade to keyword-only (semantic_score == 0)."""
    from eaasp_l2_memory_engine.embedding import provider as prov_mod
    from eaasp_l2_memory_engine.embedding.provider import MockEmbedding

    # Ensure a clean singleton so the first write uses the default mock model.
    prov_mod.reset_embedding_provider()

    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="test")
    )
    # HNSW now has one entry under default MockEmbedding.model_id.

    # Swap the singleton to a *different* model_id before search.
    prov_mod._PROVIDER_INSTANCE = MockEmbedding(model="alternate-model:v2")
    try:
        index = HybridIndex(db_path)
        hits = await index.search("test")
        # Must still return the keyword hit, with semantic disabled.
        assert len(hits) == 1
        assert hits[0].semantic_score == 0.0
    finally:
        prov_mod.reset_embedding_provider()


async def test_dedupe_by_memory_id_keeps_latest_version_hybrid(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    """When v1 and v2 are both in HNSW, search returns only v2 (union
    dedupe by memory_id keeping max version)."""
    v1 = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="alpha beta gamma")
    )
    await file_store.write(
        MemoryFileIn(
            memory_id=v1.memory_id,
            scope="s",
            category="c",
            content="alpha delta epsilon",
        )
    )

    hits = await index.search("alpha")
    assert len(hits) == 1
    assert hits[0].memory.version == 2
    assert "delta" in hits[0].memory.content


async def test_weights_env_parse_fallback_on_bad_value(
    db_path: str,
) -> None:
    """EAASP_HYBRID_WEIGHTS malformed → falls back to (0.5, 0.5) default."""
    old_val = os.environ.get("EAASP_HYBRID_WEIGHTS")
    os.environ["EAASP_HYBRID_WEIGHTS"] = "not,a,tuple"
    try:
        index = HybridIndex(db_path)
        assert index.w_fts == 0.5
        assert index.w_sem == 0.5
    finally:
        if old_val is None:
            os.environ.pop("EAASP_HYBRID_WEIGHTS", None)
        else:
            os.environ["EAASP_HYBRID_WEIGHTS"] = old_val


async def test_weights_env_parse_happy_path(
    db_path: str,
) -> None:
    """EAASP_HYBRID_WEIGHTS='0.3,0.7' is picked up; whitespace tolerated."""
    old_val = os.environ.get("EAASP_HYBRID_WEIGHTS")
    os.environ["EAASP_HYBRID_WEIGHTS"] = " 0.3 , 0.7 "
    try:
        index = HybridIndex(db_path)
        assert index.w_fts == pytest.approx(0.3)
        assert index.w_sem == pytest.approx(0.7)
    finally:
        if old_val is None:
            os.environ.pop("EAASP_HYBRID_WEIGHTS", None)
        else:
            os.environ["EAASP_HYBRID_WEIGHTS"] = old_val


async def test_weights_clamped_to_unit_interval(
    db_path: str,
) -> None:
    """Out-of-range weights are silently clamped to [0, 1]."""
    idx_neg = HybridIndex(db_path, weights=(-1.0, 2.0))
    assert idx_neg.w_fts == 0.0
    assert idx_neg.w_sem == 1.0


async def test_empty_query_returns_empty_with_semantic_enabled(
    file_store: MemoryFileStore, index: HybridIndex
) -> None:
    """Blank/whitespace queries short-circuit before embedding — no call to
    the provider, no HNSW lookup, just []."""
    await file_store.write(
        MemoryFileIn(scope="s", category="c", content="anything")
    )

    assert await index.search("   ") == []
    assert await index.search("") == []


async def test_graceful_degrade_on_missing_hnsw_dir(
    file_store: MemoryFileStore, db_path: str, tmp_path
) -> None:
    """When octo_root points at a directory with no HNSW index, search
    still returns the FTS hit (semantic_score=0.0 because HNSW is empty)."""
    import shutil

    mem = await file_store.write(
        MemoryFileIn(scope="s", category="c", content="find me keyword")
    )

    # Point octo_root at a *fresh* directory that has no HNSW seed.
    fresh_root = tmp_path / "fresh_octo_root"
    fresh_root.mkdir()
    # Also pre-emptively remove any HNSW dir that might have been seeded via
    # the file_store default octo_root (dirname(db_path)).
    default_hnsw = os.path.join(
        os.path.dirname(os.path.abspath(db_path)), "l2-memory"
    )
    if os.path.isdir(default_hnsw):
        shutil.rmtree(default_hnsw)

    index = HybridIndex(db_path, octo_root=str(fresh_root))
    hits = await index.search("keyword")

    assert len(hits) == 1
    assert hits[0].memory.memory_id == mem.memory_id
    # An empty HNSW is still a *successful* load, so semantic_score is 0
    # because there's nothing to match against.
    assert hits[0].semantic_score == 0.0
