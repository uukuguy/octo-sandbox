"""Scaffold smoke tests for pydantic-ai-runtime (S3.T6)."""
from __future__ import annotations

import pytest


def test_service_importable():
    from pydantic_ai_runtime.service import PydanticAiRuntimeService
    svc = PydanticAiRuntimeService()
    assert svc._sessions == {}


def test_session_importable():
    from pydantic_ai_runtime.session import AgentSession, EventType
    assert EventType.CHUNK == "CHUNK"
    assert EventType.STOP == "STOP"


def test_provider_importable():
    from pydantic_ai_runtime.provider import PydanticAiProvider
    assert PydanticAiProvider is not None


def test_main_importable():
    from pydantic_ai_runtime import __main__
    assert hasattr(__main__, "main")
