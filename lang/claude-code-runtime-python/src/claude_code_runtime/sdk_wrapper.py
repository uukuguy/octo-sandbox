"""SDK wrapper — encapsulates claude-agent-sdk interactions."""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import AsyncIterator

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    ResultMessage,
    TextBlock,
    ToolResultBlock,
    ToolUseBlock,
    UserMessage,
    query,
)

from .config import RuntimeConfig

logger = logging.getLogger(__name__)


@dataclass
class ChunkEvent:
    """Normalized response chunk from SDK."""

    chunk_type: str  # "text_delta" | "tool_start" | "tool_result" | "done" | "error"
    content: str = ""
    tool_name: str = ""
    tool_id: str = ""
    is_error: bool = False


def _tool_result_chunk(block: ToolResultBlock) -> ChunkEvent:
    """Map a ToolResultBlock to a normalized tool_result ChunkEvent.

    ToolResultBlock.content may be `str`, `list[dict]`, or `None` per the
    Anthropic SDK. We always surface a string so gRPC consumers get a
    stable `content` field; `None` is projected to empty string.
    `is_error` is `bool | None`; treat None as False.
    """
    if isinstance(block.content, str):
        content = block.content
    elif block.content is None:
        content = ""
    else:
        content = str(block.content)
    return ChunkEvent(
        chunk_type="tool_result",
        tool_id=block.tool_use_id,
        content=content,
        is_error=bool(block.is_error) if block.is_error is not None else False,
    )


class SdkWrapper:
    """Wraps claude-agent-sdk for use by the gRPC service."""

    def __init__(self, config: RuntimeConfig):
        self.config = config

    def _build_options(
        self,
        system_prompt: str | None = None,
        allowed_tools: list[str] | None = None,
    ) -> ClaudeAgentOptions:
        """Build ClaudeAgentOptions from config."""
        env: dict[str, str] = {}
        if self.config.anthropic_api_key:
            env["ANTHROPIC_API_KEY"] = self.config.anthropic_api_key
        if self.config.anthropic_base_url:
            env["ANTHROPIC_BASE_URL"] = self.config.anthropic_base_url

        # L1 Runtime isolation: CLAUDE_CODE_SIMPLE=1 triggers --bare mode
        # (skip hooks, LSP, plugin sync, attribution, auto-memory, CLAUDE.md).
        # This prevents the development environment from leaking into the
        # skill execution context. Runtime provides its own system_prompt,
        # MCP servers, and workspace directory.
        env["CLAUDE_CODE_SIMPLE"] = "1"

        opts = ClaudeAgentOptions(
            model=self.config.anthropic_model_name or None,
            max_turns=self.config.max_turns,
            permission_mode=self.config.permission_mode,
            env=env,
        )

        if system_prompt:
            opts.system_prompt = system_prompt
        if allowed_tools:
            opts.allowed_tools = allowed_tools

        return opts

    async def send_message(
        self,
        prompt: str,
        system_prompt: str | None = None,
        allowed_tools: list[str] | None = None,
        mcp_servers: dict | None = None,
        cwd: str | None = None,
    ) -> AsyncIterator[ChunkEvent]:
        """Send a message and yield response chunks."""
        options = self._build_options(system_prompt, allowed_tools)
        if mcp_servers:
            options.mcp_servers = mcp_servers
        if cwd:
            options.cwd = cwd
        # Output CLI stderr to our stderr for debugging
        import sys
        options.debug_stderr = sys.stderr

        # D85 (S1.T5): accumulate all assistant text blocks across the turn
        # so the terminal `done` chunk can carry the full final reply. This
        # mirrors the Rust-side `Completed(AgentLoopResult).final_messages`
        # extraction in grid-runtime's `event_to_chunk` mapper. Downstream
        # L4 STOP consumers (and telemetry) read response_text directly
        # from the `done` chunk's `content` field — no proto change required.
        # The accumulator is local to this generator instance, so concurrent
        # `send_message` calls stay naturally isolated.
        response_text_parts: list[str] = []

        try:
            async for message in query(prompt=prompt, options=options):
                if isinstance(message, AssistantMessage):
                    for block in message.content:
                        if isinstance(block, TextBlock):
                            response_text_parts.append(block.text)
                            yield ChunkEvent(
                                chunk_type="text_delta",
                                content=block.text,
                            )
                        elif isinstance(block, ToolUseBlock):
                            yield ChunkEvent(
                                chunk_type="tool_start",
                                tool_name=block.name,
                                tool_id=block.id,
                                content=str(block.input),
                            )
                        elif isinstance(block, ToolResultBlock):
                            # Belt-and-suspenders: some SDK flows surface
                            # tool_result blocks inside the assistant message.
                            yield _tool_result_chunk(block)
                elif isinstance(message, UserMessage):
                    # Anthropic SDK echoes tool results back as UserMessage
                    # content blocks (message_parser.py lines 74-81). Before
                    # D86 these blocks were silently dropped → downstream
                    # POST_TOOL_USE hooks never fired. UserMessage.content may
                    # also be a plain `str` for the original user prompt, in
                    # which case there is nothing to emit here.
                    if isinstance(message.content, list):
                        for block in message.content:
                            if isinstance(block, ToolResultBlock):
                                yield _tool_result_chunk(block)
                elif isinstance(message, ResultMessage):
                    # D85: surface the concatenated assistant text so STOP
                    # consumers see the actual final response, not "".
                    yield ChunkEvent(
                        chunk_type="done",
                        content="".join(response_text_parts),
                    )
        except Exception as e:
            # Log full exception details including stderr from CLI subprocess
            stderr_output = getattr(e, 'stderr', None) or getattr(e, 'error_output', None) or ''
            logger.error("SDK error: %s\nstderr: %s", e, stderr_output)
            import traceback
            logger.error("SDK traceback:\n%s", traceback.format_exc())
            yield ChunkEvent(
                chunk_type="error",
                content=str(e),
                is_error=True,
            )
