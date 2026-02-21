"""LLM Gateway model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


def _is_vision_modality(modality: str) -> bool:
    """Check if modality string indicates vision (image) support."""
    if not modality:
        return False
    return "image" in modality.lower()


class XRouterModelMapper(BaseModelMapper):
    """Mapper for LLM Gateway (OpenRouter-compatible) models."""

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map xrouter/OpenRouter models response to provider models.

        API returns: data[].id, name, description, context_length,
        architecture.{tokenizer, instruct_type, modality},
        top_provider.{context_length, max_completion_tokens, is_moderated},
        per_request_limits.{prompt_tokens, completion_tokens}.

        Args:
            models_data: Raw models response data (must have "data" list)

        Returns:
            List of provider models
        """
        models = []
        raw_list = models_data.get("data") or []
        for model_data in raw_list:
            model_id = model_data.get("id")
            if not model_id:
                continue
            name = model_data.get("name") or model_id
            description = model_data.get("description")
            context_length = model_data.get("context_length") or 0
            arch = model_data.get("architecture") or {}
            top = model_data.get("top_provider") or {}
            limits = model_data.get("per_request_limits") or {}
            if context_length <= 0 and top:
                context_length = top.get("context_length") or context_length
            max_completion = (limits.get("completion_tokens") or
                             top.get("max_completion_tokens") or 4096)
            modality = arch.get("modality") or "text->text"
            is_vision = _is_vision_modality(modality)
            model = ProviderModel(
                model_id=model_id,
                name=name,
                provider_id=self._provider.provider_id,
                description=description,
                context_length=context_length,
                architecture={
                    "instruct_type": arch.get("instruct_type", "none"),
                    "modality": modality,
                    "tokenizer": arch.get("tokenizer", "Other"),
                },
                capabilities={
                    "context_length": context_length,
                    "is_moderated": top.get("is_moderated", True),
                    "is_tool_calls": False,
                    "max_completion_tokens": max_completion,
                    "is_vision": is_vision,
                },
            )
            models.append(model)
        return models
