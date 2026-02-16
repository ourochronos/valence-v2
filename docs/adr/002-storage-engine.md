# ADR-002: Storage Engine Selection

**Date:** 2026-02-16  
**Status:** Proposed  
**Deciders:** Chris  

## Context

Valence v2 requires a graph storage engine that can serve as a triple store with the following critical capabilities:

1. **Fast graph traversal** — Multi-hop queries must be efficient for semantic navigation and reasoning
2. **Vector support** — Built-in approximate nearest neighbor (ANN) search for topology-derived embeddings
3. **Embeddability in Rust** — Single-process architecture without external database dependencies
4. **Durability** — Persistent storage beyond in-memory representations
5. **Performance** — Ability to handle knowledge graphs with millions of triples

The storage layer must support both the semantic triple model (subject-predicate-object) and efficient vector similarity search for embedding-based retrieval.

## Decision

**We will use Kuzu as the primary graph storage engine for Valence v2.**

### Key Characteristics

- **Embedded database** — Runs in-process, no separate server required
- **Column-oriented storage** — Optimized for analytical queries and bulk operations
- **MIT License** — Permissive licensing suitable for our use case
- **Built-in HNSW** — Hierarchical Navigable Small World index for vector ANN search
- **Exceptional performance** — Benchmarks show 18-188x faster than Neo4j on graph analytics workloads
- **Rust bindings** — Available via `kuzu` crate v0.11.2

### Triple Store Mapping

Kuzu uses a property graph model. We map RDF-style triples to this model:

- **Subjects and Objects** → Graph nodes (with type discrimination)
- **Predicates** → Typed, directed edges between nodes
- **Schema design** → Optimized for SPO (subject-predicate-object) query patterns

This approach leverages Kuzu's native graph traversal while maintaining semantic triple semantics at the application layer.

### Vector Support

Kuzu's built-in HNSW index provides:
- Approximate nearest neighbor search on embedding vectors
- Integration with topology-derived embeddings (e.g., from Node2Vec, spectral methods)
- Single query language for both graph traversal and vector similarity

This eliminates the need for a separate vector database and enables hybrid queries combining structural and semantic similarity.

## Alternatives Considered

### PostgreSQL
- **Rejected:** Not embeddable; requires separate process
- Graph traversal via recursive CTEs has O(log n) overhead per hop through btree joins
- Multi-process architecture adds deployment complexity

### Oxigraph
- Native RDF triple store with SPARQL support
- **Rejected:** No built-in vector search — a critical gap for embedding-based retrieval
- Would require separate vector store, complicating architecture

### SurrealDB
- Multi-model database with graph capabilities
- **Rejected:** Not sufficiently graph-optimized; graph features feel bolted-on
- Performance characteristics unclear for deep traversal

### Pure petgraph (in-memory only)
- Excellent for algorithms but lacks durability
- **Rejected:** No persistence layer; unsuitable as primary storage
- Will still use petgraph for in-memory graph algorithms alongside Kuzu

## Consequences

### Positive

- **Single-process architecture** — Rust application with embedded Kuzu, no external dependencies
- **No separate database to deploy** — Simplified operations and development workflow
- **Column-oriented storage** — Efficient for bulk embedding computation and analytical queries
- **Unified query interface** — Both graph traversal and vector search in one system
- **Performance headroom** — Benchmark results suggest excellent scalability

### Negative

- **Newer ecosystem** — Kuzu is less mature than Neo4j or PostgreSQL
- **Rust bindings maturity** — `kuzu` crate is at v0.11.2; API may evolve
- **Limited SPARQL support** — Native query language is Cypher-like, not SPARQL (acceptable trade-off)
- **Schema design required** — Must carefully design property graph schema for triple patterns

### Neutral

- **Learning curve** — Team must learn Kuzu's query language and embedding features
- **Migration path** — If Kuzu proves insufficient, triple abstraction layer allows switching storage backends

## Review Notes

_[Reserved for Chris to fill in during review]_

---

**References:**
- Kuzu project: https://kuzudb.com/
- Kuzu Rust bindings: https://crates.io/crates/kuzu
- Benchmark results: [internal research findings]
