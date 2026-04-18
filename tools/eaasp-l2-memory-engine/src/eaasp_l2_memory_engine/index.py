"""Layer 3 — Hybrid Retrieval Index.

S2.T2 scoring model (keyword via FTS5 + semantic via HNSW + time-decay):

    score = (w_fts * fts_score + w_sem * semantic_score) * time_decay

where:
    fts_score       = normalized BM25 from FTS5 (higher = better match, 0 if
                      the memory was only surfaced via HNSW)
    semantic_score  = cosine similarity in [0, 1] from HNSW query against
                      the same embedding model_id used at write time; 0 when
                      semantic is unavailable or the memory was only surfaced
                      via FTS5
    time_decay      = exp(-age_days / HALF_LIFE_DAYS), age from ``updated_at``
    w_fts / w_sem   = fusion weights (default 0.5 / 0.5, overridable via
                      env ``EAASP_HYBRID_WEIGHTS="w_fts,w_sem"`` or the
                      ``weights=`` constructor argument)

Graceful-degrade policy (per ADR-V2-015): if the embedding provider or HNSW
index is unavailable (missing, corrupt, model_id / dim mismatch), the search
silently falls back to keyword-only mode (``semantic_score = 0.0`` everywhere,
ranking preserved). A ``WARNING`` log is emitted but the request never fails.
"""

from __future__ import annotations

import logging
import math
import os
import re
import time
from typing import Any

from pydantic import BaseModel

from .db import get_shared_connection
from .files import MemoryFileOut, _row_to_memory

logger = logging.getLogger(__name__)

HALF_LIFE_DAYS = 30.0
_MS_PER_DAY = 86_400_000.0
MAX_TOP_K = 100

# Allow letters, digits, whitespace, and common punctuation inside a phrase.
# Everything else (FTS5 operators, quotes, special chars) is stripped.
_FTS_SAFE = re.compile(r"[^\w\s\-_.]", flags=re.UNICODE)


class SearchHit(BaseModel):
    memory: MemoryFileOut
    score: float
    fts_score: float
    time_decay: float
    # S2.T2: cosine similarity in [0, 1]; 0.0 in keyword-only mode or when a
    # FTS-surfaced memory has no corresponding HNSW hit (see D95+ follow-up).
    # Defaults to 0.0 for backward compatibility with pre-S2.T2 callers.
    semantic_score: float = 0.0


def _sanitize_query(query: str) -> str | None:
    """Return an FTS5-safe phrase query, or None for empty/unusable input.

    C2: strips all FTS5 operator characters (`*`, `^`, `:`, `"`, `(`, `)`, etc.)
    before wrapping in a phrase, preventing syntax errors and DoS via adversarial
    queries.
    """
    cleaned = _FTS_SAFE.sub(" ", query).strip()
    cleaned = " ".join(cleaned.split())  # collapse whitespace
    if not cleaned:
        return None
    return '"' + cleaned + '"'


def _time_decay(updated_at_ms: int, now_ms: int) -> float:
    age_days = max(0.0, (now_ms - updated_at_ms) / _MS_PER_DAY)
    return math.exp(-age_days / HALF_LIFE_DAYS)


def _parse_weights_from_env(
    env_var: str,
    default: tuple[float, float],
) -> tuple[float, float]:
    """Parse comma-separated weights from env, fallback to default on any error.

    Accepts ``"0.5,0.5"`` style. Malformed values emit a WARNING and return the
    default. We intentionally tolerate whitespace around commas so
    ``" 0.3 , 0.7 "`` works as well.
    """
    env_val = os.getenv(env_var, "").strip()
    if not env_val:
        return default
    try:
        parts = env_val.split(",")
        if len(parts) != 2:
            raise ValueError(f"expected 2 weights, got {len(parts)}")
        w_fts = float(parts[0].strip())
        w_sem = float(parts[1].strip())
        return (w_fts, w_sem)
    except (ValueError, TypeError) as e:
        logger.warning(
            "Failed to parse %s=%r, using default %s: %s",
            env_var,
            env_val,
            default,
            e,
        )
        return default


