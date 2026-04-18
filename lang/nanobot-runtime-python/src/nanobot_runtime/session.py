"""W2.T3 multi-turn agent loop with event emission.

Implements an async generator-based agent loop over an OpenAICompatProvider,
emitting typed AgentEvent objects for chunks, tool calls, tool results, hooks,
stops, and errors.

ADR-V2-006 envelope is used for PostToolUse hook dispatch (fail-open on
timeout/error, exit-2 → deny, other → allow).
"""
from __future__ import annotations

import asyncio
import json
import os
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, AsyncGenerator, Protocol, runtime_checkable

from nanobot_runtime.provider import OpenAICompatProvider

HOOK_TIMEOUT_SECS: float = 5.0


class EventType:
    CHUNK = "CHUNK"
    TOOL_CALL = "TOOL_CALL"
    TOOL_RESULT = "TOOL_RESULT"
    STOP = "STOP"
    ERROR = "ERROR"
    HOOK_FIRED = "HOOK_FIRED"


@dataclass
class AgentEvent:
    event_type: str
    content: str = ""
    tool_call_id: str = ""
    tool_name: str = ""
    tool_input: dict[str, Any] = field(default_factory=dict)
    is_error: bool = False
    hook_event: str = ""
    hook_decision: str = ""


@dataclass
class ToolCall:
    id: str
    name: str
    arguments: dict[str, Any]


@runtime_checkable
class ToolExecutor(Protocol):
    async def execute(self, tool_name: str, tool_input: dict[str, Any]) -> str:
        ...


class StubToolExecutor:
    """Default executor: echoes arguments back as JSON."""

    async def execute(self, tool_name: str, tool_input: dict[str, Any]) -> str:
        return json.dumps({"tool": tool_name, "args": tool_input})


async def _run_hook_subprocess(
    command: str,
    stdin_bytes: bytes,
    env: dict[str, str],
    timeout_secs: float,
) -> str:
    """Run a hook shell command with ADR-V2-006 envelope on stdin.

    Returns:
        "deny"  — exit code 2
        "allow" — exit code 0 / non-2, timeout (fail-open), or any exception
    """
    try:
        merged_env = {**os.environ, **env}
        proc = await asyncio.create_subprocess_shell(
            command,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=merged_env,
        )
        try:
            await asyncio.wait_for(proc.communicate(input=stdin_bytes), timeout=timeout_secs)
        except asyncio.TimeoutError:
            try:
                proc.kill()
                await proc.wait()
            except Exception:
                pass
            return "allow"
        if proc.returncode == 2:
            return "deny"
        return "allow"
    except Exception:
        return "allow"


