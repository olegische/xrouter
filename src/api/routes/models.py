"""Models router implementation."""

from typing import Dict, Optional, Union

from fastapi import Request

from .base import BaseRouter
from core.config import Settings
from core.logger import LoggerService
from providers.manager import ProviderManager
from router.base import (
    ModelArchitecture,
    ModelLimits,
    ModelPricing,
    ModelProvider,
    ModelResponse,
    ModelsResponse,
    OpenAIModel,
    OpenAIModelsResponse,
)
from router.usage.client import UsageClient
from router.usage.models import ModelRateResponse


class ModelsRouter(BaseRouter):
    """Models router implementation."""

    def __init__(
        self,
        logger: LoggerService,
        provider_manager: ProviderManager,
        settings: Settings,
        usage_client: Optional[UsageClient] = None,
    ) -> None:
        """Initialize router.

        Args:
            logger: Logger service instance
            provider_manager: Provider manager instance
            settings: Settings instance
        """
        # Set up instance attributes before calling super().__init__
        self.logger = logger.get_logger(__name__)
        self.provider_manager = provider_manager
        self.settings = settings
        self.usage_client = usage_client

        # Initialize with empty prefix - will be set in _setup_routes
        super().__init__(logger=logger, prefix="", tags=["models"])

    def _setup_routes(self) -> None:
        """Setup router endpoints based on API type."""
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            # OpenAI-compatible route
            self.router.add_api_route(
                "/v1/models",  # OpenAI-style path
                self.get_models,
                methods=["GET"],
                response_model=OpenAIModelsResponse,
                operation_id="get_models_v1",
                tags=["models"],
                responses={
                    200: {
                        "description": "List of available models",
                    },
                    500: {
                        "description": "Internal server error",
                        "content": {
                            "application/json": {
                                "example": {"detail": "Failed to get models"}
                            }
                        },
                    },
                },
                summary="Get Models",
                description=(
                    "Get a list of all available models. "
                    "Use order=newest to sort by creation date in descending order. "
                    "Use supported_parameters=tools to filter models that support "
                    "tool calls."
                ),
            )
        else:
            # XRouter-compatible route
            self.router.add_api_route(
                "/api/v1/models",  # XRouter-style path
                self.get_models,
                methods=["GET"],
                response_model=ModelsResponse,
                operation_id="get_models_v1",
                tags=["models"],
                responses={
                    200: {
                        "description": "List of available models",
                    },
                    500: {
                        "description": "Internal server error",
                        "content": {
                            "application/json": {
                                "example": {"detail": "Failed to get models"}
                            }
                        },
                    },
                },
                summary="Get Models",
                description=(
                    "Get a list of all available models. "
                    "Use order=newest to sort by creation date in descending order. "
                    "Use supported_parameters=tools to filter models that support "
                    "tool calls."
                ),
            )

    async def get_models(
        self,
        fastapi_request: Request,
    ) -> Union[ModelsResponse, OpenAIModelsResponse]:
        """Get all available models.

        Args:
            fastapi_request: FastAPI request object

        Returns:
            List of available models wrapped in data field
        """
        self.logger.debug(
            "Getting all models with rates",
            extra={
                "request_id": getattr(fastapi_request.state, "request_id", None),
                "client": fastapi_request.client.host
                if fastapi_request.client
                else None,
                "headers": dict(fastapi_request.headers),
                "enable_billing": self.settings.ENABLE_LLM_BILLING,
                "openai_compatible": self.settings.ENABLE_OPENAI_COMPATIBLE_API,
            },
        )

        try:
            # Get models
            models = await self.provider_manager.get_models()

            model_rates_map: Dict[str, ModelRateResponse] = {}
            if (
                not self.settings.ENABLE_OPENAI_COMPATIBLE_API
                and self.settings.ENABLE_LLM_BILLING
                and self.usage_client
            ):
                model_rates = await self.usage_client.get_all_model_rates()
                model_rates_map = {rate.model: rate for rate in model_rates}
                self.logger.info(
                    "Retrieved model rates for pricing",
                    extra={"rates_count": len(model_rates)},
                )

            # Convert to ModelResponse format
            responses = []
            for model in models:
                # Convert capabilities to top_provider format
                top_provider = {
                    "context_length": model.capabilities.get("context_length", 0),
                    "max_completion_tokens": model.capabilities.get(
                        "max_completion_tokens", 0
                    ),
                    "is_moderated": model.capabilities.get("is_moderated", True),
                }

                pricing = None
                if model_rates_map and model.external_model_id in model_rates_map:
                    rate = model_rates_map[model.external_model_id]
                    pricing = ModelPricing(
                        prompt=str(rate.prompt_rate),
                        completion=str(rate.completion_rate),
                        request="0",
                        image=str(rate.image_rate) if rate.image_rate else "0",
                        web_search="0",
                        internal_reasoning=str(rate.reasoning_rate)
                        if rate.reasoning_rate is not None
                        else "0",
                    )

                # Create response model
                response = ModelResponse(
                    id=model.external_model_id,
                    name=model.name,
                    description=model.description,
                    context_length=model.context_length,
                    pricing=pricing,
                    architecture=ModelArchitecture(**model.architecture),
                    top_provider=ModelProvider(**top_provider),
                    per_request_limits=ModelLimits(
                        prompt_tokens=model.capabilities.get("max_prompt_tokens"),
                        completion_tokens=model.capabilities.get(
                            "max_completion_tokens"
                        ),
                    ),
                )
                responses.append(response)

            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                # Convert to OpenAI format
                openai_models = [
                    OpenAIModel(
                        id=model.model_id,
                        created=1710979200,  # March 20, 2025
                        object="model",
                        owned_by=model.provider_id,
                    )
                    for model in models
                ]
                return OpenAIModelsResponse(object="list", data=openai_models)
            else:
                return ModelsResponse(data=responses)

        except Exception as e:
            self.logger.error(
                "Failed to get models",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                },
                exc_info=True,
            )
            raise
