# Valence v2

Triple-based knowledge substrate with topology-derived embeddings.

## What Is This?

Valence v2 is a knowledge engine that stores information as `(subject, predicate, object)` triples and derives meaning from graph structure rather than external language models. Embeddings come from the topology itself — spectral decomposition of the graph Laplacian — making semantic search deterministic and self-contained.

## Architecture

```
┌─────────────────────────────────────┐
│    HTTP API (axum) | MCP (stdio)   │
│  /triples /search /stats /maint    │
│  7 tools for OpenClaw integration  │
├─────────────────────────────────────┤
│          ValenceEngine              │
│   ┌───────────┐  ┌──────────────┐  │
│   │TripleStore│  │EmbeddingStore│  │
│   │(Memory/PG)│  │  (Spectral)  │  │
│   └───────────┘  └──────────────┘  │
│   ┌───────────┐  ┌──────────────┐  │
│   │   Graph   │  │  Confidence  │  │
│   │Algorithms │  │  (Dynamic)   │  │
│   └───────────┘  └──────────────┘  │
└─────────────────────────────────────┘
```

### Key Components

- **TripleStore** — Pluggable storage backend (in-memory or PostgreSQL)
- **Graph Algorithms** — PageRank, connected components, shortest path, betweenness centrality (via petgraph)
- **Dynamic Confidence** — Topology-derived trust scores: source reliability, path diversity, centrality
- **Spectral Embeddings** — Graph Laplacian eigendecomposition via faer; no external LLM needed
- **HTTP API** — 7 core operations (down from v1's 56 tools)

## Quick Start

### Native (development)

```bash
cd engine

# HTTP server (default)
cargo run -- --mode http --port 8421

# MCP stdio server (for Claude/OpenClaw)
cargo run -- --mode mcp

# Both HTTP + MCP
cargo run -- --mode both --port 8421
```

### With PostgreSQL

```bash
export DATABASE_URL="postgresql://user:pass@localhost:5433/valence_v2"
cargo run --features postgres -- --port 8421
```

### Docker Compose (production)

```bash
docker-compose up -d
```

## API

### HTTP Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/triples` | Insert triples with provenance |
| GET | `/triples?subject=X&predicate=Y&object=Z` | Query by pattern (wildcards) |
| GET | `/nodes/{id}/neighbors?depth=2` | K-hop subgraph traversal |
| GET | `/triples/{id}/sources` | Provenance retrieval |
| POST | `/search` | Semantic search via topology embeddings |
| GET | `/stats` | Engine statistics |
| POST | `/maintenance/decay` | Trigger weight decay |
| POST | `/maintenance/evict` | Garbage collection |
| POST | `/maintenance/recompute-embeddings` | Regenerate spectral embeddings |

### MCP Tools (for Claude/OpenClaw)

| Tool | Description |
|------|-------------|
| `insert_triples` | Insert triples with source provenance |
| `query_triples` | Pattern-match query (S/P/O wildcards) |
| `search` | Semantic search via topology embeddings |
| `neighbors` | K-hop subgraph traversal |
| `sources` | Get provenance for a triple |
| `stats` | Engine statistics |
| `maintain` | Run decay/eviction/recompute cycle |

See [`docs/mcp-integration.md`](docs/mcp-integration.md) for MCP setup and OpenClaw plugin installation.

## Design Philosophy

**Triples, not beliefs.** Atomic facts with separate provenance. No opinions baked into the data model.

**Confidence from topology, not metadata.** The same triple scores differently depending on query context — well-connected, multiply-sourced facts score higher than peripheral claims.

**Bounded memory.** Decay + eviction = forgetting. The system stays bounded without manual cleanup.

**Self-reinforcing loops.** Access patterns reshape the graph (stigmergy). The system improves with use.

## Tests

```bash
cargo test                          # 101 tests, all modules
cargo test --features postgres      # Include PostgreSQL backend tests (needs DB)
```

## Project Structure

```
engine/
├── src/
│   ├── api/          # HTTP endpoints (axum)
│   ├── embeddings/   # Spectral embeddings, embedding store
│   ├── graph/        # Algorithms, confidence, graph view
│   ├── mcp/          # MCP server (stdio transport)
│   ├── models/       # Triple, Node, Source types
│   ├── storage/      # TripleStore trait, MemoryStore, PgStore
│   ├── engine.rs     # ValenceEngine (unified lifecycle)
│   ├── error.rs      # Error types
│   └── main.rs       # Binary entrypoint
├── tests/
│   └── integration.rs
plugin/
└── openclaw.plugin.json  # OpenClaw MCP plugin manifest
docs/
├── api-design.md
├── mcp-integration.md     # MCP server documentation
├── bricks-architecture.md
├── satellite-repos.md
├── requirements.md
└── adr/
```

## License

MIT
