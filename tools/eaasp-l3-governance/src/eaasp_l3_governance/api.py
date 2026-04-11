"""FastAPI app exposing L3 governance REST surface.

Endpoints (MVP scope):

- ``GET  /health``                                     — liveness probe
- ``PUT  /v1/policies/managed-hooks``                  — deploy managed-settings
- ``GET  /v1/policies/versions``                       — newest-first version list
- ``PUT  /v1/policies/{hook_id}/mode``                 — enforce/shadow switch
- ``POST /v1/telemetry/events``                        — async telemetry ingest
- ``GET  /v1/telemetry/events``                        — telemetry query
- ``POST /v1/sessions/{session_id}/validate``          — three-way handshake stub
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI, HTTPException, Query
from pydantic import BaseModel, Field, ValidationError

from .audit import AuditStore, TelemetryEventIn
from .db import init_db
from .managed_settings import ManagedSettings, ensure_mode, hook_matches
from .policy_engine import PolicyEngine


class ModeSwitchRequest(BaseModel):
    mode: str = Field(..., description="enforce | shadow")


class SessionValidateRequest(BaseModel):
    agent_id: str | None = None
    skill_id: str | None = None
    runtime_tier: str | None = None


def create_app(db_path: str) -> FastAPI:
    @asynccontextmanager
    async def lifespan(_: FastAPI) -> AsyncIterator[None]:
        await init_db(db_path)
        yield

    app = FastAPI(
        title="EAASP L3 Governance",
        version="0.1.0",
        description="Thin L3 governance plane — Policy deployment + Telemetry ingest + Session validate (MVP)",
        lifespan=lifespan,
    )

    policy = PolicyEngine(db_path)
    audit = AuditStore(db_path)

    # ─── Health ───────────────────────────────────────────────────────────
    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    # ─── Contract 1: Policy Deployment ────────────────────────────────────
    @app.put("/v1/policies/managed-hooks")
    async def deploy_managed_hooks(payload: dict[str, Any]) -> dict[str, Any]:
        """Accept a pre-compiled managed-settings.json body and persist it."""
        try:
            settings = ManagedSettings.model_validate(payload)
        except ValidationError as exc:
            # Sanitize errors: Pydantic v2 can embed raw Python exceptions in
            # ``ctx`` (e.g. the ValueError raised by our unique-hook-id
            # validator), which JSONResponse cannot serialize. Convert each
            # error dict to a JSON-safe projection.
            raise HTTPException(
                status_code=422, detail=_sanitize_errors(exc.errors())
            ) from exc
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        result = await policy.deploy(settings)
        return result.model_dump()

    @app.get("/v1/policies/versions")
    async def list_policy_versions(
        limit: int = Query(default=100, ge=1, le=500),
    ) -> dict[str, Any]:
        versions = await policy.list_versions(limit=limit)
        return {"versions": [v.model_dump() for v in versions]}

    @app.put("/v1/policies/{hook_id}/mode")
    async def switch_hook_mode(
        hook_id: str, body: ModeSwitchRequest
    ) -> dict[str, Any]:
        try:
            ensure_mode(body.mode)
        except ValueError as exc:
            raise HTTPException(status_code=422, detail=str(exc)) from exc
        override = await policy.switch_mode(hook_id, body.mode)
        return override.model_dump()

    # ─── Contract 4: Telemetry Ingest ─────────────────────────────────────
    @app.post("/v1/telemetry/events")
    async def ingest_telemetry(event: TelemetryEventIn) -> dict[str, Any]:
        result = await audit.ingest(event)
        return {
            "event_id": result.event_id,
            "received_at": result.received_at,
        }

    @app.get("/v1/telemetry/events")
    async def query_telemetry(
        session_id: str | None = Query(default=None),
        since: str | None = Query(default=None),
        limit: int = Query(default=100, ge=1, le=500),
    ) -> dict[str, Any]:
        events = await audit.query(session_id=session_id, since=since, limit=limit)
        return {"events": [e.model_dump() for e in events]}

    # ─── Contract 5 (partial): Session validate ───────────────────────────
    @app.post("/v1/sessions/{session_id}/validate")
    async def validate_session(
        session_id: str, body: SessionValidateRequest
    ) -> dict[str, Any]:
        latest = await policy.latest_version()
        if latest is None:
            raise HTTPException(
                status_code=404,
                detail={
                    "code": "no_policy",
                    "message": "no managed-settings version has been deployed yet",
                },
            )

        hooks_to_attach: list[dict[str, Any]] = []
        for hook in latest.payload.get("hooks", []):
            if not hook_matches(hook, body.agent_id, body.skill_id):
                continue
            # Apply per-hook mode override if one has been set via
            # PUT /v1/policies/{hook_id}/mode (floats above version rows).
            override = await policy.get_mode_override(hook["hook_id"])
            merged = dict(hook)
            if override is not None:
                merged["mode"] = override.mode
            hooks_to_attach.append(merged)

        return {
            "session_id": session_id,
            "hooks_to_attach": hooks_to_attach,
            "managed_settings_version": latest.version,
            "validated_at": latest.created_at,
            "runtime_tier": body.runtime_tier,
        }

    return app


def _sanitize_errors(errors: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Strip non-JSON-serializable objects from Pydantic error dicts."""
    clean: list[dict[str, Any]] = []
    for err in errors:
        safe: dict[str, Any] = {}
        for key, value in err.items():
            if key == "ctx" and isinstance(value, dict):
                safe[key] = {
                    ctx_key: (
                        str(ctx_val)
                        if isinstance(ctx_val, BaseException)
                        else ctx_val
                    )
                    for ctx_key, ctx_val in value.items()
                }
            elif isinstance(value, BaseException):
                safe[key] = str(value)
            else:
                safe[key] = value
        clean.append(safe)
    return clean
