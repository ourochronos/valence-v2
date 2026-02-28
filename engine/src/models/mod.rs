//! Core data models for the knowledge graph.
//!
//! This module defines the fundamental types: [`Node`], [`Triple`], and [`Source`].

pub mod triple;
pub mod source;

pub use triple::{Triple, TripleId, Node, NodeId, Predicate};
pub use source::{Source, SourceId, SourceType, MAX_CHAIN_DEPTH};
