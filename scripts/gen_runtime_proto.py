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
import re
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


def build(
    package_name: str,
    proto_files: tuple[str, ...] | None = None,
    out_dir_override: Path | None = None,
) -> None:
    if package_name not in PACKAGES:
        sys.exit(f"error: unknown --package-name {package_name!r}; " f"known: {sorted(PACKAGES)}")
    pkg_dir, src_pkg, pkg_prefix, default_protos = PACKAGES[package_name]
    protos = tuple(proto_files) if proto_files else default_protos

    proto_root = Path(os.getenv("PROTO_ROOT", PROTO_ROOT_DEFAULT))
    # --out-dir overrides the <repo>/lang/<pkg>/src/<mod>/_proto default;
    # Dockerfile builds use this to target a flattened source tree (D153).
    out_dir = out_dir_override if out_dir_override else REPO_ROOT / pkg_dir / "src" / src_pkg / "_proto"
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
    _loosen_enum_stubs(out_dir)
    print(f"[{package_name}] Proto build complete.")


def _fix_imports(out_dir: Path, pkg_prefix: str) -> None:
    """Rewrite bare ``from eaasp.`` imports to use the package namespace."""
    for py_file in out_dir.rglob("*.py"):
        content = py_file.read_text()
        fixed = content.replace("from eaasp.", f"from {pkg_prefix}.eaasp.")
        if fixed != content:
            py_file.write_text(fixed)
            print(f"  Fixed imports in {py_file.relative_to(out_dir)}")


# D152 post-process: grpcio-tools generates `_Union[<EnumClass>, str]` for
# proto3 enum fields in message __init__ signatures, rejecting `int` values
# even though the runtime accepts them (enums subclass int via
# EnumTypeWrapper). Upstream fix tracked at
# https://github.com/protocolbuffers/protobuf/pull/25319 (OPEN, unmerged).
# Until that lands, we rewrite `_Union[X, str]` to `_Union[X, str, int]` in
# generated .pyi files so type-checkers (mypy, Pyright) accept
# `CHUNK_TYPE_TEXT_DELTA` (an int constant) without `# type: ignore`.
#
# Match surface (verified across runtime_pb2.pyi / hook_pb2.pyi / common_pb2.pyi
# for 4 packages): only `_Union[X, str]` — never `_Union[X, _Mapping]` (nested
# Messages) or `_Union[X, str, int]` (already loosened, idempotent).
#
# The X alternative is a dotted identifier or nested class reference like
# `_common_pb2.ChunkType`, `HookEventType`, `Capabilities.CredentialMode`.
_UNION_ENUM_STR_RE = re.compile(
    r"_Union\[(?P<enum>[A-Za-z_][\w.]*), str\](?!, int\])"
)


def _loosen_enum_stubs(out_dir: Path) -> int:
    """Rewrite `_Union[EnumClass, str]` → `_Union[EnumClass, str, int]` in .pyi.

    Returns the number of substitutions made. Idempotent: running twice
    produces zero substitutions on the second pass.
    """
    total = 0
    for pyi_file in out_dir.rglob("*_pb2.pyi"):
        content = pyi_file.read_text()
        new_content, count = _UNION_ENUM_STR_RE.subn(
            r"_Union[\g<enum>, str, int]",
            content,
        )
        if count:
            pyi_file.write_text(new_content)
            print(
                f"  Loosened {count} enum stub(s) in "
                f"{pyi_file.relative_to(out_dir)} (D152)"
            )
            total += count
    return total


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
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=None,
        metavar="DIR",
        help=(
            "Override the output directory (default: "
            "<repo>/<package_dir>/src/<pkg>/_proto). Useful for Docker "
            "builds where the source tree is flattened."
        ),
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = _parse_args(argv)
    build(
        args.package_name,
        tuple(args.proto_files) if args.proto_files else None,
        args.out_dir,
    )


if __name__ == "__main__":
    main()
