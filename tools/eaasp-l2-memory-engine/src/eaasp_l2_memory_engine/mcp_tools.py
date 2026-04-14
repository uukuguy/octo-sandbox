"""7 MCP tools exposed via REST facade (per S3.T1 pattern, D10 deferred).

Tools:
    memory_search, memory_read, memory_write_anchor, memory_write_file,
    memory_list (paginated via offset, S2.T3), memory_archive,
    memory_confirm (S2.T3).
"""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel

from .anchors import AnchorIn, AnchorStore
from .files import InvalidStatusTransition, MemoryFileIn, MemoryFileStore
from .index import MAX_TOP_K, HybridIndex

MAX_LIST_LIMIT = 200


class ToolManifestEntry(BaseModel):
    name: str
    description: str
    input_schema: dict[str, Any]


MCP_TOOL_MANIFEST: list[ToolManifestEntry] = [
    ToolManifestEntry(
        name="memory_search",
        description="Hybrid keyword + semantic + time-decay ranked search over memory files.",
        input_schema={
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "top_k": {"type": "integer", "default": 10},
                "scope": {"type": "string"},
                "category": {"type": "string"},
            },
            "required": ["query"],
        },
    ),
    ToolManifestEntry(
        name="memory_read",
        description="Read the latest version of a memory file by memory_id.",
        input_schema={
            "type": "object",
            "properties": {"memory_id": {"type": "string"}},
            "required": ["memory_id"],
        },
    ),
    ToolManifestEntry(
        name="memory_write_anchor",
        description="Append-only write of an evidence anchor.",
        input_schema={
            "type": "object",
            "properties": {
                "event_id": {"type": "string"},
                "session_id": {"type": "string"},
                "type": {"type": "string"},
                "data_ref": {"type": "string"},
                "snapshot_hash": {"type": "string"},
                "source_system": {"type": "string"},
                "tool_version": {"type": "string"},
                "model_version": {"type": "string"},
                "rule_version": {"type": "string"},
                "metadata": {"type": "object"},
            },
            "required": ["event_id", "session_id", "type"],
        },
    ),
    ToolManifestEntry(
        name="memory_write_file",
        description="Create a new memory file or bump the version of an existing memory_id.",
        input_schema={
            "type": "object",
            "properties": {
                "memory_id": {"type": "string"},
                "scope": {"type": "string"},
                "category": {"type": "string"},
                "content": {"type": "string"},
                "evidence_refs": {
                    "type": "array",
                    "items": {"type": "string"},
                },
                "status": {
                    "type": "string",
                    "enum": ["agent_suggested", "confirmed", "archived"],
                },
            },
            "required": ["scope", "category", "content"],
        },
    ),
    ToolManifestEntry(
        name="memory_list",
        description="List latest versions of memory files by scope/category/status.",
        input_schema={
            "type": "object",
            "properties": {
                "scope": {"type": "string"},
                "category": {"type": "string"},
                "status": {
                    "type": "string",
                    "enum": ["agent_suggested", "confirmed", "archived"],
                },
                "limit": {"type": "integer", "default": 50},
                "offset": {"type": "integer", "default": 0, "minimum": 0},
            },
        },
    ),
    ToolManifestEntry(
        name="memory_archive",
        description="Transition a memory file's status to archived.",
        input_schema={
            "type": "object",
            "properties": {"memory_id": {"type": "string"}},
            "required": ["memory_id"],
        },
    ),
    ToolManifestEntry(
        name="memory_confirm",
        description="Transition a memory file's status to confirmed "
        "(from agent_suggested).",
        input_schema={
            "type": "object",
            "properties": {"memory_id": {"type": "string"}},
            "required": ["memory_id"],
        },
    ),
]


