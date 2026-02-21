"""Error models for provider API.

This module contains error classes used across the provider API for handling
various types of errors that may occur during request processing.
"""
from typing import Any, Dict, Optional


class ProviderError(Exception):
    """Provider error with details."""

    def __init__(
        self,
        code: int,
        message: str,
        details: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Initialize provider error.

        Args:
            code: Error code
            message: Error message
            details: Optional error details
        """
        self.code = code
        self.message = message
        self.details = details or {}
        super().__init__(message)


class ValidationError(ProviderError):
    """Validation error."""

    def __init__(self, message: str, field: str) -> None:
        """Initialize validation error.

        Args:
            message: Error message
            field: Field that failed validation
        """
        super().__init__(
            code=400,
            message=message,
            details={"field": field},
        )
