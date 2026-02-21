"""Provider constants and configuration."""
from typing import Dict, List

# Provider IDs
PROVIDER_XROUTER = "xrouter"
PROVIDER_AGENTS = "agents"
PROVIDER_DEEPSEEK = "deepseek"
PROVIDER_OPENROUTER = "openrouter"
PROVIDER_OPENROUTER_PROXY = "openrouter-proxy"
PROVIDER_GIGACHAT = "gigachat"
PROVIDER_YANDEX = "yandex"
PROVIDER_OLLAMA = "ollama"
PROVIDER_ZAI = "zai"

# Provider names for display
PROVIDER_NAMES: Dict[str, str] = {
    PROVIDER_XROUTER: "XRouter",
    PROVIDER_AGENTS: "Agents",
    PROVIDER_DEEPSEEK: "Deepseek",
    PROVIDER_OPENROUTER: "OpenRouter",
    PROVIDER_OPENROUTER_PROXY: "OpenRouterProxy",
    PROVIDER_GIGACHAT: "GigaChat",
    PROVIDER_YANDEX: "Yandex",
    PROVIDER_OLLAMA: "Ollama",
    PROVIDER_ZAI: "Z.AI",
}

# Default providers for LLM Gateway API
XROUTER_API_PROVIDERS: List[str] = [
    PROVIDER_XROUTER,
    PROVIDER_DEEPSEEK,
    PROVIDER_OPENROUTER,
    PROVIDER_OPENROUTER_PROXY,
    PROVIDER_GIGACHAT,
    PROVIDER_YANDEX,
    PROVIDER_OLLAMA,
    PROVIDER_ZAI,
]
