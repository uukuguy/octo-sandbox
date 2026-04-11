"""`eaasp memory` — L2 memory engine queries."""

from __future__ import annotations

from typing import Any, Optional

import typer

from . import main as _main
from .config import CliConfig
from .output import print_json, print_table

app = typer.Typer(help="L2 memory engine queries")


@app.command("search")
def search(
    query: str = typer.Argument(..., help="Hybrid FTS query"),
    top_k: int = typer.Option(10, "--top-k"),
    scope: Optional[str] = typer.Option(None, "--scope"),
    category: Optional[str] = typer.Option(None, "--category"),
) -> None:
    """Hybrid FTS search against L2 memory files."""
    cfg = CliConfig.from_env()
    body: dict[str, Any] = {"query": query, "top_k": top_k}
    if scope:
        body["scope"] = scope
    if category:
        body["category"] = category

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST", f"{cfg.l2_url}/api/v1/memory/search", json=body
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    hits: list[Any] = []
    if isinstance(result, dict):
        hits = result.get("hits", []) or []
    elif isinstance(result, list):
        hits = result
    print_table(
        "Memory hits",
        hits,
        ["memory_id", "scope", "category", "score"],
    )


@app.command("read")
def read(memory_id: str = typer.Argument(...)) -> None:
    """Read the latest version of a memory file by id."""
    cfg = CliConfig.from_env()

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST",
                f"{cfg.l2_url}/tools/memory_read/invoke",
                json={"args": {"memory_id": memory_id}},
            )
        finally:
            await client.aclose()

    print_json(_main.run_async(_do()))


@app.command("list")
def list_cmd(
    limit: int = typer.Option(20, "--limit"),
    scope: Optional[str] = typer.Option(None, "--scope"),
) -> None:
    """List latest versions of memory files."""
    cfg = CliConfig.from_env()
    args: dict[str, Any] = {"limit": limit}
    if scope:
        args["scope"] = scope

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST",
                f"{cfg.l2_url}/tools/memory_list/invoke",
                json={"args": args},
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    rows: list[Any] = []
    if isinstance(result, dict):
        rows = result.get("items") or result.get("memories") or result.get("files") or []
    elif isinstance(result, list):
        rows = result
    if rows:
        print_table(
            "Memory files",
            rows,
            ["memory_id", "scope", "category", "status"],
        )
    else:
        print_json(result)
