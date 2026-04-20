#!/usr/bin/env bash
# check-pyright-prereqs.sh — verify the 9 per-package .venv directories
# that pyrightconfig.json binds to actually exist.
#
# Without them Pyright (in editor or headless) silently falls back to
# the repo-root .venv (no grpc) and reports 500+ bogus unresolved
# imports. D155 tracks this failure mode; this script is the gate.
#
# Exit 0: all 9 venvs present.
# Exit 1: at least one missing — first line of stderr names the first gap.
#
# Usage:
#   scripts/check-pyright-prereqs.sh           # scan all
#   MISSING_OK=1 scripts/check-pyright-prereqs.sh  # warn only, don't exit 1

set -euo pipefail

VENVS=(
  "lang/claude-code-runtime-python/.venv"
  "lang/nanobot-runtime-python/.venv"
  "lang/pydantic-ai-runtime-python/.venv"
  "tools/eaasp-l4-orchestration/.venv"
  "tools/eaasp-l3-governance/.venv"
  "tools/eaasp-l2-memory-engine/.venv"
  "tools/eaasp-cli-v2/.venv"
  "tools/mock-scada/.venv"
  "sdk/python/.venv"
)

missing=()
for venv in "${VENVS[@]}"; do
  if [ ! -d "$venv" ]; then
    missing+=("$venv")
  fi
done

if [ ${#missing[@]} -eq 0 ]; then
  echo "✓ all ${#VENVS[@]} per-package .venv directories present"
  exit 0
fi

echo "✗ ${#missing[@]}/${#VENVS[@]} per-package .venv director$( [ ${#missing[@]} -eq 1 ] && echo y || echo ies ) missing:" >&2
for v in "${missing[@]}"; do
  echo "  - $v" >&2
done
echo "" >&2
echo "Fix: run 'uv sync' inside each missing package, or 'make setup' to bootstrap all." >&2

if [ "${MISSING_OK:-0}" = "1" ]; then
  exit 0
fi
exit 1
