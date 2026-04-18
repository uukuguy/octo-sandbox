"""E2E test fixtures — in-process L3 + mock L1 for integration testing.

Two modes:
  - mock_llm: all tests run in-process, no external dependencies
  - live_llm: requires running services (skipped in CI)
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

# Add tools to path for imports
TOOLS_DIR = Path(__file__).resolve().parents[2] / "tools"
sys.path.insert(0, str(TOOLS_DIR / "eaasp-governance" / "src"))
sys.path.insert(0, str(TOOLS_DIR / "eaasp-session-manager" / "src"))

try:
    from eaasp_governance.main import create_app as create_l3_app
except ModuleNotFoundError:
    create_l3_app = None  # type: ignore[assignment]

EXAMPLES_DIR = Path(__file__).resolve().parents[2] / "sdk" / "examples" / "hr-onboarding"
CONFIG_DIR = TOOLS_DIR / "eaasp-governance" / "config"


@pytest.fixture
def l3_client():
    """Create L3 governance service TestClient with policies pre-deployed."""
    app = create_l3_app(
        l2_url="http://mock-l2:8081",
        runtimes_config=str(CONFIG_DIR / "runtimes.yaml"),
    )
    client = TestClient(app)

    # Deploy both example policies
    for yaml_file in ["enterprise.yaml", "bu_hr.yaml"]:
        yaml_content = (EXAMPLES_DIR / "policies" / yaml_file).read_text()
        resp = client.put(
            "/v1/policies/deploy",
            content=yaml_content,
            headers={"Content-Type": "application/yaml"},
        )
        assert resp.status_code == 200

    return client
