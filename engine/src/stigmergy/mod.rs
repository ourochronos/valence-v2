//! Stigmergy: Usage → Structure self-organizing loop.
//!
//! This module implements the second self-closing loop from the design docs:
//! triples that are accessed together become structurally closer in the graph,
//! while triples that are never accessed decay and eventually evict.
//!
//! The stigmergy module provides:
//! - Access tracking: record which triples are retrieved together
//! - Co-retrieval clustering: create edges between frequently co-accessed triples
//!
//! This makes the graph structurally reflect usage patterns, enabling the system
//! to self-organize based on how knowledge is actually used.

pub mod access_tracker;
pub mod co_retrieval;

pub use access_tracker::{AccessTracker, AccessTrackerConfig};
pub use co_retrieval::{CoRetrievalEngine, CoRetrievalConfig};
