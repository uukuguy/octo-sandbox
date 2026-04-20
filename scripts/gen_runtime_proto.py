#!/usr/bin/env python3
"""Generate Python gRPC stubs for EAASP v2 runtime packages.

Replaces four near-identical build_proto.py scripts under:
  - lang/claude-code-runtime-python/
  - lang/nanobot-runtime-python/
  - lang/pydantic-ai-runtime-python/
  - tools/eaasp-l4-orchestration/

Behavior per package is selected by --package-name. The registry below is the
single source of truth mapping a package slug to (directory, python package
prefix, default proto files). --proto-files overrides the default list.

Usage:
    python scripts/gen_runtime_proto.py --package-name claude-code-runtime
    python scripts/gen_runtime_proto.py --package-name eaasp-l4-orchestration
    python scripts/gen_runtime_proto.py --package-name nanobot-runtime \\
        --proto-files eaasp/runtime/v2/common.proto eaasp/runtime/v2/runtime.proto

Environment:
    PROTO_ROOT  Override the proto source root (default: <repo>/proto).
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

# Repo root — script lives at <repo>/scripts/.
REPO_ROOT = Path(__file__).resolve().parent.parent
PROTO_ROOT_DEFAULT = REPO_ROOT / "proto"

# Registry: package-name → (package_dir, src_package_name, pkg_prefix, default_protos)
#   package_dir      — directory containing the package (relative to repo root)
#   src_package_name — top-level Python package name under src/
#   pkg_prefix       — dotted Python prefix used for generated import rewrites
#   default_protos   — proto files (relative to PROTO_ROOT) compiled by default
PACKAGES: dict[str, tuple[Path, str, str, tuple[str, ...]]] = {
    "claude-code-runtime": (
        Path("lang/claude-code-runtime-python"),
        "claude_code_runtime",
        "claude_code_runtime._proto",
        (
            "eaasp/runtime/v2/common.proto",
            "eaasp/runtime/v2/runtime.proto",
            "eaasp/runtime/v2/hook.proto",
        ),
    ),
    "nanobot-runtime": (
        Path("lang/nanobot-runtime-python"),
        "nanobot_runtime",
        "nanobot_runtime._proto",
        (
            "eaasp/runtime/v2/common.proto",
            "eaasp/runtime/v2/runtime.proto",
            "eaasp/runtime/v2/hook.proto",
        ),
    ),
    "pydantic-ai-runtime": (
        Path("lang/pydantic-ai-runtime-python"),
        "pydantic_ai_runtime",
        "pydantic_ai_runtime._proto",
        (
            "eaasp/runtime/v2/common.proto",
            "eaasp/runtime/v2/runtime.proto",
            "eaasp/runtime/v2/hook.proto",
        ),
    ),
    "eaasp-l4-orchestration": (
        Path("tools/eaasp-l4-orchestration"),
        "eaasp_l4_orchestration",
        "eaasp_l4_orchestration._proto",
        # L4 is a gRPC client only — no hook.proto needed.
        (
            "eaasp/runtime/v2/common.proto",
            "eaasp/runtime/v2/runtime.proto",
        ),
    ),
}

# Invariant: pkg_prefix is always f"{src_pkg_name}._proto". Both fields exist
# for reader clarity, but they must stay in sync. Bail at import time if a
# future registry row violates this.
for _name, (_, _src, _pfx, _) in PACKAGES.items():
    assert _pfx == f"{_src}._proto", (
        f"PACKAGES[{_name!r}] pkg_prefix {_pfx!r} "
        f"must equal f'{{src_pkg_name}}._proto' = {_src}._proto"
    )


def build(package_name: str, proto_files: tuple[str, ...] | None = None) -> None:
    if package_name not in PACKAGES:
        sys.exit(f"error: unknown --package-name {package_name!r}; " f"known: {sorted(PACKAGES)}")
    pkg_dir, src_pkg, pkg_prefix, default_protos = PACKAGES[package_name]
    protos = tuple(proto_files) if proto_files else default_protos

    proto_root = Path(os.getenv("PROTO_ROOT", PROTO_ROOT_DEFAULT))
    out_dir = REPO_ROOT / pkg_dir / "src" / src_pkg / "_proto"
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "__init__.py").touch()

    for proto in protos:
        proto_path = Path(proto)
        out_subdir = out_dir / proto_path.parent
        out_subdir.mkdir(parents=True, exist_ok=True)

        # Create __init__.py for each package level.
        parts = proto_path.parent.parts
        for i in range(len(parts)):
            init_path = out_dir / Path(*parts[: i + 1]) / "__init__.py"
            init_path.touch()

        cmd = [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"--proto_path={proto_root}",
            f"--python_out={out_dir}",
            f"--grpc_python_out={out_dir}",
            f"--pyi_out={out_dir}",
            str(proto_root / proto),
        ]
        print(f"[{package_name}] Compiling {proto}...")
        subprocess.check_call(cmd)

    _fix_imports(out_dir, pkg_prefix)
    print(f"[{package_name}] Proto build complete.")


def _fix_imports(out_dir: Path, pkg_prefix: str) -> None:
    """Rewrite bare ``from eaasp.`` imports to use the package namespace."""
    for py_file in out_dir.rglob("*.py"):
        content = py_file.read_text()
        fixed = content.replace("from eaasp.", f"from {pkg_prefix}.eaasp.")
        if fixed != content:
            py_file.write_text(fixed)
            print(f"  Fixed imports in {py_file.relative_to(out_dir)}")


def _parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate Python gRPC stubs for EAASP v2 runtime packages.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=("Registered packages:\n" + "\n".join(f"  - {name}" for name in sorted(PACKAGES))),
    )
    parser.add_argument(
        "--package-name",
        required=True,
        choices=sorted(PACKAGES),
        help="Runtime/tool package slug to regenerate stubs for.",
    )
    parser.add_argument(
        "--proto-files",
        nargs="+",
        metavar="PROTO",
        default=None,
        help=(
            "Proto files (relative to PROTO_ROOT) to compile; overrides the "
            "registry default. Example: eaasp/runtime/v2/common.proto"
        ),
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = _parse_args(argv)
    build(args.package_name, tuple(args.proto_files) if args.proto_files else None)


if __name__ == "__main__":
    main()
