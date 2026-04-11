"""claude-code-runtime gRPC server entry point."""

import argparse
import asyncio
import logging
import signal

import grpc.aio

from ._proto.eaasp.runtime.v2 import runtime_pb2_grpc
from .config import RuntimeConfig
from .service import RuntimeServiceImpl

logger = logging.getLogger(__name__)


async def serve(config: RuntimeConfig) -> None:
    """Start the gRPC server."""
    server = grpc.aio.server()
    service = RuntimeServiceImpl(config)
    runtime_pb2_grpc.add_RuntimeServiceServicer_to_server(service, server)

    addr = f"[::]:{config.grpc_port}"
    server.add_insecure_port(addr)

    await server.start()
    logger.info(
        "claude-code-runtime gRPC server started on %s (model=%s)",
        addr,
        config.anthropic_model_name,
    )

    # Graceful shutdown
    loop = asyncio.get_event_loop()
    stop_event = asyncio.Event()

    def _signal_handler():
        logger.info("Shutdown signal received")
        stop_event.set()

    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, _signal_handler)

    await stop_event.wait()
    await server.stop(grace=5)
    logger.info("Server stopped.")


def main():
    parser = argparse.ArgumentParser(
        description="claude-code-runtime gRPC server"
    )
    parser.add_argument(
        "--port", type=int, default=None, help="gRPC port (default: 50052)"
    )
    parser.add_argument(
        "--env-file", type=str, default=None, help="Path to .env file"
    )
    parser.add_argument(
        "--log-level", type=str, default="INFO", help="Log level"
    )
    args = parser.parse_args()

    logging.basicConfig(
        level=getattr(logging, args.log_level.upper()),
        format="%(asctime)s %(name)s %(levelname)s %(message)s",
    )

    config = RuntimeConfig.from_env(env_file=args.env_file)
    if args.port:
        config.grpc_port = args.port

    asyncio.run(serve(config))


if __name__ == "__main__":
    main()
