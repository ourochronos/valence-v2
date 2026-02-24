//! Query and retrieval subsystem.
//!
//! This module provides multi-dimensional fusion scoring for knowledge graph retrieval.

pub mod fusion;
pub mod combined;

pub use fusion::{FusionConfig, FusionScorer, RetrievalSignals, EmbeddingBlendConfig, EmbeddingBlender, StrategyScores};
pub use combined::{CombinedQueryParams, CombinedQueryResult, CombinedQueryResponse, combined_query};
