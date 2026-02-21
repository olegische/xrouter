"""GigaChat model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


class GigaChatModelMapper(BaseModelMapper):
    """Mapper for GigaChat models."""

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map GigaChat models response to provider models.

        Args:
            models_data: Raw models response data

        Returns:
            List of provider models
        """
        models = []
        for model_data in models_data["data"]:
            model_id = model_data["id"]
            # Skip models we don't use
            if model_id == "GigaChat-Plus":
                continue

            # Map model info based on model ID
            if model_id == "GigaChat":
                description = (
                    "A lightweight model for simple tasks " "requiring maximum speed."
                )
                context_length = 32768
                max_completion_tokens = 4096
            elif model_id == "GigaChat-2":
                description = (
                    "A lightweight model for simple tasks " "requiring maximum speed."
                )
                context_length = 131072  # 128k
                max_completion_tokens = 4096
            elif model_id == "GigaChat-Pro":
                description = (
                    "An advanced model for complex tasks "
                    "requiring creativity and better adherence to instructions."
                )
                context_length = 32768
                max_completion_tokens = 4096
            elif model_id == "GigaChat-2-Pro":
                description = (
                    "An advanced model for complex tasks "
                    "requiring creativity and better adherence to instructions."
                )
                context_length = 131072  # 128k
                max_completion_tokens = 4096
            elif model_id == "GigaChat-Max":
                description = (
                    "A premium model for the most demanding tasks, "
                    "requiring maximum precision, creativity, "
                    "and context understanding."
                )
                context_length = 32768
                max_completion_tokens = 8192
            elif model_id == "GigaChat-2-Max":
                description = (
                    "A premium model for the most demanding tasks, "
                    "requiring maximum precision, creativity, "
                    "and context understanding."
                )
                context_length = 131072  # 128k
                max_completion_tokens = 8192
            else:
                continue

            model = ProviderModel(
                model_id=model_id,
                name=model_id,
                provider_id=self._provider.provider_id,
                description=description,
                context_length=context_length,
                architecture={
                    "instruct_type": "none",
                    "modality": "text->text",
                    "tokenizer": "gigachat",
                },
                capabilities={
                    "context_length": context_length,
                    "is_moderated": True,
                    "is_tool_calls": True,
                    "max_completion_tokens": max_completion_tokens,
                },
            )
            models.append(model)

        return models
