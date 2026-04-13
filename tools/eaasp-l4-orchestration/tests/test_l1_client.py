"""Tests for L1RuntimeClient + dict↔proto conversion.

Uses mock gRPC channels — no real L1 runtime needed.
"""

from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from eaasp_l4_orchestration.l1_client import (
    L1RuntimeClient,
    L1RuntimeError,
    _dict_to_session_payload,
    _send_response_to_dict,
    create_l1_client,
)


# ── Proto conversion tests ──────────────────────────────────────────────────


class TestDictToSessionPayload:
    def test_minimal_payload(self):
        payload = _dict_to_session_payload({"session_id": "s1", "runtime_id": "r1"})
        assert payload.session_id == "s1"
        assert payload.runtime_id == "r1"
        assert payload.allow_trim_p5 is True
        assert payload.allow_trim_p4 is False

    def test_full_payload(self):
        d = {
            "session_id": "sess_abc",
            "user_id": "user1",
            "runtime_id": "grid-runtime",
            "created_at": "1700000000",
            "allow_trim_p5": True,
            "allow_trim_p4": False,
            "allow_trim_p3": False,
            "policy_context": {
                "hooks": [
                    {"hook_id": "h1", "hook_type": "PreToolUse", "action": "deny"}
                ],
                "org_unit": "eaasp-mvp",
                "policy_version": "v1",
                "deploy_timestamp": "2026-04-12",
                "quotas": {"tokens_per_min": "10000"},
            },
            "memory_refs": [
                {
                    "memory_id": "m1",
                    "memory_type": "calibration",
                    "relevance_score": 0.95,
                    "summary": "Transformer-001 calibration data",
                }
            ],
            "skill_instructions": {
                "skill_id": "threshold-calibration",
                "name": "Threshold Calibration",
                "content": "You are a calibration assistant...",
            },
            "user_preferences": {
                "user_id": "user1",
                "prefs": {"language": "zh"},
                "language": "zh-CN",
                "timezone": "Asia/Shanghai",
            },
        }
        payload = _dict_to_session_payload(d)
        assert payload.session_id == "sess_abc"
        assert payload.policy_context.org_unit == "eaasp-mvp"
        assert len(payload.policy_context.hooks) == 1
        assert payload.policy_context.hooks[0].hook_id == "h1"
        assert payload.policy_context.quotas["tokens_per_min"] == "10000"
        assert len(payload.memory_refs) == 1
        assert payload.memory_refs[0].memory_id == "m1"
        assert payload.memory_refs[0].relevance_score == pytest.approx(0.95)
        # content field should fall back from "summary" key
        assert payload.memory_refs[0].content == "Transformer-001 calibration data"
        assert payload.skill_instructions.skill_id == "threshold-calibration"
        assert payload.user_preferences.language == "zh-CN"
        assert payload.user_preferences.prefs["language"] == "zh"

    def test_empty_dict(self):
        payload = _dict_to_session_payload({})
        assert payload.session_id == ""
        assert len(payload.memory_refs) == 0


