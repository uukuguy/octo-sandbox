"""Multi-turn agent loop using pydantic-ai provider.

Reuses the same AgentSession event model as nanobot-runtime but drives
pydantic-ai's Agent instead of a raw OAI HTTP call.
"""
from __future__ import annotations

import asyncio
import json
import os
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, AsyncGenerator

from pydantic_ai_runtime.provider import PydanticAiProvider

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


async def _run_hook_subprocess(
    command: str,
    stdin_bytes: bytes,
    env: dict[str, str],
    timeout_secs: float,
) -> str:
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
    def __init__(
        self,
        provider: PydanticAiProvider,
        tools: list[dict[str, Any]] | None = None,
        stop_hooks: list[str] | None = None,
        session_id: str | None = None,
        max_turns: int = 10,
    ) -> None:
        self.provider = provider
        self.tools = tools or []
        self.stop_hooks = stop_hooks or []
        self.session_id = session_id or _new_session_id()
        self.max_turns = max_turns
        self._messages: list[dict[str, Any]] = []

    async def run(self, user_message: str) -> AsyncGenerator[AgentEvent, None]:
        self._messages.append({"role": "user", "content": user_message})

        for _ in range(self.max_turns):
            try:
                response = await self.provider.chat(
                    messages=self._messages,
                    tools=self.tools or None,
                )
            except Exception as exc:
                yield AgentEvent(event_type=EventType.ERROR, content=str(exc), is_error=True)
                return

            choice = response["choices"][0]
            message = choice["message"]

            if not message.get("tool_calls"):
                content = message.get("content") or ""
                if content:
                    yield AgentEvent(event_type=EventType.CHUNK, content=content)
                self._messages.append({"role": "assistant", "content": content})

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
                continue

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

                result_str = json.dumps({"tool": tc_name, "args": tc_args})
                yield AgentEvent(
                    event_type=EventType.TOOL_RESULT,
                    tool_call_id=tc_id,
                    tool_name=tc_name,
                    content=result_str,
                )
                tool_results.append({"role": "tool", "tool_call_id": tc_id, "content": result_str})

            self._messages.extend(tool_results)

        yield AgentEvent(
            event_type=EventType.ERROR,
            content=f"max_turns={self.max_turns} exceeded",
            is_error=True,
        )

    async def _dispatch_stop_hooks(
        self, final_content: str
    ) -> AsyncGenerator[AgentEvent, None]:
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
            "GRID_TOOL_NAME": "",
        }
        for command in self.stop_hooks:
            decision = await _run_hook_subprocess(command, stdin_bytes, env, HOOK_TIMEOUT_SECS)
            yield AgentEvent(
                event_type=EventType.HOOK_FIRED,
                hook_event="Stop",
                hook_decision=decision,
            )
            if decision == "deny":
                return


def _new_session_id() -> str:
    import uuid
    return str(uuid.uuid4())
