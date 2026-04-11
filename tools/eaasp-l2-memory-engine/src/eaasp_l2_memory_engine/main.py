"""Entry point — uvicorn + FastAPI. Defaults to port 18085."""

from __future__ import annotations

import os

import uvicorn

from .api import create_app

DEFAULT_DB_PATH = os.environ.get("EAASP_L2_DB_PATH") or os.environ.get(
    "EAASP_MEMORY_DB", "./data/memory.db"
)
# Primary: EAASP_L2_PORT. Legacy alias: EAASP_MEMORY_PORT (kept one release
# cycle to avoid breaking existing launch scripts — remove after S4).
DEFAULT_PORT = int(
    os.environ.get("EAASP_L2_PORT") or os.environ.get("EAASP_MEMORY_PORT", "18085")
)
DEFAULT_HOST = os.environ.get("EAASP_L2_HOST") or os.environ.get(
    "EAASP_MEMORY_HOST", "127.0.0.1"
)

app = create_app(DEFAULT_DB_PATH)


def run() -> None:
    uvicorn.run(
        "eaasp_l2_memory_engine.main:app",
        host=DEFAULT_HOST,
        port=DEFAULT_PORT,
        reload=False,
    )


if __name__ == "__main__":
    run()
