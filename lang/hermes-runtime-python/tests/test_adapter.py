"""Tests for HermesAdapter."""

from hermes_runtime.adapter import HermesAdapter
from hermes_runtime.config import HermesRuntimeConfig


def test_adapter_create_remove():
    """Adapter 可以创建和移除 agent 占位（不实际初始化 AIAgent）。"""
    config = HermesRuntimeConfig()
    adapter = HermesAdapter(config)
    # 直接操作内部 dict 模拟（避免实际导入 hermes AIAgent）
    adapter._agents["test-1"] = "mock-agent"
    assert adapter.get_agent("test-1") == "mock-agent"
    assert adapter.get_agent("nonexistent") is None
    removed = adapter.remove_agent("test-1")
    assert removed == "mock-agent"
    assert adapter.get_agent("test-1") is None


def test_send_message_no_agent():
    """send_message 对不存在的 session 返回 error chunk。"""
    config = HermesRuntimeConfig()
    adapter = HermesAdapter(config)
    chunks = list(adapter.send_message("nonexistent", "hello"))
    assert len(chunks) == 1
    assert chunks[0]["chunk_type"] == "error"
