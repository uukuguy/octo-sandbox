"""Minimal OpenAI-compatible mock used by contract tests.

The contract suite drives runtimes against deterministic LLM output so
that protocol-level assertions (event ordering, envelope shape) are not
coupled to live model behaviour. This FastAPI app implements just enough
of the OpenAI ``/v1/chat/completions`` surface for an L1 runtime to
complete a turn: it accepts the request and returns a fixed assistant
message, optionally scripted to call a single tool once before stopping.

The app is instantiated by :func:`build_app` and hosted by tests via
``uvicorn`` on a loopback port. The runtime subprocess is pointed at the
mock via ``OPENAI_BASE_URL=http://127.0.0.1:<port>/v1``.

S0.T4 extension: a per-server scripted-turn counter lets tests request
"first call emits tool_use for <tool_name>, subsequent calls emit plain
text" so PreToolUse/PostToolUse/Stop hooks actually fire end-to-end
inside the real grid-runtime agent loop.
"""

from __future__ import annotations

import json
import threading
from typing import Any

from fastapi import FastAPI
from pydantic import BaseModel


class _ChatRequest(BaseModel):
    model: str
    messages: list[dict[str, Any]]
    tools: list[dict[str, Any]] | None = None
    stream: bool = False
    tool_choice: Any = None


def build_app(
    tool_script: list[dict[str, Any]] | None = None,
) -> FastAPI:
    """Return a FastAPI app implementing the minimum OpenAI surface.

    Args:
        tool_script: Optional ordered list of tool-call descriptors. Each
            entry dict must carry ``"tool_name"`` and ``"arguments"`` (a
            JSON-serializable dict). The Nth chat-completion request is
            answered with ``tool_calls=[{tool_script[N]}]`` and
            ``finish_reason="tool_calls"``. When the script is exhausted,
            subsequent requests fall back to the plain "mock response"
            terminal-stop reply. Pass ``None`` to disable scripting
            entirely (matches pre-S0.T4 behaviour).

    Endpoints:

    * ``POST /v1/chat/completions`` — scripted behaviour described above.
      ``stream=true`` is NOT implemented; the runtime should disable
      streaming against this mock.
    * ``GET  /health`` — liveness probe (always 200 ``{"status": "ok"}``).

    Returns:
        A :class:`fastapi.FastAPI` app ready to be served by uvicorn.
    """
    app = FastAPI(title="contract-harness-mock-openai")
    # Thread-safe per-app turn counter; uvicorn may dispatch concurrent
    # requests on its own workers. We rely on this counter to walk the
    # scripted tool-call sequence deterministically.
    counter_lock = threading.Lock()
    counter = {"n": 0}
    script = list(tool_script or [])

    @app.post("/v1/chat/completions")
    async def chat_completions(req: _ChatRequest) -> dict[str, Any]:
        with counter_lock:
            idx = counter["n"]
            counter["n"] += 1

        # Scripted path: emit a tool_calls response.
        if idx < len(script):
            entry = script[idx]
            tool_name = entry["tool_name"]
            args = entry.get("arguments", {})
            tool_id = entry.get("id", f"call_{idx}")
            return {
                "id": f"chatcmpl-mock-{idx}",
                "object": "chat.completion",
                "created": 0,
                "model": req.model,
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": None,
                            "tool_calls": [
                                {
                                    "id": tool_id,
                                    "type": "function",
                                    "function": {
                                        "name": tool_name,
                                        "arguments": json.dumps(args),
                                    },
                                }
                            ],
                        },
                        "finish_reason": "tool_calls",
                    }
                ],
                "usage": {
                    "prompt_tokens": 0,
                    "completion_tokens": 2,
                    "total_tokens": 2,
                },
            }

        # Fallback: deterministic terminal-stop response.
        return {
            "id": f"chatcmpl-mock-{idx}",
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
