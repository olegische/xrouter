"""Redis cache and rate limiting."""
import json
from typing import Any, Optional, Union, cast

from redis.asyncio import Redis

from .config import Settings
from .logger import LoggerService


class RedisClient:
    """Redis client for caching and rate limiting. When redis is None (ENABLE_CACHE=False), all ops are no-op."""

    def __init__(
        self,
        redis: Optional[Redis],
        logger: LoggerService,
        settings: Settings,
    ):
        """Initialize Redis client.

        Args:
            redis: Redis connection, or None when ENABLE_CACHE is False (stub mode).
            logger: Logger service instance.
            settings: Settings instance.
        """
        self.redis = redis
        self.settings = settings
        self.logger = logger.get_logger(__name__)
        if redis is None:
            self.logger.debug("RedisClient initialized (Redis disabled, stub mode)")
        else:
            self.logger.debug("RedisClient initialized")

    def _get_redis_key(self, key: str) -> str:
        """Get Redis key with prefix.

        Args:
            key: Redis key.

        Returns:
            str: Redis key with prefix.
        """
        return f"{self.settings.REDIS_PREFIX}:{key}"

    def _get_cache_key(self, key: str) -> str:
        """Get cache key with prefix.

        Args:
            key: Cache key.

        Returns:
            str: Cache key with prefix.
        """
        return f"{self.settings.REDIS_PREFIX}:{self.settings.CACHE_PREFIX}:{key}"

    def _get_rate_limit_key(self, api_key: str) -> str:
        """Get rate limit key for API key.

        Args:
            api_key: API key.

        Returns:
            str: Rate limit key.
        """
        return f"{self.settings.REDIS_PREFIX}:rate_limit:{api_key}"

    async def get(self, key: str) -> Optional[Any]:
        """Get value from Redis.

        Args:
            key: Redis key.

        Returns:
            Any: Value from Redis or None if key doesn't exist (or Redis disabled).
        """
        if self.redis is None:
            return None
        self.logger.debug(f"Getting value from Redis for key: {key}")
        value = await self.redis.get(key)
        if value is None:
            self.logger.debug(f"Key not found in Redis: {key}")
            return None
        self.logger.debug(f"Successfully retrieved value for key: {key}")
        return json.loads(value)

    async def set(
        self,
        key: str,
        value: Any,
        ex: Optional[int] = None,
    ) -> None:
        """Set value in Redis.

        Args:
            key: Redis key.
            value: Value to store.
            ex: Expiration time in seconds.
        """
        if self.redis is None:
            return
        self.logger.debug(f"Setting value in Redis for key: {key}")
        await self.redis.set(
            key,
            json.dumps(value),
            ex=ex,
        )
        self.logger.debug(f"Successfully set value for key: {key}")

    async def delete(self, key: str) -> None:
        """Delete value from Redis.

        Args:
            key: Redis key.
        """
        if self.redis is None:
            return
        self.logger.debug(f"Deleting value from Redis for key: {key}")
        await self.redis.delete(key)
        self.logger.debug(f"Successfully deleted value for key: {key}")

    async def close(self) -> None:
        """Close the Redis connection."""
        if self.redis is not None:
            await self.redis.close()

    async def ping(self) -> None:
        """Ping Redis to ensure it is reachable. No-op when Redis disabled."""
        if self.redis is None:
            return
        self.logger.debug("Pinging Redis")
        await self.redis.ping()
        self.logger.debug("Redis is reachable")

    async def increment_rate_limit(self, api_key: str) -> int:
        """Increment rate limit counter.

        Args:
            api_key: API key.

        Returns:
            int: Current rate limit count (1 when disabled, no persistence).
        """
        if self.redis is None:
            return 1
        key = self._get_rate_limit_key(api_key)
        self.logger.debug(f"Incrementing rate limit for API key: {api_key}")
        count: int = await self.redis.incr(key)  # type: ignore
        if count == 1:
            self.logger.debug(f"Setting expiration for rate limit key: {key}")
            await self.redis.expire(key, 60)  # 1 minute window
        self.logger.debug(f"Current rate limit count for {api_key}: {count}")
        return count

    async def get_rate_limit(self, api_key: str) -> int:
        """Get current rate limit count.

        Args:
            api_key: API key.

        Returns:
            int: Current rate limit count (0 when Redis disabled).
        """
        if self.redis is None:
            return 0
        key = self._get_rate_limit_key(api_key)
        self.logger.debug(f"Getting rate limit for API key: {api_key}")
        count: Optional[Union[bytes, str]] = await self.redis.get(key)
        result = int(count.decode() if isinstance(count, bytes) else count) if count else 0
        self.logger.debug(f"Current rate limit count for {api_key}: {result}")
        return result

    async def incrby(self, key: str, amount: int) -> int:
        """Increment value by amount.

        Args:
            key: Redis key.
            amount: Amount to increment by.

        Returns:
            int: New value after increment (amount when Redis disabled).
        """
        if self.redis is None:
            return amount
        full_key = self._get_redis_key(key)
        self.logger.debug(f"Incrementing value for key: {full_key} by {amount}")
        result = await self.redis.incrby(full_key, amount)
        self.logger.debug(f"New value after increment for {full_key}: {result}")
        return cast(int, result)

    async def cache_get(self, key: str) -> Optional[Any]:
        """Get value from cache.

        Args:
            key: Cache key.

        Returns:
            Any: Value from cache or None if key doesn't exist.
        """
        return await self.get(self._get_cache_key(key))

    async def cache_set(
        self,
        key: str,
        value: Any,
        expire: Optional[int] = None,
    ) -> None:
        """Set value in cache.

        Args:
            key: Cache key.
            value: Value to store.
            expire: Expiration time in seconds.
        """
        await self.set(
            self._get_cache_key(key),
            value,
            ex=expire or self.settings.CACHE_TTL,
        )

    async def cache_delete(self, key: str) -> None:
        """Delete value from cache.

        Args:
            key: Cache key.
        """
        await self.delete(self._get_cache_key(key))
