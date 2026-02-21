"""Token calculation handler."""
from typing import AsyncGenerator, Union

from core.logger import LoggerService
from providers.base import Provider
from providers.models import (
    ProviderError,
    ProviderResponse,
    ProviderStreamChunk,
)
from .base import RequestHandler
from ..models import ChatContext
from router.usage.models import TokenCount


class TokenizeHandler(RequestHandler):
    """Calculate expected tokens for the request.

    Args:
        logger: Logger service instance for logging
        metrics: Metrics service instance for recording metrics
        tokenizer_service: Service for calculating tokens
    """

    def __init__(
        self,
        logger: LoggerService,
    ) -> None:
        """Initialize handler.

        Args:
            logger: Logger service instance for logging
            metrics: Metrics service instance for recording metrics
            tokenizer_service: Service for calculating tokens
        """
        self.logger = logger.get_logger(__name__)
        self.logger.debug(
            "Initialized TokenCalculationHandler",
            extra={
                "handler": "TokenCalculationHandler",
            },
        )

    def canHandle(self, context: ChatContext) -> bool:
        """Check if handler can process the request.

        Args:
            context: Request context

        Returns:
            bool: True if context has provider request
        """
        can_handle = bool(
            context.provider_model and context.provider_request is not None
        )
        self.logger.debug(
            "Checking if can handle request",
            extra={
                "request_id": context.request_id,
                "has_provider_model": bool(context.provider_model),
                "has_provider_request": bool(context.provider_request),
                "can_handle": can_handle,
            },
        )
        return can_handle

    async def _tokenize(self, context: ChatContext) -> None:
        """Calculate tokens.

        Args:
            context: Request context

        Raises:
            ProviderError: If limits are exceeded or check fails
        """
        self.logger.info(
            "Starting token calculation",
            extra={
                "request_id": context.request_id,
                "model": context.provider_model.external_model_id
                if context.provider_model
                else None,
            },
        )

        try:
            if not context.api_key:
                raise ProviderError(
                    code=400,
                    message="API key must be set in context",
                    details={
                        "error": "Missing API key",
                        "request_id": context.request_id,
                    },
                )

            if not context.user_id:
                raise ProviderError(
                    code=400,
                    message="User ID must be set in context",
                    details={
                        "error": "Missing user ID",
                        "request_id": context.request_id,
                    },
                )

            if not context.provider_model:
                raise ProviderError(
                    code=400,
                    message="Provider model must be set in context",
                    details={
                        "error": "Missing provider model",
                        "request_id": context.request_id,
                    },
                )

            # For xrouter provider, calculate tokens directly using tokenizer_service
            if (
                context.provider_model
                and context.provider_model.provider_id == "xrouter"
            ):
                # Calculate input tokens using tokenizer service
                input_tokens = 1000  # default value
                if (
                    context.provider_request
                    and context.provider_request.max_tokens is not None
                ):
                    input_tokens = context.provider_request.max_tokens

                # Use max_tokens for output tokens or default
                output_tokens = 1000  # default value
                if (
                    context.provider_request
                    and context.provider_request.max_tokens is not None
                ):
                    output_tokens = context.provider_request.max_tokens

                tokens = TokenCount(
                    model=context.provider_model.external_model_id,
                    provider=context.provider_model.provider_id,
                    input=input_tokens,
                    output=output_tokens,
                    total=input_tokens + output_tokens,
                )
            else:
                # For other providers, use their own token calculation
                input_tokens = 1000  # default value
                if (
                    context.provider_request
                    and context.provider_request.max_tokens is not None
                ):
                    input_tokens = context.provider_request.max_tokens

                # Use max_tokens for output tokens or default
                output_tokens = 1000  # default value
                if (
                    context.provider_request
                    and context.provider_request.max_tokens is not None
                ):
                    output_tokens = context.provider_request.max_tokens

                tokens = TokenCount(
                    model=context.provider_model.external_model_id,
                    provider=context.provider_model.provider_id,
                    input=input_tokens,
                    output=output_tokens,
                    total=input_tokens + output_tokens,
                )
            if not tokens:
                raise ProviderError(
                    code=500,
                    message="Token calculation returned None",
                    details={
                        "error": "Token calculation failed",
                        "model_slug": context.provider_model.external_model_id,
                    },
                )

            # Replace model with external_model_id
            if (
                context.provider_model
                and context.provider_model.external_model_id is not None
            ):
                tokens.model = context.provider_model.external_model_id

            context.expected_tokens = tokens
            self.logger.info(
                "Token calculation completed",
                extra={
                    "request_id": context.request_id,
                    "model": context.provider_model.external_model_id,
                    "input_tokens": tokens.input,
                    "output_tokens": tokens.output,
                },
            )

        except ProviderError:
            raise  # Re-raise ProviderError as is
        except Exception as e:
            self.logger.error(
                "Failed to calculate tokens",
                extra={
                    "error": str(e),
                    "request_id": context.request_id,
                    "model": context.provider_model.external_model_id
                    if context.provider_model
                    else None,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to calculate tokens",
                details={
                    "error": str(e),
                    "model_slug": context.provider_model.external_model_id
                    if context.provider_model
                    else None,
                },
            )

    async def handleRequest(  # noqa: C901
        self,
        context: ChatContext,
        provider: Provider,
    ) -> AsyncGenerator[Union[ProviderResponse, ProviderStreamChunk], None]:
        """Handle request by calculating expected tokens.

        Args:
            context: Request context
            provider: Provider instance for token calculation

        Yields:
            No responses at this step

        Raises:
            ProviderError: If token calculation fails
        """
        if not self.canHandle(context):
            self.logger.warning(
                "Cannot handle request: missing provider request",
                extra={
                    "request_id": context.request_id,
                    "has_provider_model": bool(context.provider_model),
                    "has_provider_request": bool(context.provider_request),
                },
            )
            if False:
                yield  # for AsyncGenerator type hint
            return

        await self._tokenize(context)

        if False:
            yield  # for AsyncGenerator type hint
