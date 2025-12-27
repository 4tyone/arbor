"""Test file with network-related exceptions for grouping tests."""

import requests


class ConnectionTimeout(Exception):
    pass


class NetworkError(Exception):
    pass


class AuthenticationError(Exception):
    pass


class ValidationError(Exception):
    pass


def fetch_data(url):
    if not url:
        raise ValidationError("URL is required")

    try:
        response = requests.get(url)
    except requests.exceptions.Timeout:
        raise ConnectionTimeout("Request timed out")
    except requests.exceptions.ConnectionError:
        raise NetworkError("Could not connect")

    if response.status_code == 401:
        raise AuthenticationError("Invalid credentials")

    if response.status_code == 400:
        raise ValidationError("Bad request")

    return response.json()


def process_api_call():
    raise ConnectionTimeout("API timeout")


def validate_input(data):
    if not data:
        raise ValidationError("Data is required")
    if not isinstance(data, dict):
        raise TypeError("Data must be a dict")
