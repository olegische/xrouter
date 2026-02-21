"""Z.AI model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel

# Static list of Z.AI models from API documentation
# Text models: https://docs.z.ai/api-reference/llm/chat-completion
# Vision models: glm-4.6v, glm-4.5v, etc.
ZAI_MODELS = [
    {
        "model_id": "glm-5",
        "name": "GLM-5",
        "description": "Flagship foundation model for agentic engineering",
        "context_length": 131072,  # 128K
        "max_completion_tokens": 131072,
        "is_vision": False,
        "is_cot": True,
    },
    {
        "model_id": "glm-4.7",
        "name": "GLM-4.7",
        "description": "Advanced GLM-4.7 series model",
        "context_length": 131072,
        "max_completion_tokens": 131072,
        "is_vision": False,
        "is_cot": True,
    },
    {
        "model_id": "glm-4.7-flash",
        "name": "GLM-4.7 Flash",
        "description": "Fast GLM-4.7 model",
        "context_length": 131072,
        "max_completion_tokens": 131072,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.7-flashx",
        "name": "GLM-4.7 FlashX",
        "description": "Ultra-fast GLM-4.7 model",
        "context_length": 131072,
        "max_completion_tokens": 131072,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.6",
        "name": "GLM-4.6",
        "description": "GLM-4.6 text model",
        "context_length": 131072,
        "max_completion_tokens": 131072,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.5",
        "name": "GLM-4.5",
        "description": "GLM-4.5 text model",
        "context_length": 98304,  # 96K
        "max_completion_tokens": 98304,
        "is_vision": False,
        "is_cot": True,
    },
    {
        "model_id": "glm-4.5-air",
        "name": "GLM-4.5 Air",
        "description": "Lightweight GLM-4.5 model",
        "context_length": 98304,
        "max_completion_tokens": 98304,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.5-x",
        "name": "GLM-4.5 X",
        "description": "Extended GLM-4.5 model",
        "context_length": 98304,
        "max_completion_tokens": 98304,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.5-airx",
        "name": "GLM-4.5 AirX",
        "description": "Lightweight extended GLM-4.5 model",
        "context_length": 98304,
        "max_completion_tokens": 98304,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.5-flash",
        "name": "GLM-4.5 Flash",
        "description": "Fast GLM-4.5 model",
        "context_length": 98304,
        "max_completion_tokens": 98304,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4-32b-0414-128k",
        "name": "GLM-4 32B 128K",
        "description": "GLM-4 32B with 128K context",
        "context_length": 131072,
        "max_completion_tokens": 16384,
        "is_vision": False,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.6v",
        "name": "GLM-4.6V",
        "description": "Multimodal vision model with 128K context",
        "context_length": 131072,
        "max_completion_tokens": 32768,
        "is_vision": True,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.6v-flash",
        "name": "GLM-4.6V Flash",
        "description": "Fast multimodal vision model",
        "context_length": 131072,
        "max_completion_tokens": 32768,
        "is_vision": True,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.6v-flashx",
        "name": "GLM-4.6V FlashX",
        "description": "Ultra-fast multimodal vision model",
        "context_length": 131072,
        "max_completion_tokens": 32768,
        "is_vision": True,
        "is_cot": False,
    },
    {
        "model_id": "glm-4.5v",
        "name": "GLM-4.5V",
        "description": "Multimodal vision model",
        "context_length": 98304,
        "max_completion_tokens": 16384,
        "is_vision": True,
        "is_cot": False,
    },
    {
        "model_id": "autoglm-phone-multilingual",
        "name": "AutoGLM Phone Multilingual",
        "description": "Mobile intelligent assistant model",
        "context_length": 4096,
        "max_completion_tokens": 4096,
        "is_vision": True,
        "is_cot": False,
    },
]


class ZaiModelMapper(BaseModelMapper):
    """Mapper for Z.AI models.

    Z.AI does not expose a models list API, so we use a static list
    from the official documentation.
    """

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map Z.AI models to provider models.

        Ignores models_data since Z.AI has no models API - returns static list.

        Args:
            models_data: Raw models response (unused for Z.AI)

        Returns:
            List of provider models
        """
        models = []
        for m in ZAI_MODELS:
            model = ProviderModel(
                model_id=m["model_id"],
                name=m["name"],
                provider_id=self._provider.provider_id,
                description=m["description"],
                context_length=m["context_length"],
                architecture={
                    "instruct_type": "none",
                    "modality": "text->image" if m["is_vision"] else "text->text",
                    "tokenizer": "glm",
                },
                capabilities={
                    "context_length": m["context_length"],
                    "is_moderated": True,
                    "is_tool_calls": True,
                    "max_completion_tokens": m["max_completion_tokens"],
                    "is_vision": m["is_vision"],
                    "is_cot": m.get("is_cot", False),
                },
            )
            models.append(model)
        return models
