"""XBilling API models."""
from typing import Literal, Optional

from pydantic import BaseModel, Field


class IntrospectRequest(BaseModel):
    """Request model for token introspection."""

    token: str = Field(..., description="The string value of the token")
    token_type_hint: Literal["api_key"] = Field(
        "api_key", description="A hint about the type of the token to be introspected"
    )


class IntrospectResponse(BaseModel):
    """Response model for token introspection."""

    active: bool = Field(
        ..., description="Boolean indicator of whether the token is active"
    )
    token_type: Optional[Literal["api_key"]] = Field(
        None, description="Type of the token"
    )
    exp: Optional[int] = Field(None, description="Timestamp when the token will expire")
    iat: Optional[int] = Field(None, description="Timestamp when the token was issued")
    aud: Optional[str] = Field(None, description="The intended audience for this token")
    iss: Optional[str] = Field(None, description="The issuer of the token")
    sub: Optional[str] = Field(None, description="The subject of the token (user ID)")
