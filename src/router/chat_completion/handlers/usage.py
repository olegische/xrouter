"""Usage recording handler."""
import time
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
from router.usage.models import (
    CalculateCostRequest,
    Currency,
    CreateGenerationRequest,
    CreateUsageRequest,
    FinalizeHoldWithTokensRequest,
    TokenCount,
)


class UsageRecordHandler(RequestHandler):
    """Record usage statistics."""

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
            "Initialized UsageRecordHandler",
            extra={
                "handler": "UsageRecordHandler",
            },
        )

    def canHandle(self, context: ChatContext) -> bool:
        """Check if handler can process the request.

        Args:
            context: Request context

        Returns:
            bool: True if context has final response and user ID
        """
        can_handle = bool(
            context.final_response is not None and context.api_key is not None
        )
        self.logger.debug(
            "Checking if can handle request",
            extra={
                "request_id": context.request_id,
                "has_final_response": bool(context.final_response),
                "has_api_key": bool(context.api_key),
                "can_handle": can_handle,
            },
        )
        return can_handle

    async def _record_metrics(
        self, context: ChatContext, usage_id: str, generation_id: str
    ) -> None:
        """Record metrics for the request.

        Args:
            context: Request context
            usage_id: ID of the created usage record
            generation_id: ID of the created generation record
        """
        if not context.user_id:
            self.logger.debug(
                "Skipping metrics recording - no user ID",
                extra={
                    "request_id": context.request_id,
                    "usage_id": usage_id,
                    "generation_id": generation_id,
                },
            )
            return

        if not context.provider_model:
            self.logger.error(
                "Missing provider model",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Provider model must be set in context",
                details={
                    "error": "Missing provider model",
                    "request_id": context.request_id,
                },
            )

        self.logger.debug(
            "Recording usage metrics",
            extra={
                "request_id": context.request_id,
                "model": context.provider_model.model_id,
                "provider": context.provider_model.provider_id,
                "user_id": context.user_id,
                "usage_id": str(usage_id),
                "generation_id": str(generation_id),
            },
        )

    async def _confirm_transaction(self, context: ChatContext) -> None:
        """Record usage statistics and finalize transaction.

        Args:
            context: Request context

        Raises:
            ProviderError: If usage recording fails
        """
        if not context.api_key:
            self.logger.error(
                "Missing user ID",
                extra={
                    "request_id": context.request_id,
                    "has_final_response": bool(context.final_response),
                },
            )
            raise ProviderError(
                code=400,
                message="User ID must be set in context",
                details={
                    "error": "Missing user ID",
                    "request_id": context.request_id,
                },
            )

        if not context.final_response:
            self.logger.error(
                "Missing final response",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Final response must be set in context",
                details={
                    "error": "Missing final response",
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                },
            )

        if not context.provider_model:
            self.logger.error(
                "Missing provider model",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Provider model must be set in context",
                details={
                    "error": "Missing provider model",
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                },
            )

        try:
            # Get token counts from native_usage if available, otherwise from final_response
            if context.native_usage:
                usage_data = context.native_usage
                self.logger.info(
                    "Using native usage data from context",
                    extra={
                        "request_id": context.request_id,
                        "native_usage": context.native_usage.model_dump(),
                    },
                )
            elif context.final_response and context.final_response.usage:
                usage_data = context.final_response.usage
                self.logger.debug(
                    "Using usage data from final response",
                    extra={
                        "request_id": context.request_id,
                        "has_final_response_usage": True,
                    },
                )
            else:
                raise ProviderError(
                    code=400,
                    message="Usage data must be present in context or final response",
                    details={
                        "error": "Missing usage data",
                        "request_id": context.request_id,
                    },
                )

            # Get cached tokens from prompt_tokens_details if available
            cache_hit = 0
            if (
                usage_data.prompt_tokens_details
                and usage_data.prompt_tokens_details.cached_tokens is not None
            ):
                cache_hit = usage_data.prompt_tokens_details.cached_tokens

            # Get reasoning tokens from completion_tokens_details if available
            output_reasoning = None
            if (
                usage_data.completion_tokens_details
                and usage_data.completion_tokens_details.reasoning_tokens is not None
            ):
                output_reasoning = usage_data.completion_tokens_details.reasoning_tokens

            # Prepare meta_info for provider-specific data
            token_meta_info = {}

            # Add cost from OpenRouter if available
            if usage_data.cost is not None:
                token_meta_info["cost"] = str(usage_data.cost)

            tokens = TokenCount(
                input=usage_data.prompt_tokens,
                output=usage_data.completion_tokens,
                total=usage_data.total_tokens,
                model=context.provider_model.external_model_id,
                provider=context.provider_model.provider_id,
                cache_hit=cache_hit,
                input_cached=context.cache_write,
                output_reasoning=output_reasoning,
                meta_info=token_meta_info,
            )

            self.logger.debug(
                "Processing final token counts",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "model": context.provider_model.external_model_id,
                    "input_tokens": tokens.input,
                    "output_tokens": tokens.output,
                    "total_tokens": tokens.total,
                    "cache_hit": tokens.cache_hit,
                    "input_cached": tokens.input_cached,
                },
            )

            # Calculate cost based on token usage
            cost = await self.usage_client.calculate_cost(
                CalculateCostRequest(
                    token_count=tokens,
                    api_key=context.api_key,
                    currency=context.currency or Currency.RUB,
                )
            )

            self.logger.debug(
                "Calculated final cost",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "model": context.provider_model.external_model_id,
                    "cost_amount": float(cost.cost.amount),
                    "currency": cost.cost.currency,
                },
            )

            # Check if generation_id exists in context
            if not context.generation_id:
                self.logger.error(
                    "Missing generation_id (transaction_id) in context",
                    extra={
                        "request_id": context.request_id,
                        "user_id": context.user_id,
                    },
                )
                raise ProviderError(
                    code=400,
                    message="Generation ID (transaction_id) must be set in context",
                    details={
                        "error": "Missing generation ID",
                        "request_id": context.request_id,
                    },
                )

            # Finalize hold with tokens only if amount was held (not free model)
            if context.on_hold and context.on_hold > 0.0:
                await self.usage_client.finalize_hold_with_tokens(
                    FinalizeHoldWithTokensRequest(
                        api_key=context.api_key,
                        token_count=tokens,
                        transaction_id=context.generation_id,
                    )
                )

                self.logger.debug(
                    "Finalized hold with tokens",
                    extra={
                        "request_id": context.request_id,
                        "user_id": context.user_id,
                        "transaction_id": context.generation_id,
                        "input_tokens": tokens.input,
                        "output_tokens": tokens.output,
                        "amount_held": context.on_hold,
                    },
                )
            else:
                # Free model - no hold to finalize
                self.logger.debug(
                    "Skipping hold finalization for free model",
                    extra={
                        "request_id": context.request_id,
                        "user_id": context.user_id,
                        "transaction_id": context.generation_id,
                        "input_tokens": tokens.input,
                        "output_tokens": tokens.output,
                        "amount_held": context.on_hold,
                    },
                )

            # Prepare meta info for usage record
            meta_info = {
                "user_id": str(context.user_id),
                "request_id": str(context.request_id),
            }

            # Add cost meta info if it exists
            if cost.cost.meta_info:
                meta_info.update(cost.cost.meta_info)

            # Create usage record using CreateUsageRequest
            usage_request = CreateUsageRequest(
                tokens=tokens,
                cost=cost.cost,
                meta_info=meta_info,
                api_key=context.api_key,
            )
            usage_response = await self.usage_client.create_usage(usage_request)
            usage_id = usage_response.data.id

            self.logger.debug(
                "Created usage record",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "usage_id": str(usage_id),
                    "model": context.provider_model.external_model_id,
                },
            )

            # Calculate generation metrics
            generation_time = time.time() - context.metadata.get(
                "start_time", time.time()
            )
            speed = tokens.total / generation_time if generation_time > 0 else 0

            # Get finish reason with fallback
            finish_reason = "unknown"

            # First check if finish_reason is stored in context metadata
            if context.metadata and "finish_reason" in context.metadata:
                finish_reason = str(context.metadata["finish_reason"])
                self.logger.debug(
                    "Using finish reason from metadata",
                    extra={
                        "request_id": context.request_id,
                        "finish_reason": finish_reason,
                    },
                )
            # Then check if it's in the final response
            elif (
                context.final_response
                and context.final_response.choices
                and len(context.final_response.choices) > 0
                and context.final_response.choices[0].finish_reason is not None
            ):
                finish_reason = str(context.final_response.choices[0].finish_reason)
                self.logger.debug(
                    "Using finish reason from final response",
                    extra={
                        "request_id": context.request_id,
                        "finish_reason": finish_reason,
                    },
                )
            else:
                self.logger.debug(
                    "Using default finish reason",
                    extra={
                        "request_id": context.request_id,
                        "finish_reason": finish_reason,
                    },
                )

            # Create generation record
            if not context.generation_id:
                raise ProviderError(
                    code=400,
                    message="Generation ID must be set in context",
                    details={
                        "error": "Missing generation ID",
                        "request_id": context.request_id,
                        "user_id": context.user_id,
                        "usage_id": str(usage_id),
                    },
                )

            # Create generation record using CreateGenerationRequest
            generation_request = CreateGenerationRequest(
                id=context.generation_id,
                model=context.provider_model.external_model_id,
                provider=context.provider_model.provider_id,
                origin=context.origin or "",
                generation_time=generation_time,
                speed=speed,
                finish_reason=finish_reason,
                native_finish_reason=finish_reason,
                is_streaming=context.request.stream if context.request else False,
                meta_info={
                    "request_id": str(context.request_id),
                    "stream": context.request.stream if context.request else False,
                    **(context.metadata or {}),
                },
                usage_id=str(usage_id),
                api_key=context.api_key,
            )
            await self.usage_client.create_generation(generation_request)

            self.logger.debug(
                "Created generation record",
                extra={
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "generation_id": str(context.generation_id),
                    "usage_id": str(usage_id),
                },
            )

            # Record metrics
            await self._record_metrics(
                context, str(usage_id), str(context.generation_id)
            )

            self.logger.info(
                "Successfully recorded usage and generation",
                extra={
                    "request_id": context.request_id,
                    "model": context.provider_model.external_model_id,
                    "total_tokens": tokens.total,
                    "generation_time": float(generation_time),
                    "speed": float(speed),
                    "usage_id": str(usage_id),
                    "generation_id": str(context.generation_id),
                    "cost_amount": float(cost.cost.amount),
                },
            )

        except ProviderError:
            # Re-raise provider errors without modification
            raise
        except Exception as e:
            self.logger.error(
                "Failed to record usage - internal error",
                extra={
                    "error": str(e),
                    "request_id": context.request_id,
                    "user_id": context.user_id,
                    "model": context.provider_model.external_model_id
                    if context.provider_model
                    else None,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to record usage statistics",
                details={
                    "error": str(e),
                    "model_slug": context.provider_model.external_model_id
                    if context.provider_model
                    else None,
                },
            )

    async def handleRequest(
        self,
        context: ChatContext,
        provider: Provider,  # Добавляем параметр provider для совместимости
    ) -> AsyncGenerator[Union[ProviderResponse, ProviderStreamChunk], None]:
        """Handle request by recording usage statistics.

        Args:
            context: Request context
            provider: Provider instance (not used in this handler)

        Yields:
            No responses at this step

        Raises:
            ProviderError: If usage recording fails
        """
        self.logger.info(
            "Usage handler called",
            extra={
                "request_id": context.request_id,
                "has_final_response": bool(context.final_response),
                "has_api_key": bool(context.api_key),
                "final_response_usage": context.final_response.usage.model_dump()
                if context.final_response and context.final_response.usage
                else None,
            },
        )

        if not self.canHandle(context):
            self.logger.warning(
                "Cannot handle request: missing required data",
                extra={
                    "request_id": context.request_id,
                    "has_final_response": bool(
                        getattr(context, "final_response", None)
                    ),
                    "has_api_key": bool(context.api_key),
                },
            )
            if False:
                yield  # for AsyncGenerator type hint
            return

        await self._confirm_transaction(context)

        if False:
            yield  # for AsyncGenerator type hint
