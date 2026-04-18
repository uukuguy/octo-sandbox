"""FastAPI app exposing MCP tool REST facade + L4 context assembly endpoints."""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, HTTPException, Query
from pydantic import BaseModel, Field

from .anchors import AnchorStore
from .db import init_db
from .event_index import EventEmbeddingIndex
from .files import MemoryFileStore
from .index import HybridIndex
from .mcp_tools import MCP_TOOL_MANIFEST, McpToolDispatcher, ToolError


class SearchRequest(BaseModel):
    query: str
    top_k: int = Field(default=10, ge=1, le=100)
    scope: str | None = None
    category: str | None = None


class ToolInvokeRequest(BaseModel):
    args: dict[str, Any] = Field(default_factory=dict)


class IngestEventRequest(BaseModel):
    event_id: str
    payload_text: str


def create_app(db_path: str) -> FastAPI:
    @asynccontextmanager
    async def lifespan(_: FastAPI) -> AsyncIterator[None]:
        await init_db(db_path)
        yield

    app = FastAPI(
        title="EAASP L2 Memory Engine",
        version="0.1.0",
        description="Ring-2 MVP — evidence anchors + versioned memory files + hybrid retrieval",
        lifespan=lifespan,
    )

    anchors = AnchorStore(db_path)
    files = MemoryFileStore(db_path)
    index = HybridIndex(db_path)
    event_index = EventEmbeddingIndex(octo_root=str(__import__("pathlib").Path(db_path).parent))
    dispatcher = McpToolDispatcher(anchors, files, index)

    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    @app.get("/tools")
    async def list_tools() -> dict[str, Any]:
        return {"tools": [t.model_dump() for t in MCP_TOOL_MANIFEST]}

    @app.post("/tools/{name}/invoke")
    async def invoke_tool(name: str, body: ToolInvokeRequest) -> dict[str, Any]:
        try:
            return await dispatcher.invoke(name, body.args)
        except ToolError as exc:
            status = _error_status(exc.code)
            raise HTTPException(
                status_code=status,
                detail={"code": exc.code, "message": exc.message},
            ) from exc

    @app.post("/api/v1/memory/search")
    async def memory_search(body: SearchRequest) -> dict[str, Any]:
        hits = await index.search(
            query=body.query,
            top_k=body.top_k,
            scope=body.scope,
            category=body.category,
        )
        return {"hits": [h.model_dump() for h in hits]}

    @app.get("/api/v1/memory/anchors")
    async def get_anchors(event_id: str = Query(...)) -> dict[str, Any]:
        rows = await anchors.list_by_event(event_id)
        return {"anchors": [r.model_dump() for r in rows]}

    @app.post("/api/v1/events/ingest")
    async def ingest_event(body: IngestEventRequest) -> dict[str, Any]:
        """D78: dual-write ACP event payload into the event HNSW index."""
        await event_index.add(event_id=body.event_id, payload_text=body.payload_text)
        return {"event_id": body.event_id, "indexed": True}

    return app


def _error_status(code: str) -> int:
    return {
        "not_found": 404,
        "invalid_transition": 409,
        "missing_arg": 422,
        "invalid_arg": 422,
        "unknown_tool": 400,
    }.get(code, 400)
