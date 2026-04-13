"""L4 平台拦截器 — 从现有 session_orchestrator 调用中自动提取事件。

ADR-V2-001: 拦截器从 L1 response chunks 中提取 PRE_TOOL_USE / POST_TOOL_USE /
STOP 等事件，零 L1 runtime 改造。

拦截点在 session_orchestrator 的 send_message / stream_message 方法中。
"""

from __future__ import annotations

from typing import Any

from .event_models import Event, EventMetadata


class EventInterceptor:
    """从 L1 response chunks 中提取 HookEventType 事件。"""

    def extract_from_chunk(
        self,
        session_id: str,
        chunk: dict[str, Any],
        *,
        runtime_id: str = "",
    ) -> Event | None:
        """检查 chunk 是否对应一个可提取的事件。

        Returns Event or None (chunk 不对应任何事件类型)。
        """
        chunk_type = chunk.get("chunk_type", "")

        if chunk_type == "tool_call_start":
            return Event(
                session_id=session_id,
                event_type="PRE_TOOL_USE",
                payload={
                    "tool_name": chunk.get("tool_name", ""),
                    "arguments": chunk.get("arguments", {}),
                },
                metadata=EventMetadata(
                    source=f"interceptor:{runtime_id}" if runtime_id else "interceptor"
                ),
            )

        if chunk_type == "tool_result":
            is_error = chunk.get("is_error", False)
            return Event(
                session_id=session_id,
                event_type="POST_TOOL_USE_FAILURE" if is_error else "POST_TOOL_USE",
                payload={
                    "tool_name": chunk.get("tool_name", ""),
                    "result": chunk.get("content", ""),
                    "is_error": is_error,
                },
                metadata=EventMetadata(
                    source=f"interceptor:{runtime_id}" if runtime_id else "interceptor"
                ),
            )

        if chunk_type == "done":
            return Event(
                session_id=session_id,
                event_type="STOP",
                payload={
                    "reason": "complete",
                    "response_text": chunk.get("response_text", ""),
                },
                metadata=EventMetadata(
                    source=f"interceptor:{runtime_id}" if runtime_id else "interceptor"
                ),
            )

        return None

    def create_session_start(
        self, session_id: str, runtime_id: str
    ) -> Event:
        """在 Initialize 成功后调用。"""
        return Event(
            session_id=session_id,
            event_type="SESSION_START",
            payload={"runtime_id": runtime_id},
            metadata=EventMetadata(source=f"interceptor:{runtime_id}"),
        )

    def create_session_end(self, session_id: str) -> Event:
        """在 close_session / Terminate 时调用。"""
        return Event(
            session_id=session_id,
            event_type="POST_SESSION_END",
            payload={},
            metadata=EventMetadata(source="interceptor:orchestrator"),
        )
