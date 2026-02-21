"""Usage limits check handler."""
from typing import AsyncGenerator, Union

from core.logger import LoggerService
from providers.base import Provider
from providers.models import (
    ProviderError,
    ProviderResponse,
    ProviderStreamChunk,
)
from router.chat_completion.handlers.base import RequestHandler
from router.chat_completion.models import ChatContext
from router.usage.client import UsageClient
from router.usage.models import ProcessCostWithTokensRequest, TokenCount


class LimitCheckHandler(RequestHandler):
    """Check usage limits."""

    def __init__(
        self,
        logger: LoggerService,
        usage_client: UsageClient,
    ) -> None:
        """Initialize handler.

        Args:
            logger: Logger service
            usage_client: Usage client for billing operations
        """
        self.logger = logger.get_logger(__name__)
        self.usage_client = usage_client
        self.logger.debug(
            "Initialized LimitCheckHandler",
            extra={
                "handler": "LimitCheckHandler",
            },
        )

    def canHandle(self, context: ChatContext) -> bool:
        """Check if handler can process the request.

        Args:
            context: Request context

        Returns:
            bool: True if API key is present, limits need checking,
                   and token counts are available
        """
        can_handle = bool(
            context.api_key is not None
            and context.user_id is not None
            and context.on_hold is None
            and context.expected_tokens is not None
        )
        self.logger.debug(
            "Checking if can handle request",
            extra={
                "request_id": context.request_id,
                "has_api_key": bool(context.api_key),
                "has_user_id": bool(context.user_id),
                "has_on_hold": bool(context.on_hold),
                "has_expected_tokens": bool(context.expected_tokens),
                "can_handle": can_handle,
            },
        )
        return can_handle

    async def _check_limits(self, context: ChatContext) -> None:
        """Check usage limits.

        Args:
            context: Request context

        Raises:
            ProviderError: If limits are exceeded or check fails
        """
        self.logger.info(
            "Starting usage limit check",
            extra={
                "request_id": context.request_id,
                "user_id": context.user_id,
                "model": context.provider_model.model_id
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

            if not context.expected_tokens:
                raise ProviderError(
                    code=400,
                    message="Expected tokens must be set in context",
                    details={
                        "error": "Missing expected tokens",
                        "provider_name": context.provider_model.provider_id,
                        "model_slug": context.provider_model.model_id,
                    },
                )

            # Create token count object
            tokens = TokenCount(
                input=context.expected_tokens.input,
                output=context.expected_tokens.output,
                total=context.expected_tokens.total,
                model=context.provider_model.external_model_id,
                provider=context.provider_model.provider_id,
            )

            self.logger.debug(
                "Processing token counts for hold",
                extra={
                    "request_id": context.request_id,
                    "expected_input_tokens": tokens.input,
                    "expected_output_tokens": tokens.output,
                    "expected_total_tokens": tokens.total,
                },
            )

            try:
                # Hold tokens directly without calculating cost first
                response = await self.usage_client.process_cost_with_tokens(
                    ProcessCostWithTokensRequest(
                        api_key=context.api_key, token_count=tokens
                    )
                )
                context.on_hold = response.amount_held

                # Store the transaction_id as generation_id in context
                context.generation_id = response.transaction_id

                self.logger.debug(
                    "Stored transaction ID as generation_id",
                    extra={
                        "request_id": context.request_id,
                        "transaction_id": response.transaction_id,
                        "generation_id": context.generation_id,
                    },
                )

                # Check if amount_held is None (error case) vs 0.0 (free model case)
                if context.on_hold is None:
                    self.logger.warning(
                        "Usage limit exceeded - no amount held",
                        extra={
                            "request_id": context.request_id,
                            "user_id": context.user_id,
                            "model": context.provider_model.model_id
                            if context.provider_model
                            else None,
                            "input_tokens": context.expected_tokens.input,
                            "output_tokens": context.expected_tokens.output,
                        },
                    )
                    raise ProviderError(
                        code=402,
                        message="Usage limit exceeded",
                        details={
                            "error": "Insufficient funds",
                            "error_type": "payment_required",
                            "provider_name": context.provider_model.provider_id,
                            "model_slug": context.provider_model.model_id,
                            "tokens": context.expected_tokens.model_dump(),
                        },
                    )
                elif context.on_hold == 0.0:
                    # Free model - amount_held is 0.0, which is normal
                    self.logger.debug(
                        "Free model detected - no funds held",
                        extra={
                            "request_id": context.request_id,
                            "user_id": context.user_id,
                            "model": context.provider_model.model_id
                            if context.provider_model
                            else None,
                            "amount_held": 0.0,
                        },
                    )
            except ProviderError as e:
                # Если это ошибка 402, обрабатываем её специальным образом
                if e.code == 402:
                    context.on_hold = None
                    self.logger.warning(
                        "Payment required error during cost processing",
                        extra={
                            "request_id": context.request_id,
                            "user_id": context.user_id,
                            "model": context.provider_model.model_id
                            if context.provider_model
                            else None,
                            "input_tokens": context.expected_tokens.input,
                            "output_tokens": context.expected_tokens.output,
                        },
                    )
                    # Создаем более понятное сообщение об ошибке для пользователя
                    raise ProviderError(
                        code=402,
                        message="Insufficient funds for request processing",
                        details={
                            "error": "Payment required",
                            "error_type": "payment_required",
                            "provider_name": context.provider_model.provider_id,
                            "model_slug": context.provider_model.model_id,
                            "tokens": context.expected_tokens.model_dump(),
                        },
                    )
                # Для других ошибок просто пробрасываем дальше
                raise

            self.logger.info(
                "Usage limit check passed",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "model": context.provider_model.model_id
                    if context.provider_model
                    else None,
                    "on_hold": float(context.on_hold),
                    "transaction_id": context.generation_id,
                },
            )

        except ProviderError:
            raise  # Re-raise ProviderError as is
        except Exception as e:
            self.logger.error(
                "Failed to check limits",
                extra={
                    "error": str(e),
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "provider": context.provider_model.provider_id
                    if context.provider_model
                    else None,
                    "model": context.provider_model.model_id
                    if context.provider_model
                    else None,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message=str(e),
                details={
                    "error": str(e),
                    "provider_name": context.provider_model.provider_id
                    if context.provider_model
                    else None,
                    "model_slug": context.provider_model.model_id
                    if context.provider_model
                    else None,
                },
            )

    async def handleRequest(
        self,
        context: ChatContext,
        provider: Provider,  # Добавляем параметр provider для совместимости
    ) -> AsyncGenerator[Union[ProviderResponse, ProviderStreamChunk], None]:
        """Handle request by checking usage limits.

        Args:
            context: Request context
            provider: Provider instance (not used in this handler)

        Yields:
            No responses at this step

        Raises:
            ProviderError: If limits are exceeded or check fails
        """
        if not self.canHandle(context):
            self.logger.warning(
                "Cannot handle request: missing required data",
                extra={
                    "request_id": context.request_id,
                    "has_user_id": bool(context.user_id),
                    "has_on_hold": bool(context.on_hold),
                    "has_expected_tokens": bool(context.expected_tokens),
                },
            )
            if False:
                yield  # for AsyncGenerator type hint
            return

        await self._check_limits(context)

        if False:
            yield  # for AsyncGenerator type hint
