"""Configuration for claude-code-runtime."""

import os
from dataclasses import dataclass
from pathlib import Path

from dotenv import load_dotenv


@dataclass
class RuntimeConfig:
    """Runtime configuration from environment variables."""

    grpc_port: int = 50052
    runtime_id: str = "claude-code-runtime"
    runtime_name: str = "Claude Code Runtime"
    tier: str = "harness"

    # Anthropic SDK config
    anthropic_api_key: str = ""
    anthropic_base_url: str = ""
    anthropic_model_name: str = "claude-sonnet-4-20250514"

    # Claude Agent SDK config
    max_turns: int = 10
    max_budget_usd: float | None = None
    permission_mode: str = "bypassPermissions"

    @classmethod
    def from_env(cls, env_file: str | Path | None = None) -> "RuntimeConfig":
        """Load config from environment variables."""
        if env_file:
            load_dotenv(env_file)
        else:
            # Try project root .env
            root_env = Path(__file__).parent.parent.parent.parent.parent / ".env"
            if root_env.exists():
                load_dotenv(root_env)

        return cls(
            grpc_port=int(os.getenv("CLAUDE_RUNTIME_PORT", "50052")),
            runtime_id=os.getenv("CLAUDE_RUNTIME_ID", "claude-code-runtime"),
            runtime_name=os.getenv("CLAUDE_RUNTIME_NAME", "Claude Code Runtime"),
            anthropic_api_key=os.getenv("ANTHROPIC_API_KEY", ""),
            anthropic_base_url=os.getenv("ANTHROPIC_BASE_URL", ""),
            anthropic_model_name=os.getenv(
                "ANTHROPIC_MODEL_NAME", "claude-sonnet-4-20250514"
            ),
            max_turns=int(os.getenv("CLAUDE_MAX_TURNS", "10")),
            permission_mode=os.getenv("CLAUDE_PERMISSION_MODE", "bypassPermissions"),
        )