class TestSkillScopedHooksProtoMapping:
    """断点 1: Verify skill scoped hook fields map correctly to proto ScopedHook.

    The original bug was that L4 dict keys (name/type/scope/command/prompt)
    differ from proto field names (hook_id/hook_type/condition/action).
    _dict_to_session_payload must map them correctly.
    """

    def test_command_and_prompt_hooks(self):
        payload = _dict_to_session_payload(
            {
                "session_id": "s1",
                "skill_instructions": {
                    "skill_id": "test-skill",
                    "name": "Test Skill",
                    "content": "You are a test assistant.",
                    "frontmatter_hooks": [
                        {
                            "name": "block_write",
                            "type": "command",
                            "scope": "PreToolUse",
                            "command": "/path/to/hook.sh",
                        },
                        {
                            "name": "check_output",
                            "type": "prompt",
                            "scope": "PostToolUse",
                            "prompt": "Verify output has device_id",
                        },
                    ],
                    "dependencies": ["mcp:mock-scada"],
                },
            }
        )
        hooks = payload.skill_instructions.frontmatter_hooks

        assert len(hooks) == 2

        # Hook 1: command type — name→hook_id, type→hook_type, scope→condition, command→action
        assert hooks[0].hook_id == "block_write"
        assert hooks[0].hook_type == "command"
        assert hooks[0].condition == "PreToolUse"
        assert hooks[0].action == "/path/to/hook.sh"

        # Hook 2: prompt type — prompt→action
        assert hooks[1].hook_id == "check_output"
        assert hooks[1].hook_type == "prompt"
        assert hooks[1].condition == "PostToolUse"
        assert hooks[1].action == "Verify output has device_id"

        # Dependencies pass through
        assert list(payload.skill_instructions.dependencies) == ["mcp:mock-scada"]

        # Content (prose) preserved
        assert payload.skill_instructions.content == "You are a test assistant."

    def test_stop_hook_maps_correctly(self):
        """Stop scope must also map via the same path."""
        payload = _dict_to_session_payload(
            {
                "session_id": "s2",
                "skill_instructions": {
                    "skill_id": "s",
                    "name": "s",
                    "content": "",
                    "frontmatter_hooks": [
                        {
                            "name": "require_anchor",
                            "type": "command",
                            "scope": "Stop",
                            "command": "hooks/check_output_anchor.sh",
                        },
                    ],
                },
            }
        )
        hook = payload.skill_instructions.frontmatter_hooks[0]
        assert hook.hook_id == "require_anchor"
        assert hook.condition == "Stop"
        assert hook.action == "hooks/check_output_anchor.sh"

    def test_fallback_to_proto_field_names(self):
        """When dict already uses proto field names (hook_id/hook_type/condition/action),
        they should still work — this is the P1 policy_context path."""
        payload = _dict_to_session_payload(
            {
                "session_id": "s3",
                "skill_instructions": {
                    "skill_id": "s",
                    "name": "s",
                    "content": "",
                    "frontmatter_hooks": [
                        {
                            "hook_id": "proto_style",
                            "hook_type": "command",
                            "condition": "PreToolUse",
                            "action": "/bin/true",
                        },
                    ],
                },
            }
        )
        hook = payload.skill_instructions.frontmatter_hooks[0]
        assert hook.hook_id == "proto_style"
        assert hook.hook_type == "command"
        assert hook.condition == "PreToolUse"
        assert hook.action == "/bin/true"

    def test_empty_frontmatter_hooks(self):
        """No hooks should produce empty repeated field."""
        payload = _dict_to_session_payload(
            {
                "session_id": "s4",
                "skill_instructions": {
                    "skill_id": "s",
                    "name": "s",
                    "content": "prose",
                    "frontmatter_hooks": [],
                    "dependencies": [],
                },
            }
        )
        assert len(payload.skill_instructions.frontmatter_hooks) == 0
        assert list(payload.skill_instructions.dependencies) == []


class TestSendResponseToDict:
    def test_text_delta(self):
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        chunk = runtime_pb2.SendResponse(
            chunk_type="text_delta",
            content="Hello",
        )
        d = _send_response_to_dict(chunk)
        assert d["chunk_type"] == "text_delta"
        assert d["content"] == "Hello"
        assert "tool_name" not in d
        assert "is_error" not in d

    def test_tool_start(self):
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        chunk = runtime_pb2.SendResponse(
            chunk_type="tool_start",
            tool_name="scada_read_snapshot",
            tool_id="t1",
        )
        d = _send_response_to_dict(chunk)
        assert d["chunk_type"] == "tool_start"
        assert d["tool_name"] == "scada_read_snapshot"
        assert d["tool_id"] == "t1"


# ── Factory tests ───────────────────────────────────────────────────────────


