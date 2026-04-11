"""Entry point — uvicorn + FastAPI. Defaults to port 8084 per MVP spec §3.3."""

from __future__ import annotations

import os

import uvicorn

from .api import create_app

DEFAULT_DB_PATH = os.environ.get("EAASP_L4_DB_PATH", "./data/orchestration.db")
DEFAULT_PORT = int(os.environ.get("EAASP_L4_PORT", "8084"))
DEFAULT_HOST = os.environ.get("EAASP_L4_HOST", "127.0.0.1")

app = create_app(DEFAULT_DB_PATH)


def run() -> None:
    uvicorn.run(
        "eaasp_l4_orchestration.main:app",
        host=DEFAULT_HOST,
        port=DEFAULT_PORT,
        reload=False,
    )


if __name__ == "__main__":
    run()
