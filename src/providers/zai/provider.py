"""Z.AI provider implementation."""
import uuid
from typing import List

from core.cache import RedisClient
from core.logger import LoggerService
from ..base_mapper import BaseMapper
from ..base_model_mapper import BaseModelMapper
from ..models import ProviderConfig, ProviderError, ProviderModel
from ..xrouter.provider import XRouterProvider


class ZaiProvider(XRouterProvider):
    """Z.AI provider implementation.

    Extends XRouterProvider for OpenAI-compatible chat completions.
    Overrides get_models since Z.AI does not expose a models list API.
    """

    MODELS_CACHE_TTL = 3600  # 1 hour - static list, cache longer

    def __init__(
        self,
        cache: RedisClient,
        logger: LoggerService,
        provider: ProviderConfig,
        mapper: BaseMapper,
        model_mapper: BaseModelMapper,
    ) -> None:
        """Initialize Z.AI provider.

        Args:
            cache: Redis cache client
            logger: Logger service instance
            provider: Provider config
            mapper: Provider-specific mapper instance
            model_mapper: Provider-specific model mapper instance
        """
        super().__init__(
            cache=cache,
            logger=logger,
            provider=provider,
            mapper=mapper,
            model_mapper=model_mapper,
        )

    async def get_models(self) -> List[ProviderModel]:
        """Get list of available models.

        Z.AI does not expose a models API, so returns static list from
        model mapper. Results are cached.

        Returns:
            List of available provider models

        Raises:
            ProviderError: If models retrieval fails
        """
        request_id = str(uuid.uuid4())
        cache_key = f"models:{self._provider.provider_id}"

        # Try to get models from cache
        cached_models = await self.cache.cache_get(cache_key)
        if cached_models:
            self.logger.info(
                "Retrieved Z.AI models from cache",
                extra={
                    "request_id": request_id,
                    "count": len(cached_models),
                },
            )
            return [ProviderModel.model_validate(m) for m in cached_models]

        try:
            # Use static model list from model mapper (Z.AI has no models API)
            models = self.model_mapper.map_provider_models({})

            # Cache models
            await self.cache.cache_set(
                cache_key,
                [m.model_dump() for m in models],
                expire=self.MODELS_CACHE_TTL,
            )

            self.logger.info(
                "Retrieved Z.AI models (static list)",
                extra={
                    "request_id": request_id,
                    "count": len(models),
                },
            )
            return models

        except Exception as e:
            self.logger.error(
                "Failed to get Z.AI models",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message=f"Failed to get Z.AI models: {str(e)}",
                details={"error": str(e)},
            )
