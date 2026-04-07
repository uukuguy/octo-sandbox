"""EAASP L4 Session Manager — FastAPI application.

Four-plane architecture (§3):
  - Experience Plane: user-facing conversation API
  - Integration Plane: API gateway routing
  - Control Plane: admin session management
  - Persistence Plane: SQLite storage
"""

from __future__ import annotations

import argparse

from fastapi import FastAPI

from eaasp_session.clients.l3_client import L3GovernanceClient
from eaasp_session.planes.control import router as control_router
from eaasp_session.planes.experience import router as experience_router
from eaasp_session.planes.integration import router as integration_router
from eaasp_session.planes.persistence import PersistencePlane


def create_app(
    l3_url: str = "http://localhost:8083",
    db_path: str = ":memory:",
) -> FastAPI:
    """Create and configure the L4 FastAPI application."""
    app = FastAPI(
        title="EAASP L4 Session Manager",
        version="0.1.0",
        description="Human-agent collaboration with four-plane architecture",
    )

    # Persistence plane
    app.state.persistence = PersistencePlane(db_path=db_path)

    # L3 client
    app.state.l3_client = L3GovernanceClient(base_url=l3_url)

    # Register routers (planes)
    app.include_router(experience_router)   # /v1/conversations
    app.include_router(control_router)      # /v1/sessions (admin)
    app.include_router(integration_router)  # /health

    return app


app = create_app()


def main():
    """CLI entrypoint."""
    parser = argparse.ArgumentParser(description="EAASP L4 Session Manager")
    parser.add_argument("--port", type=int, default=8084)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--l3-url", default="http://localhost:8083")
    parser.add_argument("--db-path", default=":memory:")
    args = parser.parse_args()

    import uvicorn

    global app
    app = create_app(l3_url=args.l3_url, db_path=args.db_path)
    uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":
    main()
