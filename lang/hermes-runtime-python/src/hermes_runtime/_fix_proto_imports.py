"""Fix proto import paths for EAASP v2 stubs.

hermes-runtime-python symlinks `_proto/` to `claude-code-runtime-python/src/
claude_code_runtime/_proto/`. The stubs there are generated with absolute
imports of the form:

    from claude_code_runtime._proto.eaasp.runtime.v2 import common_pb2, ...

So for those imports to resolve inside hermes, the `claude-code-runtime-python/
src/` directory must be on sys.path (so the `claude_code_runtime` package is
importable).

To let hermes code use the clean spelling `from eaasp.runtime.v2 import ...`,
this shim also registers `eaasp.runtime.v2` as an alias in sys.modules pointing
at the already-loaded package.

Call `fix()` once before any proto imports (already done at the top of every
module that imports proto types).
"""

import sys
from pathlib import Path

_fixed = False


def fix():
    global _fixed
    if _fixed:
        return

    # Resolve path to claude-code-runtime-python/src so `claude_code_runtime`
    # is importable. Path layout:
    #   lang/hermes-runtime-python/src/hermes_runtime/_fix_proto_imports.py
    #   lang/claude-code-runtime-python/src/claude_code_runtime/_proto/...
    here = Path(__file__).resolve()
    ccr_src = (
        here.parent.parent.parent.parent  # lang/
        / "claude-code-runtime-python"
        / "src"
    )
    if ccr_src.is_dir():
        ccr_src_str = str(ccr_src)
        if ccr_src_str not in sys.path:
            sys.path.insert(0, ccr_src_str)

    # Register short alias `eaasp.runtime.v2` → claude_code_runtime._proto.eaasp.runtime.v2
    # so hermes modules can write `from eaasp.runtime.v2 import runtime_pb2, ...`.
    import importlib
    import importlib.util
    import importlib.machinery

    v2_pkg = importlib.import_module(
        "claude_code_runtime._proto.eaasp.runtime.v2"
    )
    # Create placeholder eaasp and eaasp.runtime packages as needed.
    for name in ("eaasp", "eaasp.runtime"):
        if name not in sys.modules:
            mod = importlib.util.module_from_spec(  # type: ignore[attr-defined]
                importlib.machinery.ModuleSpec(name, loader=None, is_package=True)
            )
            mod.__path__ = []  # type: ignore[attr-defined]
            sys.modules[name] = mod

    sys.modules["eaasp.runtime.v2"] = v2_pkg

    # Also re-export the individual _pb2 / _pb2_grpc modules under the alias
    for sub in ("common_pb2", "runtime_pb2", "hook_pb2", "runtime_pb2_grpc", "hook_pb2_grpc"):
        full = f"claude_code_runtime._proto.eaasp.runtime.v2.{sub}"
        alias = f"eaasp.runtime.v2.{sub}"
        sys.modules[alias] = importlib.import_module(full)

    _fixed = True
