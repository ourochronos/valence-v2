"""
Valence v2 — Python client library for the Valence knowledge substrate.

Provides a typed HTTP client for the Valence v2 engine REST API:
triple insertion, pattern queries, semantic search, k-hop neighborhood
traversal, provenance retrieval, and maintenance triggers.
"""

__version__ = "2.0.0"
__author__ = "ourochronos"
__license__ = "MIT"

from valence.client import ValenceClient
from valence.models import Triple, SearchResult, EngineStats

__all__ = [
    "__version__",
    "ValenceClient",
    "Triple",
    "SearchResult",
    "EngineStats",
]
