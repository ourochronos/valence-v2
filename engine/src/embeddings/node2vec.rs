//! Node2Vec: Random walk-based embeddings (stub for future implementation).
//!
//! Approach:
//! 1. Generate biased random walks from each node
//! 2. Treat walks as "sentences", nodes as "words"
//! 3. Train skip-gram model to predict context
//! 4. Node embeddings = skip-gram vectors
//!
//! Properties:
//! - Captures local neighborhood structure
//! - Parameters: walk length, walks per node, p (return param), q (in-out param)
//! - More flexible than spectral (can tune exploration vs exploitation)
//!
//! TODO: Implement when needed for comparison with spectral embeddings

use std::collections::HashMap;
use anyhow::Result;
use crate::models::NodeId;
use crate::storage::MemoryStore;

/// Configuration for Node2Vec embeddings
#[derive(Debug, Clone)]
pub struct Node2VecConfig {
    pub dimensions: usize,
    pub walk_length: usize,
    pub walks_per_node: usize,
    pub window_size: usize,
    pub p: f64,  // Return parameter
    pub q: f64,  // In-out parameter
}

impl Default for Node2VecConfig {
    fn default() -> Self {
        Self {
            dimensions: 64,
            walk_length: 80,
            walks_per_node: 10,
            window_size: 10,
            p: 1.0,
            q: 1.0,
        }
    }
}

/// Compute Node2Vec embeddings (stub - not yet implemented)
pub async fn compute_embeddings(
    _store: &MemoryStore,
    _config: Node2VecConfig,
) -> Result<HashMap<NodeId, Vec<f32>>> {
    unimplemented!("Node2Vec embeddings not yet implemented. Use spectral embeddings for now.")
}
