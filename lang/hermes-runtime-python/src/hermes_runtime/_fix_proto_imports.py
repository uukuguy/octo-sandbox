"""Fix proto import paths — add _proto directory to sys.path.

grpc_tools.protoc generates imports like `from eaasp.common.v1 import common_pb2`.
Since our proto stubs live under `_proto/eaasp/`, we need `_proto/` on sys.path
for these imports to resolve.

Call `fix()` once before any proto imports.
"""

import sys
from pathlib import Path

_fixed = False


def fix():
    global _fixed
    if _fixed:
        return
    proto_dir = str(Path(__file__).parent / "_proto")
    if proto_dir not in sys.path:
        sys.path.insert(0, proto_dir)
    _fixed = True
