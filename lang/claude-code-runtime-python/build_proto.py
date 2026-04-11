"""Build gRPC Python stubs from proto files."""

import os
import subprocess
import sys
from pathlib import Path

PROTO_ROOT = Path(os.getenv("PROTO_ROOT", Path(__file__).parent.parent.parent / "proto"))
OUT_DIR = Path(__file__).parent / "src" / "claude_code_runtime" / "_proto"


def build():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "__init__.py").touch()

    # EAASP v2.0 proto stubs.
    proto_files = [
        "eaasp/runtime/v2/common.proto",
        "eaasp/runtime/v2/runtime.proto",
        "eaasp/runtime/v2/hook.proto",
    ]

    for proto in proto_files:
        # Ensure output subdirectories exist
        proto_path = Path(proto)
        out_subdir = OUT_DIR / proto_path.parent
        out_subdir.mkdir(parents=True, exist_ok=True)

        # Create __init__.py for each package level
        parts = proto_path.parent.parts
        for i in range(len(parts)):
            init_path = OUT_DIR / Path(*parts[: i + 1]) / "__init__.py"
            init_path.touch()

        cmd = [
            sys.executable,
            "-m",
            "grpc_tools.protoc",
            f"--proto_path={PROTO_ROOT}",
            f"--python_out={OUT_DIR}",
            f"--grpc_python_out={OUT_DIR}",
            f"--pyi_out={OUT_DIR}",
            str(PROTO_ROOT / proto),
        ]
        print(f"Compiling {proto}...")
        subprocess.check_call(cmd)

    # Fix imports in generated files
    _fix_imports(OUT_DIR)
    print("Proto build complete.")


def _fix_imports(out_dir: Path):
    """Fix generated import paths to use package-relative imports."""
    for py_file in out_dir.rglob("*.py"):
        content = py_file.read_text()
        # Fix: "from eaasp.common.v1 import common_pb2"
        # To:  "from claude_code_runtime._proto.eaasp.common.v1 import common_pb2"
        fixed = content.replace(
            "from eaasp.", "from claude_code_runtime._proto.eaasp."
        )
        if fixed != content:
            py_file.write_text(fixed)
            print(f"  Fixed imports in {py_file.relative_to(out_dir)}")


if __name__ == "__main__":
    build()
