"""Dependency injection module."""
from .dependencies import container, Container
from .setup import cleanup_di, setup_di

__all__ = ["container", "setup_di", "cleanup_di", "Container"]
