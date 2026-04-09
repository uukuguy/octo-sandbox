"""HermesAdapter — wraps hermes-agent AIAgent for EAASP RuntimeContract."""

import json
import logging
import queue
import threading
from typing import Generator

from hermes_runtime.config import HermesRuntimeConfig
from hermes_runtime.governance_plugin import set_session_id

logger = logging.getLogger(__name__)


class HermesAdapter:
    """Manages one AIAgent instance per session, adapts sync→async streaming."""

    def __init__(self, config: HermesRuntimeConfig):
        self._config = config
        self._agents: dict[str, object] = {}  # session_id → AIAgent

    def create_agent(self, session_id: str, **session_kwargs) -> None:
        """Create and store an AIAgent for this session."""
        from run_agent import AIAgent

        enabled_toolsets = None
        if self._config.hermes_toolsets:
            enabled_toolsets = [
                t.strip() for t in self._config.hermes_toolsets.split(",") if t.strip()
            ]

        agent = AIAgent(
            base_url=self._config.hermes_base_url or None,
            api_key=self._config.hermes_api_key or None,
            provider=self._config.hermes_provider or None,
            model=self._config.hermes_model,
            max_iterations=self._config.hermes_max_iterations,
            enabled_toolsets=enabled_toolsets,
            session_id=session_id,
            quiet_mode=True,
            skip_context_files=True,
            skip_memory=True,
        )
        self._agents[session_id] = agent

    def get_agent(self, session_id: str):
        return self._agents.get(session_id)

    def remove_agent(self, session_id: str):
        return self._agents.pop(session_id, None)

    def send_message(
        self,
        session_id: str,
        content: str,
        conversation_history: list | None = None,
    ) -> Generator[dict, None, None]:
        """Run conversation synchronously, yield chunks via thread bridge.

        hermes AIAgent.run_conversation() is synchronous and blocking.
        We run it in a background thread and bridge results via a queue.
        """
        agent = self._agents.get(session_id)
        if agent is None:
            yield {"chunk_type": "error", "content": f"No agent for session {session_id}"}
            return

        set_session_id(session_id)

        result_queue: queue.Queue = queue.Queue()

        def _stream_delta(text: str):
            result_queue.put({"chunk_type": "text_delta", "content": text})

        def _tool_start(tool_name: str, args_preview: str):
            result_queue.put({
                "chunk_type": "tool_start",
                "content": args_preview,
                "tool_name": tool_name,
            })

        def _tool_complete(tool_name: str, result_preview: str):
            result_queue.put({
                "chunk_type": "tool_result",
                "content": result_preview,
                "tool_name": tool_name,
            })

        agent.stream_delta_callback = _stream_delta
        agent.tool_start_callback = _tool_start
        agent.tool_complete_callback = _tool_complete

        def _run():
            try:
                result = agent.run_conversation(
                    user_message=content,
                    conversation_history=conversation_history or [],
                )
                final = (
                    result.get("final_response", "") if isinstance(result, dict) else str(result)
                )
                result_queue.put({"chunk_type": "done", "content": final})
            except Exception as e:
                result_queue.put({"chunk_type": "error", "content": str(e), "is_error": True})
            finally:
                result_queue.put(None)  # sentinel

        thread = threading.Thread(target=_run, daemon=True)
        thread.start()

        while True:
            item = result_queue.get()
            if item is None:
                break
            yield item
