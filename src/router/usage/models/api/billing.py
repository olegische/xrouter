"""Billing API models."""
from pydantic import BaseModel, Field

from ..billing import Cost, TokenCount
from ..common import Currency

# Server models (for API validation)


class ServerCalculateCostRequest(BaseModel):
    """Server request model for calculating cost."""

    token_count: TokenCount = Field(..., description="Token count information")
    currency: Currency = Field(default=Currency.RUB, description="Currency of the cost")


class CalculateCostResponse(BaseModel):
    """Response model for calculating cost."""

    cost: Cost = Field(..., description="Calculated cost information")


class ServerProcessCostRequest(BaseModel):
    """Server request model for processing cost."""

    cost: Cost = Field(..., description="Cost to process")


class ProcessCostResponse(BaseModel):
    """Response model for processing cost."""

    amount_held: float = Field(..., description="Amount put on hold")
    transaction_id: str = Field(
        ..., description="Unique transaction identifier for the hold"
    )


class ServerFinalizeHoldRequest(BaseModel):
    """Server request model for finalizing a cost hold."""

    cost: Cost = Field(..., description="Cost to finalize")
    transaction_id: str = Field(
        ..., description="Unique transaction identifier for the hold to finalize"
    )


class FinalizeHoldResponse(BaseModel):
    """Response model for finalizing a cost hold."""

    success: bool = Field(..., description="Whether finalization was successful")


class ServerProcessCostWithTokensRequest(BaseModel):
    """Server request model for processing cost with tokens."""

    token_count: TokenCount = Field(..., description="Token count information")


class ServerFinalizeHoldWithTokensRequest(BaseModel):
    """Server request model for finalizing a cost hold with tokens."""

    token_count: TokenCount = Field(..., description="Token count information")
    transaction_id: str = Field(
        ..., description="Unique transaction identifier for the hold to finalize"
    )


# Client models (with api_key)


class CalculateCostRequest(ServerCalculateCostRequest):
    """Client request model for calculating cost."""

    api_key: str = Field(..., description="API key for authentication")


class ProcessCostRequest(ServerProcessCostRequest):
    """Client request model for processing cost."""

    api_key: str = Field(..., description="API key for authentication")


class FinalizeHoldRequest(ServerFinalizeHoldRequest):
    """Client request model for finalizing a cost hold."""

    api_key: str = Field(..., description="API key for authentication")
    transaction_id: str = Field(
        ..., description="Unique transaction identifier for the hold to finalize"
    )


class ProcessCostWithTokensRequest(ServerProcessCostWithTokensRequest):
    """Client request model for processing cost with tokens."""

    api_key: str = Field(..., description="API key for authentication")


class FinalizeHoldWithTokensRequest(ServerFinalizeHoldWithTokensRequest):
    """Client request model for finalizing a cost hold with tokens."""

    api_key: str = Field(..., description="API key for authentication")
