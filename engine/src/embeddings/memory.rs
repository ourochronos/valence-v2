//! In-memory embedding store with brute-force cosine similarity search.
//!
//! This is a simple implementation for small to medium graphs.
//! For larger graphs, consider using approximate nearest neighbor methods
//! like HNSW or IVF, or vector databases like qdrant or milvus.

use std::collections::HashMap;
use anyhow::{Result, bail};

use crate::models::NodeId;
use super::EmbeddingStore;

/// In-memory storage for node embeddings
#[derive(Debug, Clone)]
pub struct MemoryEmbeddingStore {
    embeddings: HashMap<NodeId, Vec<f32>>,
    dimensions: Option<usize>,
}

impl MemoryEmbeddingStore {
    /// Create a new empty embedding store
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
            dimensions: None,
        }
    }
    
    /// Create a new embedding store from a pre-computed map
    pub fn from_embeddings(embeddings: HashMap<NodeId, Vec<f32>>) -> Result<Self> {
        // Validate that all embeddings have the same dimensionality
        let dimensions = embeddings
            .values()
            .next()
            .map(|v| v.len());
        
        if let Some(dim) = dimensions {
            for (node_id, embedding) in &embeddings {
                if embedding.len() != dim {
                    bail!(
                        "Inconsistent embedding dimensions: expected {}, got {} for node {:?}",
                        dim, embedding.len(), node_id
                    );
                }
            }
        }
        
        Ok(Self {
            embeddings,
            dimensions,
        })
    }
}

impl Default for MemoryEmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingStore for MemoryEmbeddingStore {
    fn store(&mut self, node_id: NodeId, vector: Vec<f32>) -> Result<()> {
        // Check dimensionality consistency
        if let Some(expected_dim) = self.dimensions {
            if vector.len() != expected_dim {
                bail!(
                    "Dimension mismatch: expected {}, got {}",
                    expected_dim, vector.len()
                );
            }
        } else {
            // First embedding sets the dimensionality
            self.dimensions = Some(vector.len());
        }
        
        self.embeddings.insert(node_id, vector);
        Ok(())
    }
    
    fn get(&self, node_id: NodeId) -> Option<&Vec<f32>> {
        self.embeddings.get(&node_id)
    }
    
    fn query_nearest(&self, query: &[f32], k: usize) -> Result<Vec<(NodeId, f32)>> {
        if self.embeddings.is_empty() {
            return Ok(Vec::new());
        }
        
        // Check query dimensionality
        if let Some(expected_dim) = self.dimensions {
            if query.len() != expected_dim {
                bail!(
                    "Query dimension mismatch: expected {}, got {}",
                    expected_dim, query.len()
                );
            }
        }
        
        // Compute similarities for all nodes
        let mut similarities: Vec<(NodeId, f32)> = self.embeddings
            .iter()
            .map(|(&node_id, embedding)| {
                let similarity = cosine_similarity(query, embedding);
                (node_id, similarity)
            })
            .collect();
        
        // Sort by similarity (descending)
        similarities.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Take top k
        similarities.truncate(k);
        
        Ok(similarities)
    }
    
    fn all_embeddings(&self) -> HashMap<NodeId, Vec<f32>> {
        self.embeddings.clone()
    }
    
    fn len(&self) -> usize {
        self.embeddings.len()
    }
}

