"""集成平面 — API gateway routing (§3.2).

Routes requests between experience plane and L3 governance.
MVP: simple HTTP proxy. Future: event bus, CDC.
"""

from __future__ import annotations

from fastapi import APIRouter

router = APIRouter(tags=["integration"])


@router.get("/health")
async def health():
    return {"status": "ok", "service": "eaasp-session-manager", "version": "0.1.0"}