class AgentSession:
    """Multi-turn agent loop over an OpenAICompatProvider.

    Args:
        provider: Async OAI-compatible provider.
        tools: Optional OAI tool schema list passed on every chat() call.
        post_tool_use_hooks: Shell commands dispatched after each tool result.
        stop_hooks: Shell commands dispatched at natural session termination
            (ADR-V2-006 Stop envelope). Exit-2 → inject system message + continue.
        session_id: Opaque session identifier (auto-generated if omitted).
        tool_executor: Handles actual tool execution. Defaults to StubToolExecutor.
        max_turns: Upper bound on agentic turns before emitting an ERROR event.
        required_tools: Ordered list of bare tool names that the skill workflow
            requires (from SKILL.md workflow.required_tools, prefix stripped).
            When all have been called at least once, a system message is injected
            to drive the LLM toward a final text answer (mirrors grid-runtime D87).
    """

    def __init__(
        self,
        provider: OpenAICompatProvider,
        tools: list[dict[str, Any]] | None = None,
        post_tool_use_hooks: list[str] | None = None,
        stop_hooks: list[str] | None = None,
        session_id: str | None = None,
        tool_executor: ToolExecutor | None = None,
        max_turns: int = 10,
        required_tools: list[str] | None = None,
    ) -> None:
        self.provider = provider
        self.tools = tools or []
        self.post_tool_use_hooks = post_tool_use_hooks or []
        self.stop_hooks = stop_hooks or []
        self.session_id = session_id or _new_session_id()
        self.tool_executor: ToolExecutor = tool_executor or StubToolExecutor()
        self.max_turns = max_turns
        self.required_tools: list[str] = required_tools or []
        self._messages: list[dict[str, Any]] = []
        self._called_tools: set[str] = set()
        self._workflow_completion_injected: bool = False

    def _check_workflow_complete(self) -> bool:
        """Return True if all required_tools have been called at least once."""
        if not self.required_tools:
            return False
        return all(t in self._called_tools for t in self.required_tools)

    async def run(self, user_message: str) -> AsyncGenerator[AgentEvent, None]:
        """Drive the agent loop for a single user turn.

        Yields AgentEvent objects. The generator terminates on STOP or ERROR.
        """
        self._messages.append({"role": "user", "content": user_message})

        for _ in range(self.max_turns):
            try:
                response = await self.provider.chat(
                    messages=self._messages,
                    tools=self.tools or None,
                )
            except Exception as exc:
                yield AgentEvent(
                    event_type=EventType.ERROR,
                    content=str(exc),
                    is_error=True,
                )
                return

            choice = response["choices"][0]
            message = choice["message"]

            if not message.get("tool_calls"):
                content = message.get("content") or ""
                if content:
                    yield AgentEvent(event_type=EventType.CHUNK, content=content)
                self._messages.append({"role": "assistant", "content": content})

                # Dispatch Stop hooks (ADR-V2-006); exit-2 → inject + continue loop.
                denied = False
                async for hook_ev in self._dispatch_stop_hooks(content):
                    yield hook_ev
                    if (
                        hook_ev.event_type == EventType.HOOK_FIRED
                        and hook_ev.hook_decision == "deny"
                    ):
                        self._messages.append({
                            "role": "system",
                            "content": "Stop hook denied completion. Please revise your response.",
                        })
                        denied = True
                        break

                if not denied:
                    yield AgentEvent(event_type=EventType.STOP, content=content)
                    return
                continue  # stop hook denied — re-enter loop for another turn

            # Tool call path
            self._messages.append(message)
            tool_results: list[dict[str, Any]] = []

            for raw_tc in message["tool_calls"]:
                tc_id: str = raw_tc["id"]
                tc_name: str = raw_tc["function"]["name"]
                tc_args: dict[str, Any] = json.loads(raw_tc["function"]["arguments"])

                yield AgentEvent(
                    event_type=EventType.TOOL_CALL,
                    tool_call_id=tc_id,
                    tool_name=tc_name,
                    tool_input=tc_args,
                    content=json.dumps(tc_args),
                )

                is_error = False
                try:
                    result_str = await self.tool_executor.execute(tc_name, tc_args)
                except Exception as exc:
                    result_str = str(exc)
                    is_error = True

                self._called_tools.add(tc_name)

                yield AgentEvent(
                    event_type=EventType.TOOL_RESULT,
                    tool_call_id=tc_id,
                    tool_name=tc_name,
                    content=result_str,
                    is_error=is_error,
                )

                async for hook_ev in self._dispatch_post_tool_use_hooks(
                    tc_name, tc_args, result_str, is_error
                ):
                    yield hook_ev

                tool_results.append(
                    {"role": "tool", "tool_call_id": tc_id, "content": result_str}
                )

            self._messages.extend(tool_results)

            # D87-parity: once all required_tools have been called, inject a
            # system message to drive the LLM toward a final text answer.
            # Only inject once to avoid repeated nudges polluting history.
            if (
                self.required_tools
                and not self._workflow_completion_injected
                and self._check_workflow_complete()
            ):
                self._workflow_completion_injected = True
                self._messages.append({
                    "role": "system",
                    "content": (
                        "You have completed all required workflow steps "
                        f"({', '.join(self.required_tools)}). "
                        "Now provide your final answer to the user in clear, "
                        "concise text. Do not call any more tools."
                    ),
                })
            # continue loop

        yield AgentEvent(
            event_type=EventType.ERROR,
            content=f"max_turns={self.max_turns} exceeded",
            is_error=True,
        )

    async def _dispatch_post_tool_use_hooks(
        self,
        tool_name: str,
        tool_input: dict[str, Any],
        tool_result: str,
        is_error: bool,
    ) -> AsyncGenerator[AgentEvent, None]:
        """Dispatch PostToolUse hooks per ADR-V2-006 envelope."""
        if not self.post_tool_use_hooks:
            return

        envelope = {
            "event": "PostToolUse",
            "session_id": self.session_id,
            "skill_id": "",
            "tool_name": tool_name,
            "tool_input": tool_input,
            "tool_result": tool_result,
            "is_error": is_error,
            "draft_memory_id": "",
            "evidence_anchor_id": "",
            "created_at": datetime.now(timezone.utc).isoformat(),
        }
        stdin_bytes = json.dumps(envelope).encode()
        env = {
            "GRID_SESSION_ID": self.session_id,
            "GRID_TOOL_NAME": tool_name,
            "GRID_SKILL_ID": "",
            "GRID_EVENT": "PostToolUse",
        }

        for command in self.post_tool_use_hooks:
            decision = await _run_hook_subprocess(command, stdin_bytes, env, HOOK_TIMEOUT_SECS)
            yield AgentEvent(
                event_type=EventType.HOOK_FIRED,
                hook_event="PostToolUse",
                hook_decision=decision,
                tool_name=tool_name,
            )


    async def _dispatch_stop_hooks(
        self,
        final_content: str,
    ) -> AsyncGenerator[AgentEvent, None]:
        """Dispatch Stop hooks per ADR-V2-006.

        Exit-2 → deny (caller re-enters loop with system injection).
        All other exits → allow (no re-entry needed).
        """
        if not self.stop_hooks:
            return

        envelope = {
            "event": "Stop",
            "session_id": self.session_id,
            "skill_id": "",
            "content": final_content,
            "created_at": datetime.now(timezone.utc).isoformat(),
        }
        stdin_bytes = json.dumps(envelope).encode()
        env = {
            "GRID_SESSION_ID": self.session_id,
            "GRID_SKILL_ID": "",
            "GRID_EVENT": "Stop",
        }

        for command in self.stop_hooks:
            decision = await _run_hook_subprocess(command, stdin_bytes, env, HOOK_TIMEOUT_SECS)
            yield AgentEvent(
                event_type=EventType.HOOK_FIRED,
                hook_event="Stop",
                hook_decision=decision,
            )
            if decision == "deny":
                return  # caller sees deny and injects system message


def _new_session_id() -> str:
    import uuid
    return str(uuid.uuid4())
