"""Server info router implementation."""

from typing import Any

from fastapi import Request
from fastapi.responses import PlainTextResponse

from .base import BaseRouter
from core.config import Settings
from core.logger import LoggerService
from providers.manager import ProviderManager
from router.base import ServerInfo, ServerInfoResponse, ServerLoad, ServerModel

DEFAULT_SAMPLING_PARAMS: dict[str, Any] = {
    "max_tokens": 2048,
    "temperature": 1,
    "top_p": 0.699999988079071,
    "repetition_penalty": 1.100000023841858,
    "top_k": 0,
    "no_repeat_ngram_size": 0,
    "no_repeat_ngram_thr": 0,
    "no_repeat_ngram_window_size": 0,
    "no_repeat_ngram_penalty_multiplier": 0,
    "no_repeat_ngram_penalty_base": 0,
    "force_non_empty_response": True,
    "function_impossible_threshold": 0,
    "force_non_empty_function": False,
    "n": 1,
    "whitelist_check": True,
    "clean_whitelist_context": True,
    "clean_filter_context": "full",
    "function_schema_force": True,
}


class InfoRouter(BaseRouter):
    """Server info router implementation."""

    def __init__(
        self,
        logger: LoggerService,
        settings: Settings,
        provider_manager: ProviderManager,
    ):
        """Initialize router.

        Args:
            logger: Logger service instance
            settings: Settings instance
            provider_manager: Provider manager instance
        """
        self.logger = logger.get_logger(__name__)
        self.settings = settings
        self.provider_manager = provider_manager
        super().__init__(logger=logger, tags=["info"])

    def _setup_routes(self) -> None:
        """Setup router endpoints."""
        self.router.add_api_route(
            "/api/v1/info/json",
            self.get_info,
            methods=["GET"],
            response_model=ServerInfoResponse,
            summary="Server Info",
            description="Returns server information without models.",
            operation_id="get_server_info_api_v1",
            responses={
                200: {
                    "description": "Server information",
                    "content": {
                        "application/json": {
                            "example": {
                                "server_info": {
                                    "workers_count": 1,
                                    "server_version": "Undefined",
                                    "object": "server",
                                },
                                "models": [
                                    {
                                        "id": "GigaR-29b-v25.3",
                                        "max_seq_len": 32768,
                                        "max_input_len": 31744,
                                        "max_batch_size": 256,
                                        "busy_gpu": [6],
                                        "tp": 1,
                                        "sampling_params": DEFAULT_SAMPLING_PARAMS,
                                        "object": "model",
                                        "owned_by": "sberdevices",
                                        "load": {
                                            "queued_requests": 0,
                                            "active_requests": 0,
                                            "active_tokens": 0,
                                        },
                                    }
                                ],
                                "object": "list",
                            }
                        }
                    },
                }
            },
        )
        self.router.add_api_route(
            "/info/table",
            self.get_info_table,
            methods=["GET"],
            response_class=PlainTextResponse,
            summary="Server Info Table",
            description="Returns server information as plain-text ASCII table.",
            operation_id="get_server_info_table",
            responses={
                200: {
                    "description": "Server information in plain text table",
                    "content": {
                        "text/plain": {
                            "example": (
                                "+------------------------------+\n"
                                "| Server_version: Undefined    |\n"
                                "+------------------------------+\n"
                            )
                        }
                    },
                }
            },
        )

    async def _collect_server_info(self) -> ServerInfoResponse:
        """Collect server info in JSON DTO format."""
        provider_models = await self.provider_manager.get_models()

        response_models: list[ServerModel] = []
        for model in provider_models:
            capabilities = model.capabilities or {}

            max_seq_len = int(
                model.context_length or capabilities.get("context_length") or 32768
            )
            max_input_len = int(
                capabilities.get("max_prompt_tokens") or max(max_seq_len - 1024, 0)
            )
            max_batch_size = int(capabilities.get("max_batch_size") or 256)
            tp = int(capabilities.get("tp") or 1)

            raw_busy_gpu = capabilities.get("busy_gpu")
            busy_gpu = (
                [int(gpu) for gpu in raw_busy_gpu]
                if isinstance(raw_busy_gpu, list)
                else []
            )

            raw_sampling_params = capabilities.get("sampling_params")
            sampling_params = (
                raw_sampling_params
                if isinstance(raw_sampling_params, dict)
                else dict(DEFAULT_SAMPLING_PARAMS)
            )

            raw_load = capabilities.get("load")
            load_data = raw_load if isinstance(raw_load, dict) else {}

            response_models.append(
                ServerModel(
                    id=model.external_model_id,
                    max_seq_len=max_seq_len,
                    max_input_len=max_input_len,
                    max_batch_size=max_batch_size,
                    busy_gpu=busy_gpu,
                    tp=tp,
                    sampling_params=sampling_params,
                    object="model",
                    owned_by=getattr(model, "provider_id", None) or "sberdevices",
                    load=ServerLoad(
                        queued_requests=int(load_data.get("queued_requests", 0)),
                        active_requests=int(load_data.get("active_requests", 0)),
                        active_tokens=int(load_data.get("active_tokens", 0)),
                    ),
                )
            )

        return ServerInfoResponse(
            server_info=ServerInfo(
                workers_count=self.settings.WORKERS_COUNT,
                server_version=self.settings.SERVER_VERSION,
                object="server",
            ),
            models=response_models,
            object="list",
        )

    @staticmethod
    def _format_value(value: Any) -> str:
        """Convert values to endpoint-friendly string representation."""
        if isinstance(value, bool):
            return str(value).lower()
        if isinstance(value, float):
            return f"{value:.4f}"
        return str(value)

    @staticmethod
    def _format_decimal4(value: Any) -> str:
        """Convert value to fixed 4-decimal string."""
        try:
            return f"{float(value):.4f}"
        except (TypeError, ValueError):
            return "0.0000"

    def _build_table_text(self, info: ServerInfoResponse) -> str:
        """Build ASCII table from server info."""
        blocks: list[list[str]] = [
            [
                "Server_version: "
                f"{info.server_info.server_version} "
                f"Worker_threads: {info.server_info.workers_count}"
            ]
        ]

        for model in info.models:
            gpu_value = ",".join(str(gpu) for gpu in model.busy_gpu) if model.busy_gpu else "-"
            sampling = model.sampling_params

            blocks.append([f"{gpu_value} GPU | {model.id}:{model.tp}"])
            blocks.append(
                [
                    "0 | "
                    f"max_seq_len:        {model.max_seq_len:<12}"
                    f"queued_requests: {model.load.queued_requests}",
                    "  | "
                    f"max_input_len:      {model.max_input_len:<12}"
                    f"active_requests: {model.load.active_requests}",
                    "  | "
                    f"max_batch_size:     {model.max_batch_size:<12}"
                    f"active_tokens: {model.load.active_tokens}",
                ]
            )
            blocks.append(
                [
                    "2 | "
                    f"max_tokens:         {self._format_value(sampling.get('max_tokens', 2048))}"
                ]
            )
            blocks.append(
                [
                    "3 | "
                    "temperature:        "
                    f"{self._format_decimal4(sampling.get('temperature', 1))}",
                    "  | "
                    f"top_p:              {self._format_decimal4(sampling.get('top_p', 0))}",
                    "  | "
                    "repetition_penalty: "
                    f"{self._format_decimal4(sampling.get('repetition_penalty', 1))}",
                    "  | "
                    f"top_k:              {self._format_value(sampling.get('top_k', 0))}",
                ]
            )
            blocks.append(
                [
                    "6 | "
                    "clean_whitelist_context: "
                    f"{self._format_value(sampling.get('clean_whitelist_context', True))}",
                    "  | "
                    "function_impossible_threshold: "
                    f"{self._format_decimal4(sampling.get('function_impossible_threshold', 0))}",
                    "  | "
                    "force_non_empty_function_if_empty_content: "
                    f"{self._format_value(sampling.get('force_non_empty_function', False))}",
                ]
            )

        rows = [row for block in blocks for row in block]
        content_width = max(len(row) for row in rows) if rows else 0
        border = "+" + "-" * (content_width + 2) + "+"
        lines = []
        for block in blocks:
            lines.append(border)
            for row in block:
                lines.append(f"| {row.ljust(content_width)} |")
        lines.append(border)
        return "\n".join(lines) + "\n"

    async def get_info(self, request: Request) -> ServerInfoResponse:
        """Get server info endpoint."""
        self.logger.debug(
            "Server info requested",
            extra={
                "request_id": getattr(request.state, "request_id", None),
                "client": request.client.host if request.client else None,
                "headers": dict(request.headers),
            },
        )
        try:
            return await self._collect_server_info()
        except Exception as e:
            self.logger.error(
                "Failed to get server info",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                },
                exc_info=True,
            )
            raise

    async def get_info_table(self, request: Request) -> PlainTextResponse:
        """Get server info as ASCII table."""
        self.logger.debug(
            "Server info table requested",
            extra={
                "request_id": getattr(request.state, "request_id", None),
                "client": request.client.host if request.client else None,
                "headers": dict(request.headers),
            },
        )
        try:
            info = await self._collect_server_info()
            return PlainTextResponse(content=self._build_table_text(info))
        except Exception as e:
            self.logger.error(
                "Failed to get server info table",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                },
                exc_info=True,
            )
            raise
