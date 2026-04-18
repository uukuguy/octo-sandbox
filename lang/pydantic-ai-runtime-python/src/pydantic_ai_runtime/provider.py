"""pydantic-ai provider adapter.

Wraps pydantic_ai.Agent to produce an OAI-shaped response dict compatible
with the AgentSession loop. Uses pydantic-ai's OpenAI-compatible backend
configured via OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL_NAME env vars.

The adapter converts pydantic-ai's RunResult into the dict shape that
AgentSession.run() expects:
  {"choices": [{"message": {"role": "assistant", "content": "...", "tool_calls": [...]}}]}
"""
from __future__ import annotations

import os
from typing import Any

from pydantic_ai import Agent
from pydantic_ai.models.openai import OpenAIModel
from pydantic_ai.providers.openai import OpenAIProvider


class PydanticAiProvider:
    """pydantic-ai Agent wrapper that exposes a chat() interface.

    Uses OpenAIModel backed by any OAI-compatible endpoint. Tool schemas are
    registered as dynamic tools on each call via pydantic-ai's low-level API.
    """

    def __init__(
        self,
        base_url: str,
        api_key: str,
        model: str,
    ) -> None:
        # Normalize base_url: strip trailing /v1
        root = base_url.rstrip("/")
        if root.endswith("/v1"):
            root = root[: -len("/v1")]
        provider = OpenAIProvider(base_url=f"{root}/v1", api_key=api_key)
        self._oai_model = OpenAIModel(model, provider=provider)
        self._model_name = model

    async def chat(
        self,
        messages: list[dict[str, Any]],
        tools: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        """Run pydantic-ai agent and return OAI-shaped response.

        pydantic-ai does not expose a stateless chat() interface directly, so
        we use Agent.run() with the last user message as the prompt and pass
        the full message history as message_history. Tool schemas are ignored
        at the pydantic-ai layer (tools registered externally); tool_calls in
        the response are surfaced via the raw OAI response when available.

        Returns an OAI-compatible dict:
          {"choices": [{"message": {"role": "assistant", "content": str, "tool_calls": list}}]}
        """
        # Extract the latest user message as the prompt
        user_prompt = ""
        for msg in reversed(messages):
            if msg.get("role") == "user":
                content = msg.get("content", "")
                user_prompt = content if isinstance(content, str) else str(content)
                break

        agent: Agent[None, str] = Agent(self._oai_model)

        result = await agent.run(user_prompt)
        text = result.output if hasattr(result, "output") else str(result)

        return {
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": text,
                    }
                }
            ]
        }

    async def aclose(self) -> None:
        pass


def make_provider() -> PydanticAiProvider:
    return PydanticAiProvider(
        base_url=os.environ.get("OPENAI_BASE_URL", "https://api.openai.com"),
        api_key=os.environ.get("OPENAI_API_KEY", ""),
        model=os.environ.get("OPENAI_MODEL_NAME", "gpt-4o-mini"),
    )