class ToolError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class McpToolDispatcher:
    """Dispatch MCP tool invocations to the store/index backends."""

    def __init__(
        self,
        anchors: AnchorStore,
        files: MemoryFileStore,
        index: HybridIndex,
    ) -> None:
        self.anchors = anchors
        self.files = files
        self.index = index

    async def invoke(self, name: str, args: dict[str, Any]) -> dict[str, Any]:
        handler = _HANDLERS.get(name)
        if handler is None:
            raise ToolError("unknown_tool", f"unknown MCP tool: {name}")
        return await handler(self, args)

    async def _memory_search(self, args: dict[str, Any]) -> dict[str, Any]:
        query = _require(args, "query", str)
        top_k = max(1, min(int(args.get("top_k", 10)), MAX_TOP_K))
        hits = await self.index.search(
            query=query,
            top_k=top_k,
            scope=args.get("scope"),
            category=args.get("category"),
        )
        return {"hits": [h.model_dump() for h in hits]}

    async def _memory_read(self, args: dict[str, Any]) -> dict[str, Any]:
        memory_id = _require(args, "memory_id", str)
        memory = await self.files.read_latest(memory_id)
        if memory is None:
            raise ToolError("not_found", f"memory_id not found: {memory_id}")
        return memory.model_dump()

    async def _memory_write_anchor(self, args: dict[str, Any]) -> dict[str, Any]:
        anchor = AnchorIn(**args)
        out = await self.anchors.write(anchor)
        return out.model_dump()

    async def _memory_write_file(self, args: dict[str, Any]) -> dict[str, Any]:
        memory = MemoryFileIn(**args)
        try:
            out = await self.files.write(memory)
        except InvalidStatusTransition as exc:
            raise ToolError("invalid_transition", str(exc)) from exc
        return out.model_dump()

    async def _memory_list(self, args: dict[str, Any]) -> dict[str, Any]:
        limit = max(1, min(int(args.get("limit", 50)), MAX_LIST_LIMIT))
        # S2.T3: offset defaults to 0 and is clamped to >= 0 so negative
        # inputs cannot produce SQL errors or surprise pagination semantics.
        offset = max(0, int(args.get("offset", 0)))
        memories = await self.files.list(
            scope=args.get("scope"),
            category=args.get("category"),
            status=args.get("status"),
            limit=limit,
            offset=offset,
        )
        return {"memories": [m.model_dump() for m in memories]}

    async def _memory_archive(self, args: dict[str, Any]) -> dict[str, Any]:
        memory_id = _require(args, "memory_id", str)
        try:
            out = await self.files.archive(memory_id)
        except KeyError as exc:
            raise ToolError("not_found", str(exc)) from exc
        except InvalidStatusTransition as exc:
            raise ToolError("invalid_transition", str(exc)) from exc
        return out.model_dump()

    async def _memory_confirm(self, args: dict[str, Any]) -> dict[str, Any]:
        """S2.T3: transition agent_suggested → confirmed.

        Raises ToolError("not_found") when memory_id is unknown and
        ToolError("invalid_transition") when the latest status cannot
        legally move to confirmed (i.e. already archived, or already
        confirmed — the _ALLOWED_TRANSITIONS table forbids self-loops).
        """
        memory_id = _require(args, "memory_id", str)
        try:
            out = await self.files.confirm(memory_id)
        except KeyError as exc:
            raise ToolError("not_found", str(exc)) from exc
        except InvalidStatusTransition as exc:
            raise ToolError("invalid_transition", str(exc)) from exc
        return out.model_dump()


_HANDLERS: dict[str, Any] = {
    "memory_search": McpToolDispatcher._memory_search,
    "memory_read": McpToolDispatcher._memory_read,
    "memory_write_anchor": McpToolDispatcher._memory_write_anchor,
    "memory_write_file": McpToolDispatcher._memory_write_file,
    "memory_list": McpToolDispatcher._memory_list,
    "memory_archive": McpToolDispatcher._memory_archive,
    "memory_confirm": McpToolDispatcher._memory_confirm,
}


def _require(args: dict[str, Any], key: str, expected_type: type) -> Any:
    if key not in args:
        raise ToolError("missing_arg", f"missing required arg: {key}")
    value = args[key]
    if not isinstance(value, expected_type):
        raise ToolError(
            "invalid_arg",
            f"arg {key} must be {expected_type.__name__}, got {type(value).__name__}",
        )
    return value
