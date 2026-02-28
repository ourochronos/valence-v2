# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] — 2026-02-28

This is the first stable release of Valence v2, a ground-up rewrite that
replaces the v1 tool-heavy architecture with a lean, topology-driven knowledge
substrate.

### Added

- **Triple store** — atomic `(subject, predicate, object)` facts with pluggable
  storage backends: in-memory (default) and PostgreSQL (`--features postgres`).
- **Spectral embeddings** — deterministic semantic search via Graph Laplacian
  eigendecomposition using [faer](https://github.com/sarah-ek/faer-rs).  No
  external language model required.
- **Dynamic confidence scoring** — topology-derived trust: source reliability,
  path diversity, and betweenness centrality combine into a single per-triple
  score that varies with query context.
- **Graph algorithms** — PageRank, connected components, shortest path, and
  betweenness centrality via [petgraph](https://github.com/petgraph/petgraph).
- **HTTP API** (axum) — 7 endpoints replacing v1's 56 tools:
  `POST /triples`, `GET /triples`, `GET /nodes/{id}/neighbors`,
  `GET /triples/{id}/sources`, `POST /search`, `GET /stats`,
  `POST /maintenance/*`.
- **MCP server** (stdio transport) — 7 MCP tools for Claude / OpenClaw
  integration: `insert_triples`, `query_triples`, `search`, `neighbors`,
  `sources`, `stats`, `maintain`.
- **Bounded memory** — weight decay + LRU eviction keep the graph bounded
  without manual intervention (stigmergic self-maintenance).
- **Python client library** (`valence-v2` on PyPI) — zero-dependency stdlib
  HTTP client (`ValenceClient`) with typed models (`Triple`, `SearchResult`,
  `EngineStats`).
- **Go CLI** (`cli/`) — command-line interface for scripting and shell
  integration.
- **Docker Compose** — production-ready stack with PostgreSQL.
- **OpenClaw plugin manifest** (`plugin/openclaw.plugin.json`).
- 101 unit + integration tests.

### Changed

- Complete rewrite from v1 (Python/SQLite) → v2 (Rust/PostgreSQL-optional).
- API surface reduced from 56 tools to 7 endpoints / 7 MCP tools.
- Embeddings are now graph-topology-derived (spectral) rather than LLM-based.

### Removed

- All v1 Python source code (superseded by Rust engine).
- LLM-dependent embedding pipeline.

---

[2.0.0]: https://github.com/ourochronos/valence-v2/releases/tag/v2.0.0
