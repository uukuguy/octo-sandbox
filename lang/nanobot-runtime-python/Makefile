# nanobot-runtime-python local Makefile.
# Mirrors the `uv`-based workflow used by lang/claude-code-runtime-python
# (repo-root Makefile targets: claude-runtime-setup / -test / -start).
# Phase 2.5 W2.T1: scaffold only; W2.T4/W2.T5 will add proto gen and real server.

PY ?= python3
VENV ?= .venv

.PHONY: setup test run clean

# Preferred path: uv sync --extra dev (matches sibling runtime).
# Fallback: stdlib venv + pip install -e ".[dev]" for environments without uv.
setup:
	@if command -v uv >/dev/null 2>&1; then \
		uv sync --extra dev; \
	else \
		$(PY) -m venv $(VENV) && $(VENV)/bin/pip install -e ".[dev]"; \
	fi

test:
	@if command -v uv >/dev/null 2>&1; then \
		uv run pytest tests/ -v; \
	else \
		$(VENV)/bin/python -m pytest tests/ -v; \
	fi

run:
	@if command -v uv >/dev/null 2>&1; then \
		uv run python -m nanobot_runtime; \
	else \
		$(VENV)/bin/python -m nanobot_runtime; \
	fi

clean:
	rm -rf $(VENV) .venv .pytest_cache .ruff_cache
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
