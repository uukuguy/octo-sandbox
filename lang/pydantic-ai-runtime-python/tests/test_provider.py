"""Provider tests — PydanticAiProvider construction, base_url normalization,
env-driven factory, and OAI-shape contract via monkeypatched Agent.run.

No live LLM calls. Uses unittest.mock.patch.object on pydantic_ai.Agent.run
to exercise the wrapper's chat() dict-shape contract.
"""
from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest
from pydantic_ai import Agent

from pydantic_ai_runtime.provider import PydanticAiProvider, make_provider


# ---------------------------------------------------------------------------
# Construction + base_url normalization
# ---------------------------------------------------------------------------

def test_provider_constructs_with_plain_base_url():
    """Happy path — plain https host, no /v1 suffix, constructs cleanly."""
    prov = PydanticAiProvider(
        base_url="https://api.openai.com",
        api_key="sk-test",
        model="gpt-4o-mini",
    )
    assert prov._model_name == "gpt-4o-mini"
    # Underlying pydantic-ai OpenAIProvider base_url must terminate in /v1/
    assert prov._oai_model._provider.base_url.rstrip("/").endswith("/v1")


def test_provider_strips_trailing_v1_to_avoid_double_suffix():
    """Passing base_url='https://x.y/v1' must NOT result in '/v1/v1' endpoint."""
    prov = PydanticAiProvider(
        base_url="https://api.openai.com/v1",
        api_key="sk-test",
        model="gpt-4o-mini",
    )
    # Exactly one /v1 in the normalized URL (pydantic-ai may add trailing slash)
    base = prov._oai_model._provider.base_url
    assert base.count("/v1") == 1, f"Expected exactly one /v1, got {base!r}"


def test_provider_strips_trailing_slash_then_v1():
    """Trailing slash after /v1 must also be stripped cleanly."""
    prov = PydanticAiProvider(
        base_url="https://api.openai.com/v1/",
        api_key="sk-test",
        model="gpt-4o-mini",
    )
    base = prov._oai_model._provider.base_url
    assert base.count("/v1") == 1


def test_provider_preserves_non_v1_path_segments():
    """base_url with other path segments (e.g., custom gateway) must not munge them."""
    prov = PydanticAiProvider(
        base_url="https://gateway.example.com/api",
        api_key="sk-test",
        model="some-model",
    )
    # /api should be preserved, and /v1 appended by the provider
    base = prov._oai_model._provider.base_url
    assert "/api" in base
    assert base.count("/v1") == 1


# ---------------------------------------------------------------------------
# make_provider() env-driven factory
# ---------------------------------------------------------------------------

def test_make_provider_uses_env_vars(monkeypatch):
    """Factory reads OPENAI_BASE_URL / OPENAI_API_KEY / OPENAI_MODEL_NAME."""
    monkeypatch.setenv("OPENAI_BASE_URL", "https://openrouter.ai/api")
    monkeypatch.setenv("OPENAI_API_KEY", "sk-env-test")
    monkeypatch.setenv("OPENAI_MODEL_NAME", "anthropic/claude-sonnet-4")

    prov = make_provider()
    assert prov._model_name == "anthropic/claude-sonnet-4"
    # base_url should reflect env override
    assert "openrouter.ai" in prov._oai_model._provider.base_url


def test_make_provider_defaults_when_env_missing(monkeypatch):
    """Factory falls back to OPENAI defaults when env vars absent."""
    monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.delenv("OPENAI_MODEL_NAME", raising=False)

    prov = make_provider()
    assert prov._model_name == "gpt-4o-mini"
    # Default base goes to api.openai.com
    assert "api.openai.com" in prov._oai_model._provider.base_url


# ---------------------------------------------------------------------------
# chat() OAI-shape contract via monkeypatched Agent.run
# ---------------------------------------------------------------------------

async def test_chat_returns_oai_shape_dict():
    """chat() must return {'choices': [{'message': {'role': 'assistant', 'content': str}}]}."""
    fake_result = MagicMock()
    fake_result.output = "hello from pydantic-ai"

    async def fake_run(self, prompt, **kwargs):  # noqa: ARG001
        return fake_result

    with patch.object(Agent, "run", fake_run):
        prov = PydanticAiProvider(
            base_url="https://mock",
            api_key="sk-test",
            model="fake-model",
        )
        resp = await prov.chat([{"role": "user", "content": "hi"}])

    assert "choices" in resp
    assert len(resp["choices"]) == 1
    msg = resp["choices"][0]["message"]
    assert msg["role"] == "assistant"
    assert msg["content"] == "hello from pydantic-ai"


async def test_chat_uses_last_user_message_as_prompt():
    """chat() must extract the latest user message for Agent.run prompt."""
    captured_prompt: dict = {}

    fake_result = MagicMock()
    fake_result.output = "acknowledged"

    async def fake_run(self, prompt, **kwargs):  # noqa: ARG001
        captured_prompt["value"] = prompt
        return fake_result

    with patch.object(Agent, "run", fake_run):
        prov = PydanticAiProvider(
            base_url="https://mock",
            api_key="sk-test",
            model="fake-model",
        )
        await prov.chat([
            {"role": "system", "content": "you are helpful"},
            {"role": "user", "content": "first question"},
            {"role": "assistant", "content": "first answer"},
            {"role": "user", "content": "SECOND QUESTION"},
        ])

    # Latest user message wins (reversed traversal)
    assert captured_prompt["value"] == "SECOND QUESTION"


async def test_chat_surfaces_agent_run_exceptions():
    """Exceptions from Agent.run propagate so AgentSession can emit ERROR."""
    class BoomError(RuntimeError):
        pass

    async def boom_run(self, prompt, **kwargs):  # noqa: ARG001
        raise BoomError("simulated provider failure")

    with patch.object(Agent, "run", boom_run):
        prov = PydanticAiProvider(
            base_url="https://mock",
            api_key="sk-test",
            model="fake-model",
        )
        with pytest.raises(BoomError, match="simulated provider failure"):
            await prov.chat([{"role": "user", "content": "trigger"}])


async def test_aclose_is_idempotent():
    """aclose() must not raise even when called multiple times."""
    prov = PydanticAiProvider(
        base_url="https://mock",
        api_key="sk-test",
        model="fake-model",
    )
    await prov.aclose()
    await prov.aclose()  # second call must also be fine
