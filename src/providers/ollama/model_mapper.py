"""Ollama model mapper."""
from typing import Any, Dict, List

from ..base_model_mapper import BaseModelMapper
from ..models import ProviderModel


class OllamaModelMapper(BaseModelMapper):
    """Mapper for Ollama models."""

    def _find_context_length(self, model_info: Dict[str, Any]) -> int:
        """Find context length by searching for *context_length in model info."""
        for key, value in model_info.items():
            if key.endswith(".context_length") and isinstance(value, int):
                return value
        return 4096  # default if not found

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map Ollama models response to provider models.

        Args:
            models_data: Raw models response data

        Returns:
            List of provider models
        """
        models = []
        for model_data in models_data.get("models", []):
            tags_info = model_data["tags_info"]
            show_info = model_data["show_info"]

            # Extract model details
            model_id = tags_info["name"]
            details = tags_info.get("details", {})
            model_info = show_info.get("model_info", {})

            # Get context length dynamically
            context_length = self._find_context_length(model_info)

            # Create architecture info
            architecture = {
                "instruct_type": "none",  # TODO determine instruct type
                "modality": "text->text",  # All Ollama models are text-to-text
                "tokenizer": model_info.get("tokenizer.ggml.model", "unknown"),
                "format": details.get("format"),
                "family": details.get("family"),
                "families": details.get("families", []),
                "parameter_size": details.get("parameter_size"),
                "quantization_level": details.get("quantization_level"),
            }

            # Create capabilities info
            capabilities = {
                "context_length": context_length,
                "is_moderated": False,  # No built-in moderation
                "is_tool_calls": False,  # No OpenAI-style tool calls
                "max_completion_tokens": context_length,  # Can use full context for completion
            }

            model = ProviderModel(
                model_id=model_id,
                name=model_id,
                provider_id=self._provider.provider_id,
                context_length=context_length,
                architecture=architecture,
                capabilities=capabilities,
            )
            models.append(model)

        return models