class TestCreateL1Client:
    def test_known_runtime(self):
        client = create_l1_client("grid-runtime")
        assert client._runtime_id == "grid-runtime"
        assert "50051" in client._endpoint

    def test_env_override(self):
        with patch.dict("os.environ", {"EAASP_L1_GRID_RUNTIME_URL": "myhost:9999"}):
            client = create_l1_client("grid-runtime")
            assert client._endpoint == "myhost:9999"

    def test_unknown_runtime_raises(self):
        with pytest.raises(L1RuntimeError, match="no endpoint configured"):
            create_l1_client("nonexistent-runtime")


# ── Client method tests (mocked channel) ────────────────────────────────────


class TestL1RuntimeClientInitialize:
    @pytest.mark.asyncio
    async def test_initialize_success(self):
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        mock_stub.Initialize = AsyncMock(
            return_value=runtime_pb2.InitializeResponse(
                session_id="sess_123", runtime_id="grid-runtime"
            )
        )
        client._stub = mock_stub
        client._channel = MagicMock()

        result = await client.initialize(
            {"session_id": "sess_123", "runtime_id": "grid-runtime"}
        )
        assert result["session_id"] == "sess_123"
        assert result["runtime_id"] == "grid-runtime"
        mock_stub.Initialize.assert_awaited_once()

    @pytest.mark.asyncio
    async def test_initialize_grpc_error(self):
        import grpc

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        error = grpc.aio.AioRpcError(
            code=grpc.StatusCode.UNAVAILABLE,
            initial_metadata=grpc.aio.Metadata(),
            trailing_metadata=grpc.aio.Metadata(),
            details="Connection refused",
        )
        mock_stub.Initialize = AsyncMock(side_effect=error)
        client._stub = mock_stub
        client._channel = MagicMock()

        with pytest.raises(L1RuntimeError, match="UNAVAILABLE"):
            await client.initialize({"session_id": "s1"})


