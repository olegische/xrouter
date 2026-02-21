"""API key models."""
from datetime import datetime
from enum import Enum
from typing import Optional

from pydantic import BaseModel, Field


class APIKeyStatus(str, Enum):
    """API key status."""

    ACTIVE = "ACTIVE"
    INACTIVE = "INACTIVE"
    SUSPENDED = "SUSPENDED"
    REVOKED = "REVOKED"


class APIKeyType(str, Enum):
    """API key type."""

    USER = "USER"
    ADMIN = "ADMIN"


class APIKey(BaseModel):
    """Model for storing API keys."""

    key_hash: str = Field(..., description="Hash of the API key", max_length=64)
    name: Optional[str] = Field(
        None, description="Optional key name/description", max_length=255
    )
    created_at: datetime = Field(
        default_factory=datetime.utcnow, description="Creation timestamp"
    )
    expires_at: Optional[datetime] = Field(
        None, description="Optional expiration timestamp"
    )
    last_used_at: Optional[datetime] = Field(None, description="Last usage timestamp")
    status: APIKeyStatus = Field(
        default=APIKeyStatus.ACTIVE, description="API key status"
    )
    type: APIKeyType = Field(default=APIKeyType.USER, description="API key type")
    user_id: Optional[str] = Field(None, description="ID of the user who owns this key")
    issuer: Optional[str] = Field(
        None, description="Key issuer (e.g. 'xrouter' or 'xbilling')"
    )

    class Config:
        """Pydantic model configuration."""

        json_encoders = {datetime: lambda dt: dt.isoformat()}
