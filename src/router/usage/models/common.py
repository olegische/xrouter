"""Common models and types."""
from enum import Enum


class Currency(str, Enum):
    """Supported currencies."""

    RUB = "RUB"
    USD = "USD"
