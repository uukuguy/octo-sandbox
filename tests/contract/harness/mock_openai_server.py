"""Minimal OpenAI-compatible mock used by contract tests.

The contract suite drives runtimes against deterministic LLM output so
that protocol-level assertions (event ordering, envelope shape) are not
coupled to live model behaviour. This FastAPI app implements just enough
of the OpenAI ``/v1/chat/completions`` surface for an L1 runtime to
complete a turn: it accepts the request and returns a fixed assistant
message.

The app is instantiated by :func:`build_app` and hosted by tests via
``uvicorn`` on a loopback port. The runtime subprocess is pointed at the
mock via ``OPENAI_BASE_URL=http://127.0.0.1:<port>/v1``.
"""

from __future__ import annotations

from typing import Any

from fastapi import FastAPI
from pydantic import BaseModel


class _ChatRequest(BaseModel):
    model: str
    messages: list[dict[str, Any]]
    tools: list[dict[str, Any]] | None = None
    stream: bool = False


def build_app() -> FastAPI:
    """Return a FastAPI app implementing the minimum OpenAI surface.

    Currently supported endpoints:

    * ``POST /v1/chat/completions`` — returns a single ``stop`` choice with
      ``"mock response"`` as assistant content. ``stream=true`` is NOT yet
      implemented; callers that need SSE should extend this endpoint.

    Returns:
        A :class:`fastapi.FastAPI` app ready to be served by uvicorn.
    """
    app = FastAPI(title="contract-harness-mock-openai")

    @app.post("/v1/chat/completions")
    async def chat_completions(req: _ChatRequest) -> dict[str, Any]:
        # Deterministic, terminal-stop response. No tool calls — contract
        # tests that need tool calls must extend this endpoint (deferred
        # to later S0 tasks if/when needed).
        return {
            "id": "chatcmpl-mock",
            "object": "chat.completion",
            "created": 0,
            "model": req.model,
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "mock response",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 2,
                "total_tokens": 2,
            },
        }

    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    return app
