"""SDK wrapper — encapsulates claude-agent-sdk interactions."""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import AsyncIterator

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    ResultMessage,
    TextBlock,
    ToolResultBlock,
    ToolUseBlock,
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
        cwd: str | None = None,
    ) -> AsyncIterator[ChunkEvent]:
        """Send a message and yield response chunks."""
        options = self._build_options(system_prompt, allowed_tools)
        if cwd:
            options.cwd = cwd
        # Output CLI stderr to our stderr for debugging
        import sys
        options.debug_stderr = sys.stderr

        try:
            async for message in query(prompt=prompt, options=options):
                if isinstance(message, AssistantMessage):
                    for block in message.content:
                        if isinstance(block, TextBlock):
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
                            yield ChunkEvent(
                                chunk_type="tool_result",
                                tool_id=block.tool_use_id,
                                content=(
                                    block.content
                                    if isinstance(block.content, str)
                                    else str(block.content)
                                ),
                                is_error=block.is_error or False,
                            )
                elif isinstance(message, ResultMessage):
                    yield ChunkEvent(chunk_type="done", content="")
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
