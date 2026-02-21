"""Application settings."""
import json
from typing import List, Union

from pydantic import field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict

DEFAULT_OPENROUTER_MODELS: List[str] = [
    "anthropic/claude-haiku-4.5",
    "anthropic/claude-opus-4.5",
    "anthropic/claude-opus-4.6",
    "anthropic/claude-sonnet-4.5",
    "deepseek/deepseek-r1",
    "deepseek/deepseek-r1-0528",
    "deepseek/deepseek-r1-0528:free",
    "deepseek/deepseek-v3.2",
    "deepseek/deepseek-v3.2-exp",
    "deepseek/deepseek-v3.2-speciale",
    "google/gemini-2.5-flash",
    "google/gemini-2.5-flash-image",
    "google/gemini-2.5-flash-lite",
    "google/gemini-2.5-flash-lite-preview-09-2025",
    "google/gemini-2.5-flash-preview-09-2025",
    "google/gemini-2.5-pro",
    "google/gemini-2.5-pro-preview",
    "google/gemini-2.5-pro-preview-05-06",
    "google/gemini-3-flash-preview",
    "google/gemini-3-pro-image-preview",
    "google/gemini-3-pro-preview",
    "minimax/minimax-m2",
    "minimax/minimax-m2-her",
    "minimax/minimax-m2.1",
    "minimax/minimax-m2.5",
    "moonshotai/kimi-k2",
    "moonshotai/kimi-k2-0905",
    "moonshotai/kimi-k2-0905:exacto",
    "moonshotai/kimi-k2-thinking",
    "moonshotai/kimi-k2.5",
    "openai/gpt-5.2",
    "openai/gpt-5.2-chat",
    "openai/gpt-5.2-codex",
    "openai/gpt-5.2-pro",
    "x-ai/grok-4",
    "x-ai/grok-4-fast",
    "x-ai/grok-4.1-fast",
    "z-ai/glm-4.7",
    "z-ai/glm-4.7-flash",
    "z-ai/glm-5",
]


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        extra="ignore",
        env_file=".env",
        env_file_encoding="utf-8",
    )
    """Application settings."""

    # Environment
    ENVIRONMENT: str = "development"
    DEBUG: bool = False

    # Project
    PROJECT_NAME: str = "llm-gateway"
    VERSION: str = "0.1.0"

    # Host
    HOST: str = "0.0.0.0"
    PORT: int = 8900
    WORKERS_COUNT: int = 1
    SERVER_VERSION: str = "Undefined"

    # CORS
    BACKEND_CORS_ORIGINS: List[str] = ["*"]

    @field_validator("BACKEND_CORS_ORIGINS", mode="before")
    @classmethod
    def assemble_cors_origins(cls, v: Union[str, List[str]]) -> Union[List[str], str]:
        """Validate CORS origins."""
        if isinstance(v, str) and not v.startswith("["):
            return [i.strip() for i in v.split(",")]
        elif isinstance(v, (list, str)):
            return v
        raise ValueError(v)

    # Cache
    CACHE_TTL: int = 60 * 60  # 1 hour
    CACHE_PREFIX: str = "cache"

    # Redis
    ENABLE_CACHE: bool = False  # Feature toggle: when False, cache/rate-limit are no-op
    # Redis
    REDIS_HOST: str = "localhost"
    REDIS_PORT: int = 6379
    REDIS_DB: int = 0
    REDIS_USER: str = ""
    REDIS_PREFIX: str = ""
    REDIS_PASSWORD: str = ""

    @property
    def REDIS_URL(self) -> str:
        """Get Redis connection URL."""
        user_part = self.REDIS_USER or ""
        password_part = f":{self.REDIS_PASSWORD}" if self.REDIS_PASSWORD else ""
        
        credentials = ""
        if user_part or self.REDIS_PASSWORD:
            credentials = f"{user_part}{password_part}@"

        return f"redis://{credentials}{self.REDIS_HOST}:{self.REDIS_PORT}/{self.REDIS_DB}"

    # API Keys
    SERVICE_API_KEY: str = ""  # Service API key for authentication
    API_KEY_SALT: str = ""  # Salt for API key hashing

    # Provider Settings
    PROVIDER_TIMEOUT: int = 30  # seconds
    PROVIDER_MAX_RETRIES: int = 3

    # Feature Flags
    ENABLE_AUTH: bool = True  # Feature flag for authentication
    ENABLE_SERVICE_AUTH: bool = False  # Feature flag for service API key authentication
    ENABLE_AUTH_SERVICE: bool = False  # Feature flag for external auth service
    ENABLE_LLM_BILLING: bool = False  # Feature flag for usage billing flow
    DISABLE_SSL_VERIFICATION: bool = (
        False  # Feature flag to disable SSL certificate verification
    )

    # Auth Service Settings
    AUTH_SERVICE_URL: str = ""
    AUTH_SERVICE_TIMEOUT: int = 5  # seconds
    AUTH_SERVICE_API_KEY: str = ""  # JWT for service-to-service auth
    AUTH_SERVICE_CACHE_TTL: int = 900  # 15 minutes
    # Note: ENABLE_OPENAI_COMPATIBLE_API requires ENABLE_XINFERENCE to be True
    ENABLE_OPENAI_COMPATIBLE_API: bool = False  # Feature flag for OpenAI-compatible API
    ENABLE_SERVER_INFO_ENDPOINT: bool = False  # Feature flag for info/json endpoints
    ENABLE_AGENTS: bool = False  # Feature flag for LLM Gateway Agents provider
    ENABLE_XROUTER: bool = False  # Feature flag for LLM Gateway provider
    ENABLE_DEEPSEEK: bool = False  # Feature flag for Deepseek provider
    ENABLE_OPENROUTER: bool = False  # Feature flag for Openrouter provider
    ENABLE_OPENROUTER_PROXY: bool = False  # Feature flag for Openrouter Proxy provider
    ENABLE_GIGACHAT: bool = True  # Feature flag for GigaChat provider
    ENABLE_YANDEX: bool = False  # Feature flag for Yandex provider
    ENABLE_OLLAMA: bool = True  # Feature flag for Ollama provider
    ENABLE_ZAI: bool = False  # Feature flag for Z.AI provider

    # LLM Gateway Agents Settings
    AGENTS_API_KEY: str = ""
    AGENTS_BASE_URL: str = ""

    # XRouter Settings
    XROUTER_API_KEY: str = ""
    XROUTER_BASE_URL: str = "https://ai.xrouter.ru/api/v1"

    # Usage/Billing service settings
    XSERVER_API_KEY: str = ""
    XSERVER_BASE_URL: str = ""

    # DeepSeek Settings
    DEEPSEEK_API_KEY: str = ""
    DEEPSEEK_BASE_URL: str = "https://api.deepseek.com/v1"

    # OpenRouter Settings
    OPENROUTER_API_KEY: str = ""
    OPENROUTER_BASE_URL: str = "https://openrouter.ai/api/v1"
    OPENROUTER_SUPPORTED_MODELS: List[str] = DEFAULT_OPENROUTER_MODELS.copy()

    @field_validator("OPENROUTER_SUPPORTED_MODELS", mode="before")
    @classmethod
    def parse_openrouter_models(cls, v: Union[str, List[str]]) -> List[str]:
        """Parse OpenRouter supported models from JSON string or list."""
        if isinstance(v, str):
            try:
                parsed = json.loads(v)
                if isinstance(parsed, list):
                    return [str(item) for item in parsed]
                return []
            except json.JSONDecodeError:
                # Fallback to default models if JSON parsing fails
                return DEFAULT_OPENROUTER_MODELS.copy()
        elif isinstance(v, list):
            return [str(item) for item in v]
        return []

    # OpenRouter Proxy Settings
    OPENROUTER_PROXY_USER: str = ""
    OPENROUTER_PROXY_PASSWORD: str = ""
    OPENROUTER_PROXY_BASE_URL: str = ""
    OPENROUTER_PROXY_HTTP_SOCKS5_PORT: str = ""
    OPENROUTER_PROXY_SCHEME: str = "socks5"  # "socks5" | "http" | "https" (https = TLS to proxy)
    OPENROUTER_PROXY_SUPPORTED_MODELS: List[str] = DEFAULT_OPENROUTER_MODELS.copy()

    @field_validator("OPENROUTER_PROXY_SUPPORTED_MODELS", mode="before")
    @classmethod
    def parse_openrouter_proxy_models(cls, v: Union[str, List[str]]) -> List[str]:
        """Parse OpenRouter Proxy supported models from JSON string or list."""
        if isinstance(v, str):
            try:
                parsed = json.loads(v)
                if isinstance(parsed, list):
                    return [str(item) for item in parsed]
                return []
            except json.JSONDecodeError:
                # Fallback to default models if JSON parsing fails
                return DEFAULT_OPENROUTER_MODELS.copy()
        elif isinstance(v, list):
            return [str(item) for item in v]
        return []

    # GigaChat Settings
    GIGACHAT_BASE_URL: str = "https://gigachat.devices.sberbank.ru/api/v1"
    GIGACHAT_API_KEY: str = ""
    GIGACHAT_LOGIN: str = ""
    GIGACHAT_PASSWORD: str = ""
    GIGACHAT_SCOPE: str = "GIGACHAT_API_PERS"
    GIGACHAT_VERIFY_SSL_CERTS: bool = False

    # Yandex Settings
    YANDEX_API_KEY: str = ""
    YANDEX_API_KEY_ID: str = ""
    YANDEX_FOLDER_ID: str = ""
    YANDEX_BASE_URL: str = "https://llm.api.cloud.yandex.net/foundationModels/v1"

    # Ollama Settings
    OLLAMA_API_KEYS: str = ""
    OLLAMA_BASE_URLS: str = ""  # Comma-separated list of Ollama server URLs

    # Z.AI Settings
    ZAI_API_KEY: str = ""
    ZAI_BASE_URL: str = "https://api.z.ai/api/paas/v4"

    # Logging
    LOG_LEVEL: str = "INFO"
    LOG_FORMAT: str = "json"  # Available formats: json, text, structured
    LOG_EXTRA_FIELDS: list[str] = []  # Additional fields for logs

settings = Settings()
