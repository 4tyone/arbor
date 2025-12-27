"""Data models."""


class User:
    """User model."""

    def __init__(self, name: str, email: str):
        if not name:
            raise ValueError("Name is required")
        if not email or "@" not in email:
            raise ValueError("Valid email is required")
        self.name = name
        self.email = email

    def validate(self) -> bool:
        """Validate the user."""
        return bool(self.name and self.email)


class Admin(User):
    """Admin user."""

    def __init__(self, name: str, email: str, permissions: list):
        super().__init__(name, email)
        self.permissions = permissions
