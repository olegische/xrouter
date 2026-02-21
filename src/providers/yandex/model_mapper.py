"""Yandex model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


class YandexModelMapper(BaseModelMapper):
    """Mapper for Yandex models."""

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map Yandex models.

        Since Yandex doesn't provide a models endpoint, we hardcode the available models.

        Args:
            models_data: Not used for Yandex as models are hardcoded

        Returns:
            List of provider models
        """
        models = []

        # YandexGPT5 Pro
        models.append(
            ProviderModel(
                model_id="yandexgpt5-pro:latest",
                name="YandexGPT5 Pro",
                provider_id=self._provider.provider_id,
                description="YandexGPT Pro 5 model with 32K context window.",
                context_length=32768,
                architecture={
                    "instruct_type": "none",
                    "modality": "text->text",
                    "tokenizer": "yandex",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": True,
                    "max_completion_tokens": 4096,
                },
            )
        )

        # YandexGPT Pro 5.1 (RC)
        models.append(
            ProviderModel(
                model_id="yandexgpt5.1-pro:rc",
                name="YandexGPT Pro 5.1",
                provider_id=self._provider.provider_id,
                description="YandexGPT Pro 5.1 RC model with 32K context window.",
                context_length=32768,
                architecture={
                    "instruct_type": "none",
                    "modality": "text->text",
                    "tokenizer": "yandex",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": True,
                    "max_completion_tokens": 4096,
                },
            )
        )

        # YandexGPT Lite 5
        models.append(
            ProviderModel(
                model_id="yandexgpt-lite5:latest",
                name="YandexGPT Lite 5",
                provider_id=self._provider.provider_id,
                description="YandexGPT Lite 5 model with 32K context window.",
                context_length=32768,
                architecture={
                    "instruct_type": "none",
                    "modality": "text->text",
                    "tokenizer": "yandex",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": False,
                    "max_completion_tokens": 4096,
                },
            )
        )

        # Alice AI LLM
        models.append(
            ProviderModel(
                model_id="aliceai-llm:latest",
                name="Alice AI LLM",
                provider_id=self._provider.provider_id,
                description="Alice AI LLM text generation model.",
                context_length=32768,
                architecture={
                    "instruct_type": "none",
                    "modality": "text->text",
                    "tokenizer": "yandex",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": False,
                    "max_completion_tokens": 4096,
                },
            )
        )

        return models
