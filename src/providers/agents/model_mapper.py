"""LLM Gateway model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


class AgentsModelMapper(BaseModelMapper):
    """Mapper for LLM Gateway models."""

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map LLM Gateway models.

        Since we only support one model, we hardcode it.

        Args:
            models_data: Not used as models are hardcoded

        Returns:
            List of provider models
        """
        models = []

        # DeepSeek R1 70B
        models.append(
            ProviderModel(
                model_id="deepseek-r1:70b-32k",
                name="DeepSeek R1 70B (32K ctx)",
                provider_id=self._provider.provider_id,
                description="DeepSeek R1 70B is a powerful large language model with extended context length of 32K tokens (32,768 tokens). It excels at complex reasoning, coding, and analysis tasks.",
                context_length=32768,
                architecture={
                    "instruct_type": "deepseek",
                    "modality": "text->text",
                    "tokenizer": "llama",
                    "parameter_size": "70553706560",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": False,
                    "max_completion_tokens": 4096,
                    "is_vision": False,
                },
            )
        )

        # Qwen 2.5 Coder 32B
        models.append(
            ProviderModel(
                model_id="qwen2.5-coder:32b-instruct-q8_0-32k",
                name="Qwen 2.5 Coder 32B (32K ctx)",
                provider_id=self._provider.provider_id,
                description="Qwen 2.5 Coder 32B is a specialized coding model with extended context length of 32K tokens (32,768 tokens). It excels at programming tasks across multiple languages and frameworks.",
                context_length=32768,
                architecture={
                    "instruct_type": "qwen",
                    "modality": "text->text",
                    "tokenizer": "qwen2",
                    "parameter_size": "32763876352",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": False,
                    "max_completion_tokens": 4096,
                    "is_vision": False,
                },
            )
        )

        # Llama 3.2-Vision 90B
        models.append(
            ProviderModel(
                model_id="llama3.2-vision:90b-32k",
                name="Llama 3.2-Vision 90B (32K ctx)",
                provider_id=self._provider.provider_id,
                description="Llama 3.2-Vision 90B is a powerful multimodal model that excels at visual recognition, image reasoning, captioning, and answering questions about images. It supports English, German, French, Italian, Portuguese, Hindi, Spanish, and Thai for text-only tasks.",
                context_length=32768,
                architecture={
                    "instruct_type": "llama",
                    "modality": "image+text->text",
                    "tokenizer": "llama",
                    "parameter_size": "90000000000",
                },
                capabilities={
                    "context_length": 32768,
                    "is_moderated": True,
                    "is_tool_calls": False,
                    "max_completion_tokens": 4096,
                    "is_vision": True,
                },
            )
        )

        return models
