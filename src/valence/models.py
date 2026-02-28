"""Typed data models for the Valence v2 API."""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Any, Optional


@dataclass
class Triple:
    """An (subject, predicate, object) triple with optional metadata."""
    subject: str
    predicate: str
    object: str
    id: Optional[str] = None
    weight: float = 1.0
    sources: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "subject": self.subject,
            "predicate": self.predicate,
            "object": self.object,
            "weight": self.weight,
        }
        if self.sources:
            d["sources"] = self.sources
        if self.metadata:
            d["metadata"] = self.metadata
        return d


@dataclass
class SearchResult:
    """A single semantic search hit."""
    triple: Triple
    score: float
    distance: float


@dataclass
class EngineStats:
    """Snapshot of engine health statistics."""
    triple_count: int
    node_count: int
    source_count: int
    embedding_dimension: int
    extra: dict[str, Any] = field(default_factory=dict)
