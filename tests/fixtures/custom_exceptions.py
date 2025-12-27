"""Test file with custom exception definitions."""


class CustomError(Exception):
    """A custom exception for testing."""
    pass


class ValidationError(CustomError):
    """Validation failed."""
    pass


class NetworkError(Exception):
    """Network operation failed."""
    pass


def raise_custom():
    raise CustomError("custom error")


def raise_validation():
    raise ValidationError("validation failed")


def raise_network():
    raise NetworkError("network error")


def raise_builtin():
    raise ValueError("builtin error")
