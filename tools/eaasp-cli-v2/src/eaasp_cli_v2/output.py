"""Rich rendering helpers — tables / JSON / panels / error formatting."""

from __future__ import annotations

import json
from typing import Any

from rich.console import Console
from rich.json import JSON
from rich.panel import Panel
from rich.table import Table

from .client import CliError

_stdout = Console()
_stderr = Console(stderr=True)


def print_table(title: str, rows: list[Any], columns: list[str]) -> None:
    """Render a list of dict-like rows as a rich table.

    ``rows`` is tolerant — non-dict entries are coerced to ``{"value": repr(row)}``.
    Missing columns render as an empty cell so partial service payloads don't crash.
    """
    table = Table(title=title, show_lines=False, header_style="bold cyan")
    for col in columns:
        table.add_column(col, overflow="fold")

    if not rows:
        table.add_row(*["(empty)"] + [""] * (len(columns) - 1))
    else:
        for row in rows:
            if not isinstance(row, dict):
                row = {"value": str(row)}
            values = []
            for col in columns:
                raw = row.get(col, "")
                if isinstance(raw, (dict, list)):
                    values.append(json.dumps(raw, ensure_ascii=False))
                else:
                    values.append("" if raw is None else str(raw))
            table.add_row(*values)

    _stdout.print(table)


def print_json(obj: Any) -> None:
    """Pretty-print any JSON-serializable object."""
    try:
        _stdout.print(JSON(json.dumps(obj, ensure_ascii=False, default=str)))
    except Exception:  # pragma: no cover — defensive
        _stdout.print(repr(obj))


def print_panel(body: str, title: str, style: str = "cyan") -> None:
    _stdout.print(Panel(body, title=title, border_style=style))


def print_error(err: CliError) -> None:
    """Render a ``CliError`` to stderr with its detail body."""
    _stderr.print(f"[bold red]error[/bold red]: {err.message}")
    if err.detail is not None:
        try:
            _stderr.print(JSON(json.dumps(err.detail, ensure_ascii=False, default=str)))
        except Exception:  # pragma: no cover
            _stderr.print(repr(err.detail))
