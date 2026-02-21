"""Provider models."""
from pydantic import BaseModel, Field


class ProviderConfig(BaseModel):
    """Provider configuration."""

    provider_id: str = Field(
        description="Unique identifier for the provider "
        "(e.g., 'openai', 'anthropic', 'xrouter')"
    )
    name: str = Field(description="Human-readable name of the provider")
    credentials: str = Field(
        description="Authentication credentials or API key for the provider"
    )
    parameters: dict = Field(
        description="Additional configuration parameters specific to the provider "
        "(e.g., model settings, API version)"
    )
    base_url: str = Field(description="Base URL for the provider's API endpoint")

    class Config:
        """Pydantic model configuration."""

        from_attributes = True
