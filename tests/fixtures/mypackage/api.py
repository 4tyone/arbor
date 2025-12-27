"""API module with functions that can raise exceptions."""

from typing import Optional


def get_data(url: str) -> dict:
    """Fetch data from a URL."""
    if not url:
        raise ValueError("URL cannot be empty")
    if not url.startswith("http"):
        raise ValueError("URL must start with http")
    return {"status": "ok"}


def post_data(url: str, data: dict) -> Optional[dict]:
    """Post data to a URL."""
    if not url:
        raise ValueError("URL cannot be empty")
    if data is None:
        return None
    return {"status": "posted"}


class APIClient:
    """Client for API interactions."""

    def __init__(self, base_url: str):
        self.base_url = base_url

    def request(self, method: str, path: str) -> dict:
        """Make a request."""
        if not method:
            raise ValueError("Method required")
        return {"method": method, "path": path}

    def get(self, path: str) -> dict:
        """GET request."""
        return self.request("GET", path)
