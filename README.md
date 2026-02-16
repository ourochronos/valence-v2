# Valence v2

Triple-based knowledge substrate with topology-derived embeddings.

## What Is This?

Valence v2 is a knowledge engine that stores information as `(subject, predicate, object)` triples and derives meaning from graph structure rather than external language models. Embeddings come from the topology itself — spectral decomposition of the graph Laplacian — making semantic search deterministic and self-contained.

## Architecture

```
┌─────────────────────────────────────┐
│           HTTP API (axum)           │
│  /triples /search /stats /maint    │
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
cargo run -- --port 8421
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
│   ├── models/       # Triple, Node, Source types
│   ├── storage/      # TripleStore trait, MemoryStore, PgStore
│   ├── engine.rs     # ValenceEngine (unified lifecycle)
│   ├── error.rs      # Error types
│   └── main.rs       # Binary entrypoint
├── tests/
│   └── integration.rs
docs/
├── api-design.md
├── bricks-architecture.md
├── satellite-repos.md
├── requirements.md
└── adr/
```

## License

MIT
