//! Query and retrieval subsystem.
//!
//! This module provides multi-dimensional fusion scoring for knowledge graph retrieval.

pub mod fusion;

pub use fusion::{FusionConfig, FusionScorer, RetrievalSignals};
