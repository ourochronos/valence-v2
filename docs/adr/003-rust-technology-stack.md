# ADR-003: Rust Technology Stack

**Date:** 2026-02-16  
**Status:** Proposed  
**Deciders:** Chris  

## Context

Valence v2 is implemented in Rust and requires a coherent technology stack covering:

1. **API layer** — HTTP REST, Model Context Protocol (MCP), and gRPC interfaces
2. **Graph algorithms** — In-memory graph operations alongside persistent storage
3. **Linear algebra** — Spectral embeddings and matrix operations
4. **Vector indexing** — ANN search (potentially supplementing Kuzu's built-in capabilities)
5. **Graph embeddings** — Node2Vec and related topology-based embedding methods

This ADR documents the core Rust crates and libraries selected for these capabilities.

## Decision

### API Layer

- **axum** — HTTP REST API server
  - Ergonomic, composable middleware
  - Excellent performance and Tower ecosystem integration
  - Strong async/await support with Tokio runtime

- **rmcp** — Model Context Protocol implementation
  - Native MCP support for LLM integration
  - Enables Valence to expose semantic interfaces to language models

- **tonic** — gRPC framework
  - High-performance RPC for service-to-service communication
  - Protocol Buffers code generation
  - Streaming support for large result sets

### Graph Operations

- **petgraph** — In-memory graph algorithms
  - Complements Kuzu's persistent storage
  - Rich algorithm library (BFS, DFS, shortest paths, etc.)
  - Used for transient computations and algorithm implementations
  - Data can be loaded from Kuzu into petgraph for intensive in-memory analysis

### Linear Algebra

- **faer** — High-performance linear algebra
  - Fast matrix operations for spectral embedding methods
  - Modern, idiomatic Rust API
  - Excellent performance characteristics for eigenvalue decomposition
  - Used for Laplacian eigenmaps and spectral clustering

### Vector Indexing

- **usearch** — Vector similarity search
  - Fallback/supplement if Kuzu's built-in HNSW proves insufficient
  - High-performance HNSW implementation with Rust bindings
  - Flexible distance metrics (cosine, euclidean, etc.)
  - **Status:** Optional; evaluate after benchmarking Kuzu's native vector search

### Graph Embeddings

- **Custom Node2Vec implementation**
  - No mature Rust crate available for production use
  - Will implement custom Node2Vec based on:
    - Random walk generation (using petgraph)
    - Skip-gram training (potentially via word2vec-rs or custom implementation)
  - Allows fine-tuning for Valence's specific use cases
  - Integration with Kuzu for walk generation from persistent graphs

## Alternatives Considered

### actix-web (vs. axum)
- More mature, excellent performance
- **Rejected:** Less ergonomic API, more boilerplate than axum
- axum's Tower middleware ecosystem is more composable

### nalgebra (vs. faer)
- Well-established linear algebra library
- **Rejected:** Performance benchmarks favor faer for large-scale matrix operations
- faer's API is more modern and idiomatic

### qdrant-client (vs. usearch)
- Client for external Qdrant vector database
- **Rejected:** Adds external dependency; prefer embedded solutions
- usearch provides in-process indexing if needed

## Consequences

### Positive

- **Rust-native stack** — All components are Rust libraries, no FFI overhead
- **Single runtime** — Tokio-based async runtime across HTTP, gRPC, and MCP
- **Performance** — Native Rust performance without cross-language boundaries
- **Type safety** — End-to-end type checking from API to storage
- **Ecosystem alignment** — All crates align with modern Rust async ecosystem

### Negative

- **Custom Node2Vec** — Must implement and maintain our own embedding algorithm
- **faer maturity** — Newer library; less battle-tested than nalgebra
- **Dependency management** — Multiple crates to track and update
- **Learning curve** — Team must learn multiple library APIs

### Neutral

- **usearch uncertainty** — May not be needed if Kuzu's vector search suffices
- **Tower middleware** — Powerful but requires understanding Tower's abstraction layers
- **gRPC schema evolution** — Must manage Protocol Buffers versioning

## Implementation Notes

### Node2Vec Custom Implementation

Planned approach:
1. **Walk generation** — Use petgraph for biased random walks on graph loaded from Kuzu
2. **Sequence generation** — Convert walks to sequences of node identifiers
3. **Skip-gram training** — Implement or integrate skip-gram model (possibly via `word2vec-rs` or custom)
4. **Embedding storage** — Store resulting vectors in Kuzu with HNSW indexing

### Integration Architecture

```
┌─────────────────────────────────────────┐
│  API Layer (axum, rmcp, tonic)          │
├─────────────────────────────────────────┤
│  Business Logic (Rust application code) │
├─────────────────────────────────────────┤
│  Graph Algorithms (petgraph)            │
│  Linear Algebra (faer)                  │
│  Graph Embeddings (custom Node2Vec)     │
├─────────────────────────────────────────┤
│  Storage (Kuzu + optional usearch)      │
└─────────────────────────────────────────┘
```

## Review Notes

_[Reserved for Chris to fill in during review]_

---

**References:**
- axum: https://github.com/tokio-rs/axum
- tonic: https://github.com/hyperium/tonic
- rmcp: https://crates.io/crates/rmcp
- petgraph: https://github.com/petgraph/petgraph
- faer: https://github.com/sarah-ek/faer-rs
- usearch: https://github.com/unum-cloud/usearch
