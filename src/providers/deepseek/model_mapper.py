"""Deepseek model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


class DeepseekModelMapper(BaseModelMapper):
    """Mapper for Deepseek models."""

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map Deepseek models response to provider models.

        Args:
            models_data: Raw models response data

        Returns:
            List of provider models
        """
        models = []
        for model_data in models_data["data"]:
            model_id = model_data["id"]

            # Map model info based on model ID
            if model_id == "deepseek-chat":
                name = "DeepSeek: DeepSeek V3"
                description = (
                    "A versatile chat model with strong general capabilities "
                    "and extended context length."
                )
                context_length = 65536  # 64K
                max_completion_tokens = 8192  # 8K
                is_cot = False
                max_cot_tokens = None
            # TODO добавить работу с reasoning моделью
            elif model_id == "deepseek-reasoner":
                name = "DeepSeek: DeepSeek R1"
                description = (
                    "An advanced reasoning model optimized for complex problem-solving "
                    "with chain-of-thought capabilities."
                )
                context_length = 65536  # 64K
                max_completion_tokens = 8192  # 8K
                is_cot = True
                max_cot_tokens = 32768  # 32K
            else:
                continue

            model = ProviderModel(
                model_id=model_id,
                name=name,
                provider_id=self._provider.provider_id,
                description=description,
                context_length=context_length,
                architecture={
                    "instruct_type": "none",
                    "modality": "text->text",
                    "tokenizer": "deepseek",
                },
                capabilities={
                    "context_length": context_length,
                    "is_moderated": True,
                    "is_tool_calls": True,
                    "max_completion_tokens": max_completion_tokens,
                    "is_cot": is_cot,
                    "max_cot_tokens": max_cot_tokens,
                },
            )
            models.append(model)

        return models
