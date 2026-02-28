"""HTTP client for the Valence v2 REST API."""
from __future__ import annotations

import json
from typing import Any, Optional
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError

from valence.models import Triple, SearchResult, EngineStats


class ValenceError(Exception):
    """Raised when the Valence API returns an error response."""
    def __init__(self, status: int, message: str) -> None:
        super().__init__(f"HTTP {status}: {message}")
        self.status = status


class ValenceClient:
    """Synchronous HTTP client for the Valence v2 engine.

    Parameters
    ----------
    base_url:
        Base URL of the running Valence engine, e.g.
        ``"http://localhost:8421"``.  Trailing slash is stripped.
    timeout:
        Per-request timeout in seconds (default 30).

    Examples
    --------
    >>> from valence import ValenceClient
    >>> client = ValenceClient("http://localhost:8421")
    >>> client.insert([Triple("Alice", "knows", "Bob")])
    >>> results = client.search("Who does Alice know?")
    """

    def __init__(self, base_url: str = "http://localhost:8421", timeout: int = 30) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _request(self, method: str, path: str, body: Any = None) -> Any:
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode() if body is not None else None
        headers = {"Content-Type": "application/json", "Accept": "application/json"}
        req = Request(url, data=data, headers=headers, method=method)
        try:
            with urlopen(req, timeout=self.timeout) as resp:
                return json.loads(resp.read())
        except HTTPError as exc:
            raise ValenceError(exc.code, exc.read().decode(errors="replace")) from exc
        except URLError as exc:
            raise ValenceError(0, str(exc.reason)) from exc

    @staticmethod
    def _parse_triple(raw: dict[str, Any]) -> Triple:
        return Triple(
            subject=raw["subject"],
            predicate=raw["predicate"],
            object=raw["object"],
            id=raw.get("id"),
            weight=float(raw.get("weight", 1.0)),
            sources=raw.get("sources", []),
            metadata=raw.get("metadata", {}),
        )

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def insert(self, triples: list[Triple]) -> list[str]:
        """Insert one or more triples.  Returns the assigned IDs."""
        payload = {"triples": [t.to_dict() for t in triples]}
        result = self._request("POST", "/triples", payload)
        return result.get("ids", [])

    def query(
        self,
        subject: Optional[str] = None,
        predicate: Optional[str] = None,
        object: Optional[str] = None,
        limit: int = 100,
    ) -> list[Triple]:
        """Pattern-match query.  Pass ``None`` to wildcard a field."""
        params: list[str] = []
        if subject is not None:
            params.append(f"subject={subject}")
        if predicate is not None:
            params.append(f"predicate={predicate}")
        if object is not None:
            params.append(f"object={object}")
        params.append(f"limit={limit}")
        qs = "&".join(params)
        result = self._request("GET", f"/triples?{qs}")
        return [self._parse_triple(r) for r in result.get("triples", [])]

    def search(self, query: str, limit: int = 10) -> list[SearchResult]:
        """Semantic search via spectral topology embeddings."""
        payload = {"query": query, "limit": limit}
        result = self._request("POST", "/search", payload)
        hits = []
        for r in result.get("results", []):
            hits.append(
                SearchResult(
                    triple=self._parse_triple(r["triple"]),
                    score=float(r.get("score", 0.0)),
                    distance=float(r.get("distance", 0.0)),
                )
            )
        return hits

    def neighbors(self, node_id: str, depth: int = 2) -> list[Triple]:
        """K-hop subgraph traversal from *node_id*."""
        result = self._request("GET", f"/nodes/{node_id}/neighbors?depth={depth}")
        return [self._parse_triple(r) for r in result.get("triples", [])]

    def sources(self, triple_id: str) -> list[dict[str, Any]]:
        """Retrieve provenance sources for a triple."""
        return self._request("GET", f"/triples/{triple_id}/sources").get("sources", [])

    def stats(self) -> EngineStats:
        """Return current engine statistics."""
        raw = self._request("GET", "/stats")
        return EngineStats(
            triple_count=raw.get("triple_count", 0),
            node_count=raw.get("node_count", 0),
            source_count=raw.get("source_count", 0),
            embedding_dimension=raw.get("embedding_dimension", 0),
            extra={k: v for k, v in raw.items() if k not in
                   {"triple_count", "node_count", "source_count", "embedding_dimension"}},
        )

    def maintain(
        self,
        decay: bool = True,
        evict: bool = False,
        recompute_embeddings: bool = False,
    ) -> dict[str, Any]:
        """Trigger maintenance operations on the engine."""
        results: dict[str, Any] = {}
        if decay:
            results["decay"] = self._request("POST", "/maintenance/decay", {})
        if evict:
            results["evict"] = self._request("POST", "/maintenance/evict", {})
        if recompute_embeddings:
            results["recompute"] = self._request(
                "POST", "/maintenance/recompute-embeddings", {}
            )
        return results