class TestL1RuntimeClientConnectMcp:
    """Tests for L1RuntimeClient.connect_mcp()."""

    @pytest.mark.asyncio
    async def test_connect_mcp_success(self):
        """connect_mcp sends correct proto and returns parsed response."""
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        mock_resp = runtime_pb2.ConnectMCPResponse(
            success=True,
            connected=["mock-scada", "eaasp-l2-memory"],
            failed=[],
        )
        mock_stub.ConnectMCP = AsyncMock(return_value=mock_resp)
        client._stub = mock_stub
        client._channel = MagicMock()

        result = await client.connect_mcp(
            session_id="sess-001",
            servers=[
                {
                    "name": "mock-scada",
                    "transport": "stdio",
                    "command": "mock-scada",
                    "args": ["--transport", "stdio"],
                },
                {
                    "name": "eaasp-l2-memory",
                    "transport": "stdio",
                    "command": "eaasp-l2-memory",
                },
            ],
        )
        assert result["success"] is True
        assert result["connected"] == ["mock-scada", "eaasp-l2-memory"]
        assert result["failed"] == []

        # Verify proto was constructed correctly.
        call_args = mock_stub.ConnectMCP.call_args
        req = call_args[0][0]
        assert req.session_id == "sess-001"
        assert len(req.servers) == 2
        assert req.servers[0].name == "mock-scada"
        assert req.servers[0].transport == "stdio"
        assert req.servers[0].command == "mock-scada"
        assert list(req.servers[0].args) == ["--transport", "stdio"]

    @pytest.mark.asyncio
    async def test_connect_mcp_partial_failure(self):
        """connect_mcp returns failed list when some servers fail."""
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        mock_resp = runtime_pb2.ConnectMCPResponse(
            success=False,
            connected=["mock-scada"],
            failed=["bad-server"],
        )
        mock_stub.ConnectMCP = AsyncMock(return_value=mock_resp)
        client._stub = mock_stub
        client._channel = MagicMock()

        result = await client.connect_mcp(
            session_id="sess-001",
            servers=[
                {"name": "mock-scada", "transport": "stdio", "command": "mock-scada"},
                {"name": "bad-server", "transport": "stdio", "command": "nonexistent"},
            ],
        )
        assert result["success"] is False
        assert "mock-scada" in result["connected"]
        assert "bad-server" in result["failed"]

    @pytest.mark.asyncio
    async def test_connect_mcp_grpc_unavailable(self):
        """connect_mcp raises L1RuntimeError on gRPC failure."""
        import grpc

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        error = grpc.aio.AioRpcError(
            code=grpc.StatusCode.UNAVAILABLE,
            initial_metadata=grpc.aio.Metadata(),
            trailing_metadata=grpc.aio.Metadata(),
            details="Connection refused",
        )
        mock_stub.ConnectMCP = AsyncMock(side_effect=error)
        client._stub = mock_stub
        client._channel = MagicMock()

        with pytest.raises(L1RuntimeError, match="ConnectMCP"):
            await client.connect_mcp("sess-001", [{"name": "x", "transport": "stdio"}])

    @pytest.mark.asyncio
    async def test_connect_mcp_sse_transport(self):
        """connect_mcp correctly builds SSE server config with url."""
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        mock_resp = runtime_pb2.ConnectMCPResponse(
            success=True,
            connected=["sse-server"],
            failed=[],
        )
        mock_stub.ConnectMCP = AsyncMock(return_value=mock_resp)
        client._stub = mock_stub
        client._channel = MagicMock()

        await client.connect_mcp(
            session_id="sess-001",
            servers=[
                {
                    "name": "sse-server",
                    "transport": "sse",
                    "url": "http://host.docker.internal:18090/sse",
                },
            ],
        )
        req = mock_stub.ConnectMCP.call_args[0][0]
        assert req.servers[0].transport == "sse"
        assert req.servers[0].url == "http://host.docker.internal:18090/sse"
        # command should be empty for SSE transport
        assert req.servers[0].command == ""

    @pytest.mark.asyncio
    async def test_connect_mcp_with_env(self):
        """connect_mcp correctly passes env vars to McpServerConfig."""
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        client = L1RuntimeClient("localhost:50051", "grid-runtime")
        mock_stub = MagicMock()
        mock_resp = runtime_pb2.ConnectMCPResponse(
            success=True, connected=["env-server"], failed=[],
        )
        mock_stub.ConnectMCP = AsyncMock(return_value=mock_resp)
        client._stub = mock_stub
        client._channel = MagicMock()

        await client.connect_mcp(
            session_id="sess-001",
            servers=[
                {
                    "name": "env-server",
                    "transport": "stdio",
                    "command": "my-server",
                    "env": {"API_KEY": "secret123", "LOG_LEVEL": "debug"},
                },
            ],
        )
        req = mock_stub.ConnectMCP.call_args[0][0]
        assert req.servers[0].env["API_KEY"] == "secret123"
        assert req.servers[0].env["LOG_LEVEL"] == "debug"


class TestL1RuntimeClientSend:
    @pytest.mark.asyncio
    async def test_send_streaming(self):
        from eaasp_l4_orchestration._proto.eaasp.runtime.v2 import runtime_pb2

        client = L1RuntimeClient("localhost:50051", "grid-runtime")

        # Create an async iterator mock for the stream.
        chunks = [
            runtime_pb2.SendResponse(chunk_type="text_delta", content="Hello "),
            runtime_pb2.SendResponse(chunk_type="text_delta", content="world"),
            runtime_pb2.SendResponse(chunk_type="done"),
        ]

        async def mock_stream(*args, **kwargs):
            for c in chunks:
                yield c

        mock_stub = MagicMock()
        mock_stub.Send = mock_stream
        client._stub = mock_stub
        client._channel = MagicMock()

        results = []
        async for chunk in client.send("sess_123", "test message"):
            results.append(chunk)

        assert len(results) == 3
        assert results[0]["chunk_type"] == "text_delta"
        assert results[0]["content"] == "Hello "
        assert results[1]["content"] == "world"
        assert results[2]["chunk_type"] == "done"
