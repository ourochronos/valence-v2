//! Valence Engine: Triple-based knowledge substrate with topology-derived embeddings.
//!
//! The engine is the deterministic core — insert, link, retrieve, cluster, decay, evict.
//! Inference (LLM) exists only at the boundary: decomposing natural language to triples
//! on write, recomposing triples to natural language on read.

pub mod error;       // Error types
pub mod models;
pub mod storage;
pub mod graph;       // In-memory graph algorithms (petgraph)
pub mod api;         // HTTP server for MCP
pub mod embeddings;  // Topology-derived embeddings
pub mod engine;      // Unified engine: storage + embeddings + lifecycle
pub mod mcp;         // MCP (Model Context Protocol) server for OpenClaw integration
pub mod stigmergy;   // Access tracking and co-retrieval clustering
// pub mod query;       // Hybrid retrieval (vector + graph)

pub use error::{ValenceError, Result};
pub use models::{Triple, Node, Source};
pub use storage::TripleStore;
pub use graph::{GraphView, ConfidenceScore};
pub use engine::ValenceEngine;
pub use stigmergy::{AccessTracker, CoRetrievalEngine};
