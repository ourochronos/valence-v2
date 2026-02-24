# Valence v2 — Knowledge Substrate

Triple-based knowledge engine in Rust with topology-derived embeddings, P2P federation, and bounded memory.

## Quick Reference

- **Language**: Rust (edition 2021)
- **Toolchain**: `source ~/.cargo/env` before cargo commands
- **Postgres**: localhost:5434, user=valence, password=valence, db=valence_v2
- **Build**: `cargo check` / `cargo build`
- **Test**: `cargo test` (276+ tests)
- **Test with Postgres**: `cargo test --features postgres`
- **Test with Federation**: `cargo test --features federation`
- **CLI**: Go project in `cli/`, build with `cd cli && go build -o valence-cli .`

## Architecture

```
valence-engine (Rust)
├── models/       — Triple, Node, Source + identity, VKB, trust, sharing, compliance types
├── storage/      — TripleStore trait, MemoryStore, PgStore
├── tiered_store/ — Hot/cold with promotion/demotion
├── graph/        — PageRank, centrality, shortest path, confidence scoring
├── embeddings/   — Spectral (graph Laplacian) + Node2Vec, no external LLM
├── query/        — Multi-dimensional fusion scoring
├── context/      — Context assembly for LLM agents, working sets
├── stigmergy/    — Access tracking, co-retrieval clustering
├── lifecycle/    — Decay + eviction = bounded memory
├── inference/    — Feedback loop: usage patterns improve retrieval
├── budget/       — Budget-bounded tiered retrieval
├── resilience/   — Graceful degradation (Full → Cold → Minimal → Offline)
├── identity/     — DID-based identity, Ed25519 keypairs (ed25519-dalek)
├── vkb/          — Sessions, exchanges, patterns, insights
├── federation/   — P2P via libp2p (gossipsub, kademlia, request-response)
├── trust/        — Trust edges, reputation, verification, disputes
├── sharing/      — Consent chains, sharing intents, access control
├── compliance/   — GDPR tombstones, consent records, audit log
├── api/          — HTTP endpoints (axum)
├── mcp/          — MCP server (rmcp, stdio transport)
├── engine.rs     — Unified lifecycle orchestrator
├── config.rs     — TOML + env + CLI configuration
└── error.rs      — Error hierarchy (thiserror)

cli/ (Go)
├── cmd/          — Cobra commands wrapping the HTTP API
├── client/       — HTTP client + types
└── config/       — CLI configuration (viper)

tools/
└── migrate-v1/   — v1 belief → v2 triple migration
```

## Design Philosophy

- **Triples, not beliefs.** Atomic (S, P, O) facts with separate provenance. No opinions in the data model.
- **Confidence from topology, not metadata.** Same triple scores differently depending on query context.
- **Bounded memory.** Decay + eviction = forgetting. The system stays bounded without manual cleanup.
- **Self-reinforcing.** Access patterns reshape the graph (stigmergy). The system improves with use.
- **Self-contained embeddings.** Graph Laplacian eigendecomposition. No external LLM API needed.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| petgraph | In-memory graph algorithms |
| axum | HTTP server |
| rmcp | MCP (Model Context Protocol) server |
| faer | Linear algebra for spectral embeddings |
| sqlx | PostgreSQL (feature-gated) |
| libp2p | P2P federation (feature-gated) |
| ed25519-dalek | Identity / signing |
| serde / schemars | Serialization + JSON Schema |
| tokio | Async runtime |

## Feature Flags

- `default` — In-memory storage, no network
- `postgres` — PostgreSQL backend via sqlx
- `federation` — P2P networking via libp2p

## Code Conventions

- Line length: not enforced, but keep readable
- Error handling: thiserror for library errors, anyhow for application code
- Async: tokio runtime, async-trait for trait objects
- Testing: `#[tokio::test]` for async, in-memory stores for unit tests
- Modules: one domain per module, focused traits, implementations in submodules
