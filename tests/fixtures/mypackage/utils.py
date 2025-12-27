"""Utility functions."""


def helper_function(value: str) -> str:
    """A helper function."""
    if not value:
        raise ValueError("Value cannot be empty")
    return value.upper()


def internal_helper():
    """Internal helper not exported."""
    return "internal"
