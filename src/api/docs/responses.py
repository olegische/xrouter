"""Common response examples for API documentation."""
from typing import Dict

# Common error responses
ERROR_RESPONSES: Dict[int, Dict] = {
    400: {
        "description": "Bad request",
        "content": {
            "application/json": {
                "example": {
                    "error": {
                        "message": "Invalid request parameters",
                        "type": "invalid_request",
                        "code": 400,
                        "details": {"error": "Invalid model format"},
                    }
                }
            }
        },
    },
    401: {
        "description": "Unauthorized",
        "content": {
            "application/json": {
                "example": {
                    "error": {
                        "message": "Authentication required",
                        "type": "auth_error",
                        "code": 401,
                        "details": {"error": "Missing API key"},
                    }
                }
            }
        },
    },
    429: {
        "description": "Rate limit exceeded",
        "content": {
            "application/json": {
                "example": {
                    "error": {
                        "message": "Rate limit exceeded",
                        "type": "rate_limit_error",
                        "code": 429,
                        "details": {"error": "Too many requests"},
                    }
                }
            }
        },
    },
    500: {
        "description": "Internal server error",
        "content": {
            "application/json": {
                "example": {
                    "error": {
                        "message": "Internal server error",
                        "type": "internal_error",
                        "code": 500,
                        "details": {"error": "Unexpected error occurred"},
                    }
                }
            }
        },
    },
}
