"""Handler chain factory for chat completion."""
from typing import Optional

from core.logger import LoggerService
from core.settings import Settings
from providers.manager import ProviderManager
from .handler_chain import RequestHandlerChain
from .handlers.base import RequestHandler
from .handlers.completion import CompletionHandler
from .handlers.limits import LimitCheckHandler
from .handlers.tokenize import TokenizeHandler
from .handlers.transform import RequestTransformHandler
from .handlers.usage import UsageRecordHandler
from router.usage.client import UsageClient


class HandlerChainFactory:
    """Factory for creating chat completion handler chains.

    The factory creates handler chains with handlers in a predefined order:
    1. Request transformation - Convert request to provider format and validate
    2. Tokenize request - Estimate token usage before provider call
    3. Limit check (optional) - Create hold for paid requests
    4. Completion execution - Execute request with provider
    5. Usage recording (optional) - Finalize hold and persist analytics
    """

    def __init__(
        self,
        logger: LoggerService,
        provider_manager: ProviderManager,
        settings: Settings,
        usage_client: Optional[UsageClient] = None,
    ) -> None:
        """Initialize factory.

        Args:
            logger: Logger service
            provider_manager: Provider manager service
            settings: Application settings

        Raises:
            ValueError: If any required service is missing
        """
        if not logger:
            raise ValueError("Logger service is required")
        if not provider_manager:
            raise ValueError("Provider manager is required")
        if not settings:
            raise ValueError("Settings is required")
        # These services are guaranteed to be non-None after the checks above
        self.logger = logger.get_logger(__name__)
        self.instance_logger = logger
        self.provider_manager = provider_manager
        self.settings = settings
        self.usage_client = usage_client

        self.logger.debug(
            "Initialized HandlerChainFactory",
            extra={
                "provider_manager": provider_manager.__class__.__name__,
                "has_usage_client": bool(usage_client),
            },
        )

    def create(self) -> RequestHandlerChain:
        """Create chat completion handler chain with standard handlers.

        Returns:
            Configured handler chain instance
        """
        self.logger.info(
            "Creating handler chain with standard handlers",
            extra={"factory": "HandlerChainFactory"},
        )

        self.logger.debug(
            "Initializing handlers in standard order", extra={"action": "init_handlers"}
        )

        handlers: list[RequestHandler] = [
            # 1. Transform and validate request
            RequestTransformHandler(
                logger=self.instance_logger,
                settings=self.settings,
            ),
            # 2. Calculate initial token count
            TokenizeHandler(
                logger=self.instance_logger,
            ),
        ]

        billing_enabled = bool(
            getattr(self.settings, "ENABLE_LLM_BILLING", False) and self.usage_client
        )
        if billing_enabled:
            self.logger.debug(
                "Billing handlers enabled",
                extra={"feature_flag": "ENABLE_LLM_BILLING"},
            )
            handlers.append(
                LimitCheckHandler(
                    logger=self.instance_logger,
                    usage_client=self.usage_client,
                )
            )
        else:
            self.logger.debug(
                "Billing handlers disabled",
                extra={
                    "feature_flag": "ENABLE_LLM_BILLING",
                    "has_usage_client": bool(self.usage_client),
                },
            )

        # Add completion handler
        handlers.append(
            CompletionHandler(
                logger=self.instance_logger,
                settings=self.settings,
            ),
        )

        if billing_enabled:
            handlers.append(
                UsageRecordHandler(
                    logger=self.instance_logger,
                    usage_client=self.usage_client,
                )
            )

        for idx, handler in enumerate(handlers, 1):
            self.logger.debug(
                "Adding handler to chain",
                extra={
                    "position": idx,
                    "handler_type": handler.__class__.__name__,
                    "total_handlers": len(handlers),
                },
            )

        chain = RequestHandlerChain(handlers, logger=self.instance_logger)
        self.logger.info(
            "Handler chain created successfully",
            extra={
                "total_handlers": len(handlers),
                "handler_types": [h.__class__.__name__ for h in handlers],
            },
        )

        return chain
