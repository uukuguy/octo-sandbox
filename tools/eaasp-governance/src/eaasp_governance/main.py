"""EAASP L3 Governance Service — FastAPI application.

Serves 5 API contracts on port 8083:
  1. Policy Deploy   — PUT/GET /v1/policies
  2. Intent Gateway   — POST /v1/intents
  3. Skill Lifecycle  — GET /v1/skills/{id}/governance
  4. Telemetry Ingest — POST/GET /v1/telemetry
  5. Session Control  — POST/GET/DELETE /v1/sessions
"""

from __future__ import annotations

import argparse
from pathlib import Path

from fastapi import FastAPI

from eaasp_governance.api.intent_gateway import init_resolver, router as intent_router
from eaasp_governance.api.policy_deploy import router as policy_router
from eaasp_governance.api.session_control import router as session_router
from eaasp_governance.api.skill_lifecycle import router as skill_router
from eaasp_governance.api.telemetry_ingest import router as telemetry_router
from eaasp_governance.clients.l2_registry import L2RegistryClient
from eaasp_governance.runtime_pool import RuntimePool
from eaasp_governance.session_state import GovernanceSession


def create_app(
    l2_url: str = "http://localhost:8081",
    runtimes_config: str | None = None,
) -> FastAPI:
    """Create and configure the FastAPI application."""
    app = FastAPI(
        title="EAASP L3 Governance Service",
        version="0.1.0",
        description="Enterprise agent governance — policy, intent, session, telemetry",
    )

    # In-memory stores (MVP)
    app.state.policy_store = {}       # str → list[dict] (version history, D10)
    app.state.telemetry_store = {}
    app.state.sessions = {}
    app.state.l1_clients = {}

    # Intent resolver (BH-D5)
    intents_config = Path(__file__).parent.parent.parent / "config" / "intents.yaml"
    app.state.intent_resolver = init_resolver(
        config_path=str(intents_config) if intents_config.exists() else None
    )

    # L2 client
    app.state.l2_client = L2RegistryClient(base_url=l2_url)

    # Runtime pool
    pool = RuntimePool()
    config_path = runtimes_config or str(
        Path(__file__).parent.parent.parent / "config" / "runtimes.yaml"
    )
    pool.load_config(config_path)
    app.state.runtime_pool = pool

    # Register routers
    app.include_router(policy_router)
    app.include_router(intent_router)
    app.include_router(skill_router)
    app.include_router(telemetry_router)
    app.include_router(session_router)

    @app.get("/health")
    async def health():
        return {"status": "ok", "service": "eaasp-governance", "version": "0.1.0"}

    return app


app = create_app()


def main():
    """CLI entrypoint."""
    parser = argparse.ArgumentParser(description="EAASP L3 Governance Service")
    parser.add_argument("--port", type=int, default=8083)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--l2-url", default="http://localhost:8081")
    parser.add_argument("--runtimes-config", default=None)
    args = parser.parse_args()

    import uvicorn

    global app
    app = create_app(l2_url=args.l2_url, runtimes_config=args.runtimes_config)
    uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":
    main()
