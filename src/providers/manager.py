"""Provider manager implementation."""
import re
from decimal import Decimal
from typing import List, Optional, Tuple, cast
from urllib.parse import urlparse

from core.cache import RedisClient
from core.logger import LoggerService
from core.settings import Settings
from .base import Provider
from .base_model_mapper import BaseModelMapper
from .factory import ProviderFactory
from .mapper_factory import MapperFactory
from .model_mapper_factory import ModelMapperFactory
from .models import ProviderConfig, ProviderError, ProviderModel
from router.base.providers import (
    PROVIDER_DEEPSEEK,
    PROVIDER_GIGACHAT,
    PROVIDER_NAMES,
    PROVIDER_OLLAMA,
    PROVIDER_OPENROUTER,
    PROVIDER_OPENROUTER_PROXY,
    PROVIDER_AGENTS,
    PROVIDER_XROUTER,
    PROVIDER_YANDEX,
    PROVIDER_ZAI,
    XROUTER_API_PROVIDERS,
)

# Cache TTL settings
MODEL_RATES_CACHE_TTL = 300  # 5 minutes


class ProviderManager:
    """Provider manager handles provider lifecycle."""

    def __init__(
        self,
        logger: LoggerService,
        settings: Settings,
        provider_factory: ProviderFactory,
        mapper_factory: MapperFactory,
        model_mapper_factory: ModelMapperFactory,
        cache: RedisClient,
    ) -> None:
        """Initialize provider manager.

        Args:
            logger: Logger service instance for logging operations
            settings: Application settings for configuration
            provider_factory: Factory for creating provider instances
            mapper_factory: Factory for creating provider-specific mappers
            model_mapper_factory: Factory for creating provider-specific model mappers
            cache: Redis cache client for caching operations
        """
        self.logger_service = logger  # Store original logger service for injection
        self.logger = logger.get_logger(__name__)  # Get logger instance for this class
        self.settings = settings
        self.provider_factory = provider_factory
        self.mapper_factory = mapper_factory
        self.model_mapper_factory = model_mapper_factory
        self.cache = cache

    def _is_provider_enabled(self, provider_alias: str) -> bool:
        """Check if provider is enabled by feature toggle.

        Args:
            provider_alias: Provider alias

        Returns:
            bool: True if provider is enabled, False otherwise
        """
        toggle_map = {
            PROVIDER_XROUTER: self.settings.ENABLE_XROUTER,
            PROVIDER_AGENTS: self.settings.ENABLE_AGENTS,
            PROVIDER_DEEPSEEK: self.settings.ENABLE_DEEPSEEK,
            PROVIDER_OPENROUTER: self.settings.ENABLE_OPENROUTER,
            PROVIDER_OPENROUTER_PROXY: self.settings.ENABLE_OPENROUTER_PROXY,
            PROVIDER_GIGACHAT: self.settings.ENABLE_GIGACHAT,
            PROVIDER_YANDEX: self.settings.ENABLE_YANDEX,
            PROVIDER_OLLAMA: self.settings.ENABLE_OLLAMA,
            PROVIDER_ZAI: self.settings.ENABLE_ZAI,
        }
        return bool(toggle_map.get(provider_alias, False))

    @staticmethod
    def normalize_model_id(model_id: str) -> str:
        """Normalize model ID to standard format.

        Args:
            model_id: Raw model ID string

        Returns:
            Normalized model ID in lowercase with standardized separators
        """
        # Convert to lowercase
        normalized = model_id.lower()

        # Replace spaces with hyphens
        normalized = normalized.replace(" ", "-")

        # Replace multiple hyphens with single hyphen
        while "--" in normalized:
            normalized = normalized.replace("--", "-")

        # Remove leading/trailing hyphens
        normalized = normalized.strip("-")

        return normalized

    def _parse_ollama_urls_and_keys(self) -> List[Tuple[str, str]]:
        """Parse semicolon-separated URLs and API keys.

        Returns:
            List of tuples (url, api_key)

        Examples:
            OLLAMA_BASE_URLS=http://185.70.104.22:11434
            OLLAMA_BASE_URLS=http://185.70.104.22:11434;http://185.70.104.23:11434
            OLLAMA_API_KEYS=key1;key2
        """
        urls = (
            [url.strip() for url in self.settings.OLLAMA_BASE_URLS.split(";")]
            if self.settings.OLLAMA_BASE_URLS
            else []
        )
        api_keys = (
            [key.strip() for key in self.settings.OLLAMA_API_KEYS.split(";")]
            if self.settings.OLLAMA_API_KEYS
            else []
        )

        # If no API keys provided, use empty strings
        if not api_keys:
            api_keys = ["" for _ in urls]
        # If fewer API keys than URLs, pad with empty strings
        elif len(api_keys) < len(urls):
            api_keys.extend(["" for _ in range(len(urls) - len(api_keys))])

        return list(zip(urls, api_keys))

    def _parse_ollama_model_id(self, external_model_id: str) -> Tuple[str, str]:
        """Parse Ollama model ID to extract server and model information.

        Args:
            external_model_id: External model ID in format ollama@server[:port]/model_id

        Returns:
            Tuple of (server_url, model_id)

        Raises:
            ProviderError: If model ID format is invalid
        """
        pattern = r"^ollama@([^/]+)/(.+)$"
        match = re.match(pattern, external_model_id)
        if not match:
            raise ProviderError(
                code=400,
                message=f"Invalid Ollama model ID format: {external_model_id}",
                details={"external_model_id": external_model_id},
            )

        server, model_id = match.groups()

        # Add protocol if not present
        if not server.startswith(("http://", "https://")):
            server = f"http://{server}"

        # Parse URL to validate format
        try:
            parsed = urlparse(server)
            if not parsed.netloc:
                raise ValueError("Invalid server URL")
            server_url = server
        except ValueError as e:
            raise ProviderError(
                code=400,
                message=f"Invalid server URL in model ID: {server}",
                details={"error": str(e)},
            )

        return server_url, model_id

    def _get_credentials_from_settings(self, provider_alias: str) -> str:
        """Get provider credentials from settings.

        Args:
            provider_alias: Provider alias

        Returns:
            Provider credentials from settings
        """
        credentials_registry = {
            PROVIDER_XROUTER: self.settings.XROUTER_API_KEY,
            PROVIDER_AGENTS: self.settings.AGENTS_API_KEY,
            PROVIDER_DEEPSEEK: self.settings.DEEPSEEK_API_KEY,
            PROVIDER_OPENROUTER: self.settings.OPENROUTER_API_KEY,
            PROVIDER_OPENROUTER_PROXY: self.settings.OPENROUTER_API_KEY,
            PROVIDER_YANDEX: self.settings.YANDEX_API_KEY,
            PROVIDER_GIGACHAT: self.settings.GIGACHAT_API_KEY or (
                f"{self.settings.GIGACHAT_LOGIN}:{self.settings.GIGACHAT_PASSWORD}"
                if self.settings.GIGACHAT_LOGIN and self.settings.GIGACHAT_PASSWORD
                else ""
            ),
            PROVIDER_ZAI: self.settings.ZAI_API_KEY,
        }
        return credentials_registry.get(provider_alias, "")

    def _get_base_url_from_settings(self, provider_alias: str) -> str:
        """Get provider base URL from settings.

        Args:
            provider_alias: Provider alias

        Returns:
            Provider base URL from settings
        """
        base_url_map = {
            PROVIDER_XROUTER: self.settings.XROUTER_BASE_URL,
            PROVIDER_AGENTS: self.settings.AGENTS_BASE_URL,
            PROVIDER_DEEPSEEK: self.settings.DEEPSEEK_BASE_URL,
            PROVIDER_OPENROUTER: self.settings.OPENROUTER_BASE_URL,
            PROVIDER_OPENROUTER_PROXY: self.settings.OPENROUTER_BASE_URL,
            PROVIDER_GIGACHAT: self.settings.GIGACHAT_BASE_URL,
            PROVIDER_YANDEX: self.settings.YANDEX_BASE_URL,
            PROVIDER_ZAI: self.settings.ZAI_BASE_URL,
        }
        return base_url_map.get(provider_alias, "")

    def _get_parameters_from_settings(self, provider_alias: str) -> dict:
        """Get provider parameters from settings.

        Args:
            provider_alias: Provider alias

        Returns:
            Provider parameters from settings
        """
        # Common parameters for all providers
        params = {
            "timeout": self.settings.PROVIDER_TIMEOUT,
            "verify_ssl": not self.settings.DISABLE_SSL_VERIFICATION,
        }

        # Provider-specific parameters
        if provider_alias == PROVIDER_YANDEX:
            params.update(
                {
                    "api_key_id": self.settings.YANDEX_API_KEY_ID,
                    "folder_id": self.settings.YANDEX_FOLDER_ID,
                }
            )
        elif provider_alias == PROVIDER_OPENROUTER_PROXY:
            # Extract HTTP port from the HTTP/SOCKS5 port string
            http_port = self.settings.OPENROUTER_PROXY_HTTP_SOCKS5_PORT.split("/")[0]
            params.update(
                {
                    "proxy_url": f"{self.settings.OPENROUTER_PROXY_BASE_URL}:{http_port}",
                    "proxy_user": self.settings.OPENROUTER_PROXY_USER,
                    "proxy_password": self.settings.OPENROUTER_PROXY_PASSWORD,
                    "proxy_scheme": self.settings.OPENROUTER_PROXY_SCHEME,
                }
            )

        return params

    def _create_provider_config_from_settings(
        self, provider_alias: str
    ) -> ProviderConfig:
        """Create provider config from settings.

        Args:
            provider_alias: Provider alias

        Returns:
            Provider configuration
        """
        return ProviderConfig(
            provider_id=provider_alias,
            name=PROVIDER_NAMES.get(provider_alias, provider_alias.capitalize()),
            credentials=self._get_credentials_from_settings(provider_alias),
            parameters=self._get_parameters_from_settings(provider_alias),
            base_url=self._get_base_url_from_settings(provider_alias),
        )

    async def _build_provider(
        self, provider_config: ProviderConfig
    ) -> Tuple[Provider, BaseModelMapper]:
        """Create provider instance with its mappers.

        Args:
            provider_config: Provider configuration

        Returns:
            Tuple of (provider, model_mapper)
        """
        model_mapper = self.model_mapper_factory.create(provider=provider_config)
        mapper = self.mapper_factory.create(provider=provider_config)
        provider = self.provider_factory.create(
            provider=provider_config,
            mapper=mapper,
            model_mapper=model_mapper,
        )
        return provider, model_mapper

    async def _process_models(
        self,
        models: List[ProviderModel],
        provider_alias: str,
        server_id: Optional[str] = None,
    ) -> List[ProviderModel]:
        """Process and cache models from a provider.

        Args:
            models: List of models to process
            provider_alias: Provider alias
            server_id: Optional server identifier for Ollama

        Returns:
            List of processed models
        """
        processed_models = []
        for model in models:
            # Use model_id directly since it already includes version if needed
            normalized_id = self.normalize_model_id(model.model_id)

            # For OpenAI compatible API, use just the model ID
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                model.external_model_id = normalized_id
            else:
                # For LLM Gateway API, include provider prefix
                if provider_alias == PROVIDER_OLLAMA:
                    model.external_model_id = (
                        f"{provider_alias}@{server_id}/{normalized_id}"
                    )
                else:
                    model.external_model_id = f"{provider_alias}/{normalized_id}"

            processed_models.append(model)

        return processed_models

    async def get_models(self) -> List[ProviderModel]:
        """Get models from providers based on API type.

        When OpenAI compatible API is enabled, returns only xrouter provider models.
        Otherwise, returns models from all enabled providers.

        Returns:
            List of models from enabled providers

        Raises:
            ProviderError: If operation fails
        """
        self.logger.debug("Getting models based on API type")

        try:
            all_models = []

            # When OpenAI compatible API is enabled, use only
            # agents provider
            providers_to_check = (
                [PROVIDER_AGENTS]
                if self.settings.ENABLE_OPENAI_COMPATIBLE_API
                else XROUTER_API_PROVIDERS
            )

            # Get models from each enabled provider
            for provider_alias in providers_to_check:
                # Log provider state before attempting to get models
                provider_state = (
                    "enabled"
                    if self._is_provider_enabled(provider_alias)
                    else "disabled"
                )
                self.logger.info(
                    f"Provider {provider_alias} state check",
                    extra={
                        "provider_alias": provider_alias,
                        "state": provider_state,
                        "settings": {
                            "ENABLE_AGENTS": self.settings.ENABLE_AGENTS,
                            "ENABLE_XROUTER": self.settings.ENABLE_XROUTER,
                            "ENABLE_DEEPSEEK": self.settings.ENABLE_DEEPSEEK,
                            "ENABLE_OPENROUTER": self.settings.ENABLE_OPENROUTER,
                            "ENABLE_OPENROUTER_PROXY": self.settings.ENABLE_OPENROUTER_PROXY,
                            "ENABLE_GIGACHAT": self.settings.ENABLE_GIGACHAT,
                            "ENABLE_YANDEX": self.settings.ENABLE_YANDEX,
                            "ENABLE_OLLAMA": self.settings.ENABLE_OLLAMA,
                            "ENABLE_ZAI": self.settings.ENABLE_ZAI,
                        },
                    },
                )

                if not self._is_provider_enabled(provider_alias):
                    self.logger.info(
                        f"Provider {provider_alias} is disabled by feature toggle",
                        extra={"provider_alias": provider_alias},
                    )
                    continue

                try:
                    if provider_alias == PROVIDER_OLLAMA:
                        # Handle multiple Ollama servers
                        ollama_models = []
                        urls_and_keys = self._parse_ollama_urls_and_keys()
                        self.logger.info(
                            "Parsed Ollama URLs and API keys",
                            extra={
                                "urls_count": len(urls_and_keys),
                                "raw_urls": self.settings.OLLAMA_BASE_URLS,
                            },
                        )
                        for base_url, api_key in urls_and_keys:
                            try:
                                # Create provider config for this server
                                provider_config = ProviderConfig(
                                    provider_id=PROVIDER_OLLAMA,
                                    name=PROVIDER_NAMES[PROVIDER_OLLAMA],
                                    credentials=api_key,
                                    parameters={
                                        "timeout": self.settings.PROVIDER_TIMEOUT
                                    },
                                    base_url=base_url,
                                )

                                # Create provider and get models
                                self.logger.debug(
                                    "Creating Ollama provider instance",
                                    extra={
                                        "base_url": base_url,
                                        "timeout": provider_config.parameters.get(
                                            "timeout"
                                        ),
                                    },
                                )
                                provider, _ = await self._build_provider(
                                    provider_config
                                )
                                self.logger.debug("Getting models from Ollama provider")
                                server_models = await provider.get_models()
                                self.logger.debug(
                                    "Received models from Ollama provider",
                                    extra={
                                        "base_url": base_url,
                                        "models_count": len(server_models),
                                        "raw_model_ids": [
                                            m.model_id for m in server_models
                                        ],
                                    },
                                )

                                # Parse server URL for model ID generation
                                parsed_url = urlparse(base_url)
                                server_id = parsed_url.netloc

                                # Process and cache models
                                processed_models = await self._process_models(
                                    server_models, provider_alias, server_id
                                )
                                ollama_models.extend(processed_models)

                                self.logger.info(
                                    "Successfully retrieved models from Ollama server",
                                    extra={
                                        "server_url": base_url,
                                        "server_id": server_id,
                                        "models_count": len(processed_models),
                                        "model_ids": [
                                            m.model_id for m in processed_models
                                        ],
                                    },
                                )

                            except Exception as e:
                                self.logger.error(
                                    f"Failed to get models from Ollama server {base_url}",
                                    extra={
                                        "error": str(e),
                                        "error_type": type(e).__name__,
                                        "base_url": base_url,
                                    },
                                )
                                continue

                        # Add Ollama models to the list
                        if ollama_models:
                            all_models.extend(ollama_models)
                            self.logger.info(
                                "Retrieved and cached models from provider",
                                extra={"provider_alias": provider_alias},
                            )

                    else:
                        # Handle other providers
                        provider_config = self._get_provider_by_alias(provider_alias)
                        self.logger.debug(
                            f"Creating {provider_alias} provider instance",
                            extra={
                                "provider_id": provider_config.provider_id,
                                "base_url": provider_config.base_url,
                                "timeout": provider_config.parameters.get("timeout"),
                            },
                        )
                        provider, _ = await self._build_provider(provider_config)
                        self.logger.debug(
                            f"Getting models from {provider_alias} provider"
                        )
                        models = await provider.get_models()
                        self.logger.debug(
                            f"Received models from {provider_alias} provider",
                            extra={
                                "provider_id": provider_config.provider_id,
                                "base_url": provider_config.base_url,
                                "models_count": len(models),
                                "raw_model_ids": [m.model_id for m in models],
                            },
                        )

                        # Process and cache models
                        processed_models = await self._process_models(
                            models, provider_alias
                        )

                        # Add provider models to the list if any were retrieved
                        if processed_models:
                            all_models.extend(processed_models)
                            self.logger.info(
                                "Retrieved and cached models from provider",
                                extra={
                                    "provider_alias": provider_alias,
                                    "models_count": len(processed_models),
                                    "model_ids": [m.model_id for m in processed_models],
                                    "base_url": provider_config.base_url,
                                },
                            )

                except Exception as e:
                    self.logger.error(
                        f"Failed to get models from provider {provider_alias}",
                        extra={
                            "error": str(e),
                            "error_type": type(e).__name__,
                            "provider_alias": provider_alias,
                        },
                    )
                    # Continue with other providers even if one fails
                    continue

            self.logger.info(
                "Retrieved all models",
                extra={"total_count": len(all_models)},
            )

            return all_models

        except Exception as e:
            self.logger.error(
                "Failed to get models",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                },
            )
            raise ProviderError(
                code=500,
                message="Error while getting models",
                details={
                    "error": str(e),
                },
            )

    def _get_provider_by_alias(self, provider_alias: str) -> ProviderConfig:
        """Get provider by alias (internal use only).

        Args:
            provider_alias: Provider alias (e.g. 'gigachat', 'yandex', 'xrouter')

        Returns:
            Provider configuration

        Raises:
            ProviderError: If provider is not found or disabled
        """
        # Check if provider is enabled
        if not self._is_provider_enabled(provider_alias):
            error_msg = f"Provider {provider_alias} is disabled by feature toggle"
            self.logger.error(
                error_msg,
                extra={"provider_alias": provider_alias},
            )
            raise ProviderError(
                code=403,
                message=error_msg,
                details={"provider_alias": provider_alias},
            )

        # Create provider config from settings
        provider_config = self._create_provider_config_from_settings(provider_alias)

        self.logger.info(
            "Found provider",
            extra={"provider_id": provider_config.provider_id},
        )

        return provider_config

    def get_provider_by_model_id(
        self, external_model_id: str
    ) -> Tuple[ProviderConfig, str]:
        """Get provider configuration by external model ID.

        For OpenAI compatible API, always returns xrouter provider with the model_id as is.
        For LLM Gateway API, expects provider/model:version format or ollama@server[:port]/model_id.

        Args:
            external_model_id: External model ID in format provider/model:version
                             or ollama@server[:port]/model_id for Ollama models

        Returns:
            Provider configuration and model ID

        Raises:
            ProviderError: If provider is not found or disabled
        """
        self.logger.debug(
            "Looking up provider by model ID",
            extra={"external_model_id": external_model_id},
        )

        try:
            # For OpenAI compatible API, always use agents provider
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                return (
                    self._get_provider_by_alias(PROVIDER_AGENTS),
                    external_model_id,
                )

            # For LLM Gateway API, parse provider from model ID
            if "@" in external_model_id:  # Ollama format
                server_url, model_id = self._parse_ollama_model_id(external_model_id)
                provider_alias = PROVIDER_OLLAMA
            else:
                parts = external_model_id.split("/", 1)
                if len(parts) != 2:
                    raise ProviderError(
                        code=400,
                        message=f"Invalid model ID format: {external_model_id}",
                        details={"external_model_id": external_model_id},
                    )
                provider_alias = parts[0]
                model_id = parts[1]

            # Check if provider is enabled
            if not self._is_provider_enabled(provider_alias):
                error_msg = f"Provider {provider_alias} is disabled by feature toggle"
                self.logger.error(
                    error_msg,
                    extra={"provider_alias": provider_alias},
                )
                raise ProviderError(
                    code=403,
                    message=error_msg,
                    details={"provider_alias": provider_alias},
                )

            if provider_alias == PROVIDER_OLLAMA:
                # Find matching API key for the server URL
                urls_and_keys = self._parse_ollama_urls_and_keys()
                api_key = ""
                for url, key in urls_and_keys:
                    if url == server_url:
                        api_key = key
                        break

                provider_dict = {
                    "provider_id": PROVIDER_OLLAMA,
                    "name": PROVIDER_NAMES[PROVIDER_OLLAMA],
                    "credentials": api_key,
                    "parameters": {"timeout": self.settings.PROVIDER_TIMEOUT},
                    "base_url": server_url,  # Use server URL from model ID
                }
            else:
                # For non-Ollama providers, use the internal method
                provider_config = self._get_provider_by_alias(provider_alias)
                return provider_config, model_id

            self.logger.info(
                "Found provider",
                extra={
                    "provider_id": provider_dict["provider_id"],
                    "model_id": model_id,
                },
            )

            provider_config = cast(
                ProviderConfig, ProviderConfig.model_validate(provider_dict)
            )
            return provider_config, model_id

        except Exception as e:
            if isinstance(e, ProviderError):
                raise

            self.logger.error(
                "Failed to get provider",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                    "external_model_id": external_model_id,
                },
            )
            raise ProviderError(
                code=500,
                message="Error while looking up provider",
                details={
                    "error": str(e),
                    "external_model_id": external_model_id,
                },
            )