/// Compute cosine similarity between two vectors
/// Returns value in range [-1, 1] where 1 = identical, -1 = opposite
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }
    
    dot_product / (magnitude_a * magnitude_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_store_and_retrieve() {
        let mut store = MemoryEmbeddingStore::new();
        let node_id = Uuid::new_v4();
        let embedding = vec![1.0, 2.0, 3.0];
        
        store.store(node_id, embedding.clone()).unwrap();
        
        let retrieved = store.get(node_id);
        assert_eq!(retrieved, Some(&embedding));
    }

    #[test]
    fn test_dimension_consistency() {
        let mut store = MemoryEmbeddingStore::new();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        
        // First embedding: 3 dimensions
        store.store(node1, vec![1.0, 2.0, 3.0]).unwrap();
        
        // Second embedding: different dimensions should fail
        let result = store.store(node2, vec![1.0, 2.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_query_nearest_basic() {
        let mut store = MemoryEmbeddingStore::new();
        
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let node3 = Uuid::new_v4();
        
        // Node1: [1, 0, 0]
        store.store(node1, vec![1.0, 0.0, 0.0]).unwrap();
        // Node2: [0.9, 0.1, 0] - very similar to node1
        store.store(node2, vec![0.9, 0.1, 0.0]).unwrap();
        // Node3: [0, 0, 1] - orthogonal to node1
        store.store(node3, vec![0.0, 0.0, 1.0]).unwrap();
        
        // Query with vector similar to node1
        let query = vec![1.0, 0.0, 0.0];
        let results = store.query_nearest(&query, 3).unwrap();
        
        // Should return all 3, sorted by similarity
        assert_eq!(results.len(), 3);
        
        // First result should be node1 (identical)
        assert_eq!(results[0].0, node1);
        assert!((results[0].1 - 1.0).abs() < 0.001, "Expected similarity ~1.0, got {}", results[0].1);
        
        // Second should be node2 (similar)
        assert_eq!(results[1].0, node2);
        assert!(results[1].1 > 0.9, "Expected similarity > 0.9, got {}", results[1].1);
        
        // Third should be node3 (orthogonal)
        assert_eq!(results[2].0, node3);
        assert!(results[2].1.abs() < 0.001, "Expected similarity ~0.0, got {}", results[2].1);
    }

    #[test]
    fn test_query_nearest_k_limit() {
        let mut store = MemoryEmbeddingStore::new();
        
        for i in 0..10 {
            let node_id = Uuid::new_v4();
            let embedding = vec![i as f32, 0.0, 0.0];
            store.store(node_id, embedding).unwrap();
        }
        
        let query = vec![5.0, 0.0, 0.0];
        let results = store.query_nearest(&query, 3).unwrap();
        
        // Should return exactly 3 results
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_query_empty_store() {
        let store = MemoryEmbeddingStore::new();
        let query = vec![1.0, 0.0, 0.0];
        let results = store.query_nearest(&query, 5).unwrap();
        
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_from_embeddings() {
        let mut embeddings = HashMap::new();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        
        embeddings.insert(node1, vec![1.0, 2.0, 3.0]);
        embeddings.insert(node2, vec![4.0, 5.0, 6.0]);
        
        let store = MemoryEmbeddingStore::from_embeddings(embeddings).unwrap();
        
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(node1), Some(&vec![1.0, 2.0, 3.0]));
    }

    #[test]
    fn test_from_embeddings_inconsistent_dimensions() {
        let mut embeddings = HashMap::new();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        
        embeddings.insert(node1, vec![1.0, 2.0, 3.0]);
        embeddings.insert(node2, vec![4.0, 5.0]); // Different dimension
        
        let result = MemoryEmbeddingStore::from_embeddings(embeddings);
        assert!(result.is_err());
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!((sim - 1.0).abs() < 0.001);
        
        // Orthogonal vectors
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]);
        assert!(sim.abs() < 0.001);
        
        // Opposite vectors
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[-1.0, 0.0, 0.0]);
        assert!((sim + 1.0).abs() < 0.001);
        
        // Zero vector
        let sim = cosine_similarity(&[0.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert_eq!(sim, 0.0);
    }

    // === EDGE CASE TESTS ===

    #[test]
    fn test_query_nearest_empty_store() {
        let store = MemoryEmbeddingStore::new();
        
        // Query on empty store should return empty results
        let query = vec![1.0, 0.0, 0.0];
        let results = store.query_nearest(&query, 5).unwrap();
        
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_nearest_k_greater_than_store_size() {
        let mut store = MemoryEmbeddingStore::new();
        
        // Add only 2 embeddings
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();
        
        store.store(node1, vec![1.0, 0.0, 0.0]).unwrap();
        store.store(node2, vec![0.0, 1.0, 0.0]).unwrap();
        
        // Request k=10 (more than available)
        let query = vec![1.0, 0.0, 0.0];
        let results = store.query_nearest(&query, 10).unwrap();
        
        // Should return only 2 results (all available)
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_nearest_dimension_mismatch() {
        let mut store = MemoryEmbeddingStore::new();
        
        let node1 = uuid::Uuid::new_v4();
        store.store(node1, vec![1.0, 2.0, 3.0]).unwrap();
        
        // Query with different dimensions
        let query = vec![1.0, 0.0]; // 2D instead of 3D
        let result = store.query_nearest(&query, 5);
        
        // Should error on dimension mismatch
        assert!(result.is_err());
    }

    #[test]
    fn test_store_dimension_mismatch() {
        let mut store = MemoryEmbeddingStore::new();
        
        let node1 = uuid::Uuid::new_v4();
        let node2 = uuid::Uuid::new_v4();
        
        // First embedding sets dimensionality to 3
        store.store(node1, vec![1.0, 2.0, 3.0]).unwrap();
        
        // Second embedding with different dimension should fail
        let result = store.store(node2, vec![1.0, 2.0]);
        assert!(result.is_err());
        
        // Should still only have 1 embedding
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_query_nearest_k_zero() {
        let mut store = MemoryEmbeddingStore::new();
        
        let node1 = uuid::Uuid::new_v4();
        store.store(node1, vec![1.0, 0.0, 0.0]).unwrap();
        
        // Request k=0
        let query = vec![1.0, 0.0, 0.0];
        let results = store.query_nearest(&query, 0).unwrap();
        
        // Should return empty (truncated to 0)
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_nonexistent_node() {
        let store = MemoryEmbeddingStore::new();
        
        let fake_id = uuid::Uuid::new_v4();
        let result = store.get(fake_id);
        
        assert!(result.is_none());
    }

    #[test]
    fn test_all_embeddings_empty() {
        let store = MemoryEmbeddingStore::new();
        
        let all = store.all_embeddings();
        assert_eq!(all.len(), 0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        // Different length vectors should return 0.0
        let sim = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]);
        assert_eq!(sim, 0.0);
    }
}
