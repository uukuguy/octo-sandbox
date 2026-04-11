"""Entry point — uvicorn + FastAPI. Defaults to port 18083."""

from __future__ import annotations

import os

import uvicorn

from .api import create_app

DEFAULT_DB_PATH = os.environ.get("EAASP_L3_DB_PATH", "./data/governance.db")
DEFAULT_PORT = int(os.environ.get("EAASP_L3_PORT", "18083"))
DEFAULT_HOST = os.environ.get("EAASP_L3_HOST", "127.0.0.1")

app = create_app(DEFAULT_DB_PATH)


def run() -> None:
    uvicorn.run(
        "eaasp_l3_governance.main:app",
        host=DEFAULT_HOST,
        port=DEFAULT_PORT,
        reload=False,
    )


if __name__ == "__main__":
    run()