class HybridIndex:
    """Hybrid keyword + semantic + time-decay retrieval.

    NOTE on model_id migration (ADR-V2-015 iron law 3): changing
    ``EAASP_EMBEDDING_PROVIDER`` or the embedding model at runtime requires
    manual HNSW index migration (dual-write + reindex + cutover). Queries
    against a stale HNSW dir will raise :class:`ModelIdMismatchError` /
    :class:`DimensionMismatchError`; :meth:`search` catches them and degrades
    to keyword-only until operators reindex.
    """

    def __init__(
        self,
        db_path: str,
        *,
        octo_root: str | None = None,
        weights: tuple[float, float] | None = None,
    ) -> None:
        """Construct hybrid index.

        Args:
            db_path: Path to SQLite database (``memory_files`` + ``memory_fts``
                tables).
            octo_root: Root directory for HNSW indices
                (``{octo_root}/l2-memory/hnsw-{safe_model_id}/``). Defaults to
                ``dirname(db_path)`` to match :class:`MemoryFileStore` so that
                test fixtures using ``tmp_path`` get isolated indices
                automatically.
            weights: Tuple ``(w_fts, w_sem)`` for the fusion formula. When
                ``None`` (the default) parsed from env
                ``EAASP_HYBRID_WEIGHTS="w_fts,w_sem"``; if the env var is
                unset or malformed, falls back to ``(0.5, 0.5)``.
        """
        self.db_path = db_path
        self.octo_root = octo_root or os.path.dirname(os.path.abspath(db_path))

        if weights is None:
            weights = _parse_weights_from_env("EAASP_HYBRID_WEIGHTS", (0.5, 0.5))

        # Clamp to [0, 1] — out-of-range weights silently snap rather than raise;
        # this keeps ops safe if someone sets a negative value in config.
        self.w_fts = max(0.0, min(1.0, weights[0]))
        self.w_sem = max(0.0, min(1.0, weights[1]))

    async def search(
        self,
        query: str,
        top_k: int = 10,
        scope: str | None = None,
        category: str | None = None,
    ) -> list[SearchHit]:
        """Hybrid keyword + semantic + time-decay search.

        Algorithm:
          1. Sanitize query and embed with the current provider (graceful
             degrade if embedding fails).
          2. Oversample from FTS5 (``top_k * 4``) and HNSW (``top_k * 4``).
          3. Union by ``memory_id``, deduped to the latest version.
          4. Rescore both dimensions:
             ``score = (w_fts * fts + w_sem * sem) * decay``
          5. Return the top ``top_k`` sorted by final score.

        Graceful degrade: if the embedding provider or HNSW index is
        unavailable (offline, corrupt, model_id or dim mismatch), the search
        reverts to keyword-only mode. A WARNING is logged and
        ``semantic_score`` is set to ``0.0`` on every hit. Ranking is
        preserved (FTS scores still order results); only the absolute score
        magnitude is reduced by the ``w_sem`` fraction. Callers that care
        about semantic availability can check ``hit.semantic_score``.

        NOTE: in keyword-only mode with default weights ``(0.5, 0.5)`` the
        final score is half the pre-S2.T2 magnitude, but ranking is
        preserved because FTS determines order. This is acceptable for
        graceful degrade — temporary semantic unavailability does not break
        search UX.
        """
        # C3: bound top_k (unbounded causes memory blow-up on crafted input).
        top_k = max(1, min(int(top_k), MAX_TOP_K))

        fts_query = _sanitize_query(query)
        if fts_query is None:
            return []

        # ------------------------------------------------------------------
        # STEP 1 — Try to embed the query
        # ------------------------------------------------------------------
        keyword_only = False
        query_embedding: list[float] | None = None
        embedder_model_id: str | None = None
        embedder_dim: int | None = None

        try:
            from .embedding.provider import get_embedding_provider

            embedder = get_embedding_provider()
            query_embedding = await embedder.embed(query)
            embedder_model_id = embedder.model_id
            embedder_dim = embedder.dimension
        except Exception as e:  # noqa: BLE001 — degrade, never fail search
            logger.warning(
                "hybrid_search embedding skipped, degrading to keyword-only: %s",
                e,
            )
            keyword_only = True

        # ------------------------------------------------------------------
        # STEP 2 — Try to load HNSW index matching the embedder
        # ------------------------------------------------------------------
        hnsw_index: Any = None

        if (
            not keyword_only
            and query_embedding is not None
            and embedder_model_id is not None
            and embedder_dim is not None
        ):
            try:
                from .vector_index import (
                    DimensionMismatchError,
                    HNSWVectorIndex,
                    ModelIdMismatchError,
                )

                try:
                    hnsw_index = HNSWVectorIndex(
                        model_id=embedder_model_id,
                        octo_root=self.octo_root,
                        dim=embedder_dim,
                    )
                except (ModelIdMismatchError, DimensionMismatchError) as e:
                    logger.warning(
                        "hybrid_search HNSW model_id/dim mismatch, "
                        "degrading to keyword-only: %s",
                        e,
                    )
                    keyword_only = True
                    hnsw_index = None
            except Exception as e:  # noqa: BLE001 — degrade on any load error
                logger.warning(
                    "hybrid_search HNSW load failed, degrading to keyword-only: %s",
                    e,
                )
                keyword_only = True
                hnsw_index = None

        # ------------------------------------------------------------------
        # STEP 3 — FTS5 oversample (top_k * 4)
        # ------------------------------------------------------------------
        # M2: join against (memory_id, MAX(version)) inside the query instead
        # of doing one _is_latest() call per candidate row.
        sql = """
            SELECT mf.*, bm25(memory_fts) AS rank
            FROM memory_fts
            JOIN memory_files mf
              ON memory_fts.memory_id = mf.memory_id
             AND memory_fts.version = mf.version
            JOIN (
                SELECT memory_id, MAX(version) AS mv
                FROM memory_files
                GROUP BY memory_id
            ) latest
              ON mf.memory_id = latest.memory_id AND mf.version = latest.mv
            WHERE memory_fts MATCH ?
        """
        params: list[Any] = [fts_query]
        if scope is not None:
            sql += " AND mf.scope = ?"
            params.append(scope)
        if category is not None:
            sql += " AND mf.category = ?"
            params.append(category)
        sql += " ORDER BY rank ASC LIMIT ?"
        params.append(top_k * 4)

        db = await get_shared_connection(self.db_path)
        try:
            cur = await db.execute(sql, params)
            fts_rows = await cur.fetchall()
        except Exception:
            # Defense in depth against any residual FTS5 parse error.
            fts_rows = []

        now_ms = int(time.time() * 1000)

        # Build FTS candidate map: memory_id → (MemoryFileOut, fts_score).
        # BM25 in SQLite returns lower = better; normalize so larger = better.
        fts_candidates: dict[str, tuple[MemoryFileOut, float]] = {}
        for row in fts_rows:
            memory = _row_to_memory(row)
            bm25 = row["rank"]
            fts_score = 1.0 / (1.0 + max(bm25, 0.0))
            fts_candidates[memory.memory_id] = (memory, fts_score)

        # ------------------------------------------------------------------
        # STEP 4 — HNSW search + union (skipped in keyword-only mode)
        # ------------------------------------------------------------------
        hnsw_candidates: dict[str, tuple[MemoryFileOut, float]] = {}

        if (
            not keyword_only
            and hnsw_index is not None
            and query_embedding is not None
        ):
            try:
                hnsw_hits = await hnsw_index.search(
                    query_embedding, top_k=top_k * 4
                )

                # Dedupe HNSW hits by memory_id, keeping the highest version.
                # HNSW keys follow the ``memory_id:vN`` convention written by
                # MemoryFileStore (files.py:194).
                hnsw_by_id: dict[str, tuple[float, int]] = {}
                for hit in hnsw_hits:
                    parts = hit.id.split(":v")
                    if len(parts) != 2:
                        continue
                    mem_id, version_str = parts
                    try:
                        version = int(version_str)
                    except ValueError:
                        continue
                    # Clamp cosine to [0, 1]: cosine space on unit vectors is
                    # in [-1, 1], but bge-m3 / mock embeddings are normalized
                    # so values should already sit in [0, 1]. Snap just in
                    # case to keep the fusion formula well-behaved.
                    clamped = max(0.0, min(1.0, hit.score))
                    if (
                        mem_id not in hnsw_by_id
                        or version > hnsw_by_id[mem_id][1]
                    ):
                        hnsw_by_id[mem_id] = (clamped, version)

                # Fetch memories not already covered by FTS so the union is
                # complete. Do it in a single IN (...) query.
                # IMPORTANT: scope/category filters MUST be applied here too,
                # otherwise HNSW would surface out-of-scope memories that
                # scope-filtered FTS deliberately excluded. This keeps the
                # search scope-safe.
                missing_ids = [
                    mid for mid in hnsw_by_id if mid not in fts_candidates
                ]
                if missing_ids:
                    placeholders = ",".join("?" * len(missing_ids))
                    fetch_sql = f"""
                        SELECT mf.*
                        FROM memory_files mf
                        JOIN (
                            SELECT memory_id, MAX(version) AS mv
                            FROM memory_files
                            GROUP BY memory_id
                        ) latest
                          ON mf.memory_id = latest.memory_id
                         AND mf.version = latest.mv
                        WHERE mf.memory_id IN ({placeholders})
                    """
                    fetch_params: list[Any] = list(missing_ids)
                    if scope is not None:
                        fetch_sql += " AND mf.scope = ?"
                        fetch_params.append(scope)
                    if category is not None:
                        fetch_sql += " AND mf.category = ?"
                        fetch_params.append(category)

                    db = await get_shared_connection(self.db_path)
                    cur = await db.execute(fetch_sql, fetch_params)
                    missing_rows = await cur.fetchall()

                    for row in missing_rows:
                        memory = _row_to_memory(row)
                        if memory.memory_id in hnsw_by_id:
                            sem_score, _ = hnsw_by_id[memory.memory_id]
                            hnsw_candidates[memory.memory_id] = (
                                memory,
                                sem_score,
                            )

                # Also populate semantic scores for IDs that are in BOTH
                # FTS and HNSW — the union pass reads hnsw_candidates to
                # decorate FTS hits with their cosine scores.
                for mem_id, (sem_score, _) in hnsw_by_id.items():
                    if mem_id in fts_candidates:
                        memory, _ = fts_candidates[mem_id]
                        hnsw_candidates[mem_id] = (memory, sem_score)
            except Exception as e:  # noqa: BLE001 — degrade on search error
                logger.warning("hybrid_search HNSW search failed: %s", e)
                # Continue with FTS-only candidates; semantic_score stays 0.

        # ------------------------------------------------------------------
        # STEP 5 — Union + rescore
        # ------------------------------------------------------------------
        # Union key: memory_id → (MemoryFileOut, fts_score, sem_score)
        union: dict[str, tuple[MemoryFileOut, float, float]] = {}

        for mem_id, (memory, fts_score) in fts_candidates.items():
            sem_score = 0.0
            if mem_id in hnsw_candidates:
                _, sem_score = hnsw_candidates[mem_id]
            union[mem_id] = (memory, fts_score, sem_score)

        for mem_id, (memory, sem_score) in hnsw_candidates.items():
            if mem_id not in union:
                # D95+ follow-up: if MemoryFileStore lost an embedding
                # (e.g. HNSW add failed silently), we could fall back to
                # computing cosine from the embedding_vec column on disk.
                # For T2 we accept sem_score=0.0 when the memory is only
                # in FTS.
                fts_score = 0.0
                union[mem_id] = (memory, fts_score, sem_score)

        hits: list[SearchHit] = []
        for mem_id, (memory, fts_score, sem_score) in union.items():
            # Suppress noise: HNSW oversample (top_k*4) surfaces every row
            # in a small corpus, and negative cosines clamp to 0. If *both*
            # dimensions contributed nothing, the hit is pure oversample
            # noise and should not appear in results. This matches pre-T2
            # behaviour where FTS-only results always had positive score.
            if fts_score <= 0.0 and sem_score <= 0.0:
                continue
            decay = _time_decay(memory.updated_at, now_ms)
            final_score = (
                self.w_fts * fts_score + self.w_sem * sem_score
            ) * decay
            hits.append(
                SearchHit(
                    memory=memory,
                    score=final_score,
                    fts_score=fts_score,
                    time_decay=decay,
                    semantic_score=sem_score,
                )
            )

        hits.sort(key=lambda h: h.score, reverse=True)
        return hits[:top_k]
