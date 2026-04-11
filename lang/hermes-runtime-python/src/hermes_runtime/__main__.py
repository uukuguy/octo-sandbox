"""hermes-runtime gRPC server entry point."""

import asyncio
import logging

from grpc import aio

from hermes_runtime._fix_proto_imports import fix as _fix_proto_imports

_fix_proto_imports()

from eaasp.runtime.v2 import runtime_pb2_grpc  # noqa: E402

from hermes_runtime.config import HermesRuntimeConfig
from hermes_runtime.service import RuntimeServiceImpl

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(name)s %(levelname)s %(message)s",
)
logger = logging.getLogger("hermes-runtime")


async def serve():
    config = HermesRuntimeConfig.from_env()
    server = aio.server()
    service = RuntimeServiceImpl(config)
    runtime_pb2_grpc.add_RuntimeServiceServicer_to_server(service, server)
    addr = f"[::]:{config.grpc_port}"
    server.add_insecure_port(addr)
    logger.info(
        "hermes-runtime starting on %s (model=%s, tier=%s)",
        addr, config.hermes_model, config.tier,
    )
    await server.start()
    await server.wait_for_termination()


def main():
    asyncio.run(serve())


if __name__ == "__main__":
    main()
