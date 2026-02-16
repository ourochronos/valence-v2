//! Valence Engine: Triple-based knowledge substrate with topology-derived embeddings.
//!
//! The engine is the deterministic core — insert, link, retrieve, cluster, decay, evict.
//! Inference (LLM) exists only at the boundary: decomposing natural language to triples
//! on write, recomposing triples to natural language on read.

pub mod error;       // Error types
pub mod models;
pub mod storage;
pub mod config;      // Unified configuration system
pub mod tiered_store; // Cold/warm split: hot (memory) + cold (postgres) with promotion/demotion
pub mod graph;       // In-memory graph algorithms (petgraph)
pub mod api;         // HTTP server for MCP
pub mod embeddings;  // Topology-derived embeddings
pub mod engine;      // Unified engine: storage + embeddings + lifecycle
pub mod mcp;         // MCP (Model Context Protocol) server for OpenClaw integration
pub mod stigmergy;   // Access tracking and co-retrieval clustering
pub mod budget;      // Budget-bounded operations and tiered retrieval
pub mod context;     // Context assembly: the read boundary for LLM agents
pub mod lifecycle;   // Knowledge lifecycle: structural decay + bounded memory
pub mod query;       // Hybrid retrieval with multi-dimensional fusion scoring
pub mod resilience;  // Graceful degradation and fallback strategies
pub mod inference;   // Inference training loop: query patterns feed back to improve retrieval

pub use error::{ValenceError, Result};
pub use models::{Triple, Node, Source};
pub use storage::TripleStore;
pub use config::EngineConfig;
pub use tiered_store::{TieredStore, TieredConfig, PromotionPolicy, DemotionPolicy, Tier};
pub use graph::{GraphView, ConfidenceScore};
pub use engine::ValenceEngine;
pub use stigmergy::{AccessTracker, CoRetrievalEngine};
pub use lifecycle::{LifecycleManager, DecayPolicy, MemoryBounds};
pub use query::{FusionConfig, FusionScorer, RetrievalSignals};
pub use resilience::{ResilienceManager, DegradationLevel, DegradationWarning};
pub use inference::{UsageFeedback, FeedbackSignal, FeedbackRecorder, WeightAdjuster, AdjustmentStrategy};
