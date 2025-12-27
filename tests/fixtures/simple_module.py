"""A simple module with functions."""


def simple_function(x: int) -> int:
    """A simple function."""
    if x < 0:
        raise ValueError("x must be non-negative")
    return x * 2


def another_function():
    """Another function that returns None."""
    return None


class SimpleClass:
    """A simple class."""

    def method_one(self):
        """First method."""
        return 1

    def method_two(self, value):
        """Second method."""
        if value is None:
            raise TypeError("value cannot be None")
        return value
