//! Topology-derived embeddings: vector space emerges from graph structure.
//!
//! Unlike traditional RAG (which uses external LLM embeddings like BERT or Ada),
//! valence derives embeddings directly from the graph's topology. This eliminates
//! dependency on external models, enables deterministic computation, and ensures
//! that vector similarity is grounded in actual structural relationships.

pub mod spectral;
pub mod memory;
pub mod node2vec;

use std::collections::HashMap;
use anyhow::Result;
use crate::models::NodeId;

/// EmbeddingStore trait: manages storage and retrieval of node embeddings.
pub trait EmbeddingStore: Send + Sync {
    /// Store an embedding vector for a node
    fn store(&mut self, node_id: NodeId, vector: Vec<f32>) -> Result<()>;
    
    /// Get the embedding for a node, if it exists
    fn get(&self, node_id: NodeId) -> Option<&Vec<f32>>;
    
    /// Query for k nearest neighbors to a query vector
    /// Returns list of (NodeId, similarity_score) sorted by descending similarity
    fn query_nearest(&self, query: &[f32], k: usize) -> Result<Vec<(NodeId, f32)>>;
    
    /// Get all stored embeddings
    fn all_embeddings(&self) -> HashMap<NodeId, Vec<f32>>;
    
    /// Number of stored embeddings
    fn len(&self) -> usize;
    
    /// Check if store is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
