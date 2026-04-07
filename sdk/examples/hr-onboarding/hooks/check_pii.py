#!/usr/bin/env python3
"""PreToolUse command handler — check for PII before file_write.

Reads a JSON payload from stdin with the tool invocation context,
scans the content for common PII patterns, and prints a JSON decision.

Decision format:
  {"decision": "allow"}   — no PII detected, proceed
  {"decision": "block", "reason": "..."}  — PII detected, block the write
"""

from __future__ import annotations

import json
import re
import sys

# Simple PII patterns (for demonstration — production would use a proper library)
_PII_PATTERNS = [
    (r"\b\d{3}-\d{2}-\d{4}\b", "SSN"),
    (r"\b\d{18}|\d{17}X\b", "Chinese ID number"),
    (r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b", "email address"),
    (r"\b1[3-9]\d{9}\b", "Chinese phone number"),
]


def check_pii(content: str) -> list[str]:
    """Return list of PII types found in content."""
    found = []
    for pattern, label in _PII_PATTERNS:
        if re.search(pattern, content):
            found.append(label)
    return found


def main() -> None:
    payload = json.load(sys.stdin)
    content = payload.get("tool_input", {}).get("content", "")
    pii_types = check_pii(content)
    if pii_types:
        result = {
            "decision": "block",
            "reason": f"PII detected: {', '.join(pii_types)}. Redact before writing.",
        }
    else:
        result = {"decision": "allow"}
    json.dump(result, sys.stdout)


if __name__ == "__main__":
    main()
