"""OpenRouter model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


class OpenRouterProxyModelMapper(BaseModelMapper):
    """Mapper for OpenRouter models."""

    @property
    def supported_models(self) -> set:
        """Get supported models from settings."""
        return set(self.settings.OPENROUTER_PROXY_SUPPORTED_MODELS)

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map OpenRouter models response to provider models.

        Args:
            models_data: Raw models response data

        Returns:
            List of provider models
        """
        models = []
        models_list = models_data.get("data", [])
        if not isinstance(models_list, list):
            return []

        for model_data in models_list:
            if not isinstance(model_data, dict):
                continue

            model_id = str(model_data.get("id", ""))

            # Filter only specific models we want to expose
            if model_id not in self.supported_models:
                continue

            # Get base model data from response
            name = str(model_data.get("name", ""))
            description = str(model_data.get("description", ""))
            context_length = int(model_data.get("context_length", 4096))

            # Get architecture info
            architecture = model_data.get("architecture", {})
            modality = str(architecture.get("modality", "text->text"))
            tokenizer = architecture.get("tokenizer")
            if not tokenizer:
                # Set default tokenizer based on model
                if "openai" in model_id:
                    tokenizer = "openai"
                else:
                    tokenizer = "unknown"

            # Get capabilities from top_provider
            top_provider = model_data.get("top_provider") or {}
            capabilities = {
                "context_length": int(
                    top_provider.get("context_length") or context_length
                ),
                "is_moderated": bool(top_provider.get("is_moderated", True)),
                "is_tool_calls": True,  # We enable tool calls for our supported models
                "max_completion_tokens": int(
                    top_provider.get("max_completion_tokens") or 4096
                ),
            }

            # Add is_cot for thinking model
            if model_id.endswith(":thinking"):
                capabilities["is_cot"] = True

            model = ProviderModel(
                model_id=model_id,
                name=name,
                provider_id=self._provider.provider_id,
                description=description,
                context_length=context_length,
                architecture={
                    "instruct_type": "none",
                    "modality": modality,
                    "tokenizer": tokenizer,
                },
                capabilities=capabilities,
            )
            models.append(model)

        return models
