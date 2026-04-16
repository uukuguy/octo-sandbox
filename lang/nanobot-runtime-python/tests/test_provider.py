"""W2.T2 provider tests — strict OAI subset, no OpenRouter-specific fields."""
from __future__ import annotations

import pytest

from nanobot_runtime.provider import OpenAICompatProvider


@pytest.fixture
def mock_oai_response():
    return {
        "choices": [
            {"message": {"role": "assistant", "content": "hi"}}
        ]
    }


async def test_provider_sends_standard_oai_payload(httpx_mock, mock_oai_response):
    httpx_mock.add_response(
        url="http://mock/v1/chat/completions",
        json=mock_oai_response,
    )
    provider = OpenAICompatProvider(
        base_url="http://mock",
        api_key="sk-test",
        model="gpt-4o-mini",
    )
    try:
        resp = await provider.chat([{"role": "user", "content": "hi"}])
    finally:
        await provider.aclose()

    assert resp["choices"][0]["message"]["content"] == "hi"

    # Verify payload shape — strict OAI subset only
    sent = httpx_mock.get_request()
    import json as _json
    body = _json.loads(sent.content)
    assert body["model"] == "gpt-4o-mini"
    assert body["messages"] == [{"role": "user", "content": "hi"}]
    assert "tools" not in body  # no tools passed this turn


async def test_provider_does_not_send_openrouter_specific_headers(httpx_mock, mock_oai_response):
    httpx_mock.add_response(
        url="http://mock/v1/chat/completions",
        json=mock_oai_response,
    )
    provider = OpenAICompatProvider(
        base_url="http://mock",
        api_key="sk-test",
        model="gpt-4o-mini",
    )
    try:
        await provider.chat([{"role": "user", "content": "hi"}])
    finally:
        await provider.aclose()

    sent = httpx_mock.get_request()
    # Explicit ban — these headers must NOT be present
    assert "HTTP-Referer" not in sent.headers
    assert "X-Title" not in sent.headers
    # Standard OAI headers must be present
    assert sent.headers["Authorization"] == "Bearer sk-test"
    assert sent.headers["Content-Type"] == "application/json"


async def test_provider_passes_tools_when_supplied(httpx_mock, mock_oai_response):
    httpx_mock.add_response(
        url="http://mock/v1/chat/completions",
        json=mock_oai_response,
    )
    provider = OpenAICompatProvider(
        base_url="http://mock",
        api_key="sk-test",
        model="gpt-4o-mini",
    )
    tools = [{"type": "function", "function": {"name": "foo", "parameters": {}}}]
    try:
        await provider.chat(
            messages=[{"role": "user", "content": "call foo"}],
            tools=tools,
        )
    finally:
        await provider.aclose()

    sent = httpx_mock.get_request()
    import json as _json
    body = _json.loads(sent.content)
    assert body["tools"] == tools
    # Still no provider routing field
    assert "provider" not in body
