//! Spring model: real-time incremental embeddings via physics-inspired nudging.
//!
//! On every triple insert, connected nodes' embeddings are nudged toward each other.
//! This is O(1) per edge — just two vector additions. The spring model provides
//! approximately-right embeddings that are corrected periodically by full spectral
//! or Node2Vec recomputation.
//!
//! Nodes without embeddings are lazily initialized from their neighbors' mean.

use std::collections::HashMap;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};

use crate::models::NodeId;
use super::EmbeddingStore;

/// Configuration for the spring model
#[derive(Debug, Clone)]
pub struct SpringConfig {
    /// Number of embedding dimensions (must match other strategies)
    pub dimensions: usize,
    /// Learning rate for spring nudges (default: 0.1)
    pub learning_rate: f32,
}

impl Default for SpringConfig {
    fn default() -> Self {
        Self {
            dimensions: 64,
            learning_rate: 0.1,
        }
    }
}

impl SpringConfig {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            ..Default::default()
        }
    }
}

/// A single embedding vector with metadata about which strategy produced it
/// and when it was last updated.
#[derive(Debug, Clone)]
pub struct TimestampedEmbedding {
    pub vector: Vec<f32>,
    pub updated_at: DateTime<Utc>,
}

impl TimestampedEmbedding {
    pub fn new(vector: Vec<f32>) -> Self {
        Self {
            vector,
            updated_at: Utc::now(),
        }
    }
}

/// Embedding strategy identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddingStrategy {
    Spring,
    Node2Vec,
    Spectral,
}

/// Per-node storage for multiple embedding vectors (one per strategy).
#[derive(Debug, Clone)]
pub struct NodeEmbeddings {
    pub spring: Option<TimestampedEmbedding>,
    pub node2vec: Option<TimestampedEmbedding>,
    pub spectral: Option<TimestampedEmbedding>,
}

impl NodeEmbeddings {
    pub fn new() -> Self {
        Self {
            spring: None,
            node2vec: None,
            spectral: None,
        }
    }

    /// Get embedding for a specific strategy
    pub fn get(&self, strategy: EmbeddingStrategy) -> Option<&TimestampedEmbedding> {
        match strategy {
            EmbeddingStrategy::Spring => self.spring.as_ref(),
            EmbeddingStrategy::Node2Vec => self.node2vec.as_ref(),
            EmbeddingStrategy::Spectral => self.spectral.as_ref(),
        }
    }

    /// Set embedding for a specific strategy
    pub fn set(&mut self, strategy: EmbeddingStrategy, embedding: TimestampedEmbedding) {
        match strategy {
            EmbeddingStrategy::Spring => self.spring = Some(embedding),
            EmbeddingStrategy::Node2Vec => self.node2vec = Some(embedding),
            EmbeddingStrategy::Spectral => self.spectral = Some(embedding),
        }
    }
}

impl Default for NodeEmbeddings {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-embedding store: each node carries up to three embedding vectors
/// (spring, node2vec, spectral), each with its own timestamp.
///
/// Also implements EmbeddingStore using the spring embeddings as the primary
/// embedding for backward compatibility.
#[derive(Debug, Clone)]
pub struct MultiEmbeddingStore {
    embeddings: HashMap<NodeId, NodeEmbeddings>,
    dimensions: usize,
    spring_config: SpringConfig,
}

impl MultiEmbeddingStore {
    /// Create a new multi-embedding store
    pub fn new(dimensions: usize) -> Self {
        Self {
            embeddings: HashMap::new(),
            dimensions,
            spring_config: SpringConfig::new(dimensions),
        }
    }

    /// Create with custom spring config
    pub fn with_config(config: SpringConfig) -> Self {
        let dimensions = config.dimensions;
        Self {
            embeddings: HashMap::new(),
            dimensions,
            spring_config: config,
        }
    }

    /// Get the full NodeEmbeddings for a node
    pub fn get_node_embeddings(&self, node_id: NodeId) -> Option<&NodeEmbeddings> {
        self.embeddings.get(&node_id)
    }

    /// Get a specific strategy's embedding for a node
    pub fn get_strategy(&self, node_id: NodeId, strategy: EmbeddingStrategy) -> Option<&Vec<f32>> {
        self.embeddings
            .get(&node_id)
            .and_then(|ne| ne.get(strategy))
            .map(|te| &te.vector)
    }

    /// Store a batch of embeddings for a given strategy (e.g., after spectral recompute)
    pub fn store_batch(
        &mut self,
        strategy: EmbeddingStrategy,
        embeddings: HashMap<NodeId, Vec<f32>>,
    ) -> Result<()> {
        for (node_id, vector) in embeddings {
            if vector.len() != self.dimensions {
                bail!(
                    "Dimension mismatch: expected {}, got {} for node {:?}",
                    self.dimensions, vector.len(), node_id
                );
            }
            let entry = self.embeddings.entry(node_id).or_insert_with(NodeEmbeddings::new);
            entry.set(strategy, TimestampedEmbedding::new(vector));
        }
        Ok(())
    }

    /// Spring nudge: after inserting triple (subject -> object), nudge both
    /// embeddings toward each other. If either lacks a spring embedding,
    /// lazy-initialize it.
    ///
    /// `neighbor_embeddings` provides spring embeddings of neighbors for lazy init.
    /// Returns true if any embeddings were updated.
    pub fn spring_nudge(
        &mut self,
        subject: NodeId,
        object: NodeId,
        edge_weight: f32,
        neighbor_spring_embeddings: &HashMap<NodeId, Vec<f32>>,
    ) -> bool {
        let lr = self.spring_config.learning_rate;
        let dims = self.dimensions;

        // Ensure both nodes have spring embeddings (lazy init if needed)
        self.ensure_spring_embedding(subject, neighbor_spring_embeddings);
        self.ensure_spring_embedding(object, neighbor_spring_embeddings);

        // Get both embeddings - we need to work with clones due to borrow checker
        let subj_vec = self.embeddings.get(&subject)
            .and_then(|ne| ne.spring.as_ref())
            .map(|te| te.vector.clone());
        let obj_vec = self.embeddings.get(&object)
            .and_then(|ne| ne.spring.as_ref())
            .map(|te| te.vector.clone());

        match (subj_vec, obj_vec) {
            (Some(s_emb), Some(o_emb)) => {
                // Nudge subject toward object
                let mut new_s = s_emb.clone();
                let mut new_o = o_emb.clone();

                for i in 0..dims {
                    let delta = lr * edge_weight * (o_emb[i] - s_emb[i]);
                    new_s[i] += delta;
                    new_o[i] -= delta; // Nudge object toward subject (symmetric)
                    // Correction: nudge object toward subject means:
                    // new_o[i] += lr * edge_weight * (s_emb[i] - o_emb[i])
                    // which equals: new_o[i] -= lr * edge_weight * (o_emb[i] - s_emb[i])
                    // So the -= delta is correct.
                }

                // Store updated embeddings
                let now = Utc::now();
                if let Some(ne) = self.embeddings.get_mut(&subject) {
                    ne.spring = Some(TimestampedEmbedding { vector: new_s, updated_at: now });
                }
                if let Some(ne) = self.embeddings.get_mut(&object) {
                    ne.spring = Some(TimestampedEmbedding { vector: new_o, updated_at: now });
                }

                true
            }
            _ => false,
        }
    }

    /// Ensure a node has a spring embedding. If it doesn't, initialize from
    /// the mean of its neighbors' spring embeddings. If no neighbors have
    /// spring embeddings either, initialize with small random values.
    fn ensure_spring_embedding(
        &mut self,
        node_id: NodeId,
        neighbor_spring_embeddings: &HashMap<NodeId, Vec<f32>>,
    ) {
        let has_spring = self.embeddings
            .get(&node_id)
            .and_then(|ne| ne.spring.as_ref())
            .is_some();

        if has_spring {
            return;
        }

        let dims = self.dimensions;

        // Try to initialize from neighbors' mean
        let neighbor_vecs: Vec<&Vec<f32>> = neighbor_spring_embeddings.values().collect();

        let init_vec = if !neighbor_vecs.is_empty() {
            // Mean of neighbor embeddings
            let mut mean = vec![0.0f32; dims];
            let count = neighbor_vecs.len() as f32;
            for nv in &neighbor_vecs {
                for (i, &val) in nv.iter().enumerate().take(dims) {
                    mean[i] += val;
                }
            }
            for val in &mut mean {
                *val /= count;
            }
            mean
        } else {
            // No neighbor embeddings — initialize with small random values
            // Use a deterministic-ish seed based on node_id for reproducibility
            let bytes = node_id.as_bytes();
            let mut vec = vec![0.0f32; dims];
            for i in 0..dims {
                // Simple hash-based pseudo-random: mix node_id bytes with dimension index
                let byte_idx = i % 16;
                let seed = bytes[byte_idx] as f32 / 255.0 - 0.5;
                vec[i] = seed * 0.1; // Small magnitude
            }
            vec
        };

        let entry = self.embeddings.entry(node_id).or_insert_with(NodeEmbeddings::new);
        entry.spring = Some(TimestampedEmbedding::new(init_vec));
    }

    /// Get all spring embeddings (useful for collecting neighbor embeddings)
    pub fn all_spring_embeddings(&self) -> HashMap<NodeId, Vec<f32>> {
        self.embeddings
            .iter()
            .filter_map(|(&node_id, ne)| {
                ne.spring.as_ref().map(|te| (node_id, te.vector.clone()))
            })
            .collect()
    }

    /// Get the number of nodes that have at least one embedding
    pub fn node_count(&self) -> usize {
        self.embeddings.len()
    }

    /// Get embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Query nearest neighbors using a specific strategy's embeddings
    pub fn query_nearest_by_strategy(
        &self,
        query: &[f32],
        k: usize,
        strategy: EmbeddingStrategy,
    ) -> Result<Vec<(NodeId, f32)>> {
        if query.len() != self.dimensions {
            bail!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimensions, query.len()
            );
        }

        let mut similarities: Vec<(NodeId, f32)> = self.embeddings
            .iter()
            .filter_map(|(&node_id, ne)| {
                ne.get(strategy).map(|te| {
                    let sim = cosine_similarity(query, &te.vector);
                    (node_id, sim)
                })
            })
            .collect();

        similarities.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        similarities.truncate(k);
        Ok(similarities)
    }
}

/// EmbeddingStore implementation using spring embeddings as primary.
/// This provides backward compatibility with code that uses the EmbeddingStore trait.
impl EmbeddingStore for MultiEmbeddingStore {
    fn store(&mut self, node_id: NodeId, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimensions {
            bail!(
                "Dimension mismatch: expected {}, got {}",
                self.dimensions, vector.len()
            );
        }
        let entry = self.embeddings.entry(node_id).or_insert_with(NodeEmbeddings::new);
        entry.spring = Some(TimestampedEmbedding::new(vector));
        Ok(())
    }

    fn get(&self, node_id: NodeId) -> Option<&Vec<f32>> {
        self.embeddings
            .get(&node_id)
            .and_then(|ne| ne.spring.as_ref())
            .map(|te| &te.vector)
    }

    fn query_nearest(&self, query: &[f32], k: usize) -> Result<Vec<(NodeId, f32)>> {
        self.query_nearest_by_strategy(query, k, EmbeddingStrategy::Spring)
    }

    fn all_embeddings(&self) -> HashMap<NodeId, Vec<f32>> {
        self.all_spring_embeddings()
    }

    fn len(&self) -> usize {
        // Count nodes that have at least a spring embedding
        self.embeddings
            .values()
            .filter(|ne| ne.spring.is_some())
            .count()
    }
}

/// Compute cosine similarity between two vectors
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
    fn test_multi_store_basic() {
        let mut store = MultiEmbeddingStore::new(3);
        let node = Uuid::new_v4();

        // Store via EmbeddingStore trait (goes to spring)
        store.store(node, vec![1.0, 2.0, 3.0]).unwrap();

        assert_eq!(store.get(node), Some(&vec![1.0, 2.0, 3.0]));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_multi_store_dimension_mismatch() {
        let mut store = MultiEmbeddingStore::new(3);
        let node = Uuid::new_v4();

        let result = store.store(node, vec![1.0, 2.0]); // Wrong dimensions
        assert!(result.is_err());
    }

    #[test]
    fn test_store_batch_strategies() {
        let mut store = MultiEmbeddingStore::new(3);
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        // Store spectral embeddings
        let mut spectral = HashMap::new();
        spectral.insert(node1, vec![1.0, 0.0, 0.0]);
        spectral.insert(node2, vec![0.0, 1.0, 0.0]);
        store.store_batch(EmbeddingStrategy::Spectral, spectral).unwrap();

        // Store node2vec embeddings
        let mut n2v = HashMap::new();
        n2v.insert(node1, vec![0.5, 0.5, 0.0]);
        n2v.insert(node2, vec![0.0, 0.5, 0.5]);
        store.store_batch(EmbeddingStrategy::Node2Vec, n2v).unwrap();

        // Verify both strategies stored
        assert_eq!(
            store.get_strategy(node1, EmbeddingStrategy::Spectral),
            Some(&vec![1.0, 0.0, 0.0])
        );
        assert_eq!(
            store.get_strategy(node1, EmbeddingStrategy::Node2Vec),
            Some(&vec![0.5, 0.5, 0.0])
        );

        // Spring should be None (not stored yet)
        assert_eq!(store.get_strategy(node1, EmbeddingStrategy::Spring), None);
    }

    #[test]
    fn test_spring_nudge_convergence() {
        let mut store = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 3,
            learning_rate: 0.1,
        });

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        // Set initial spring embeddings far apart
        store.store(a, vec![1.0, 0.0, 0.0]).unwrap();
        store.store(b, vec![0.0, 1.0, 0.0]).unwrap();

        let initial_sim = cosine_similarity(
            store.get(a).unwrap(),
            store.get(b).unwrap(),
        );

        // Nudge them toward each other multiple times
        for _ in 0..10 {
            let neighbor_embs = store.all_spring_embeddings();
            store.spring_nudge(a, b, 1.0, &neighbor_embs);
        }

        let final_sim = cosine_similarity(
            store.get(a).unwrap(),
            store.get(b).unwrap(),
        );

        // After nudging, they should be more similar
        assert!(
            final_sim > initial_sim,
            "Similarity should increase after nudging: initial={:.3}, final={:.3}",
            initial_sim, final_sim
        );
    }

    #[test]
    fn test_spring_nudge_symmetric() {
        let mut store = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 3,
            learning_rate: 0.1,
        });

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        store.store(a, vec![1.0, 0.0, 0.0]).unwrap();
        store.store(b, vec![0.0, 1.0, 0.0]).unwrap();

        let neighbor_embs = store.all_spring_embeddings();
        store.spring_nudge(a, b, 1.0, &neighbor_embs);

        let emb_a = store.get(a).unwrap();
        let emb_b = store.get(b).unwrap();

        // Both should have moved: A gained some of B's direction, B gained some of A's
        assert!(emb_a[1] > 0.0, "A should have gained some y-component from B");
        assert!(emb_b[0] > 0.0, "B should have gained some x-component from A");
    }

    #[test]
    fn test_lazy_init_from_neighbors() {
        let mut store = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 3,
            learning_rate: 0.1,
        });

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4(); // New node, no embedding yet

        // A and B have embeddings
        store.store(a, vec![1.0, 0.0, 0.0]).unwrap();
        store.store(b, vec![0.0, 1.0, 0.0]).unwrap();

        // C has no embedding yet
        assert!(store.get(c).is_none());

        // Nudge C toward A — C should be lazy-initialized
        // Provide A and B as neighbor embeddings for C
        let mut neighbor_embs = HashMap::new();
        neighbor_embs.insert(a, vec![1.0, 0.0, 0.0]);
        neighbor_embs.insert(b, vec![0.0, 1.0, 0.0]);

        store.spring_nudge(c, a, 1.0, &neighbor_embs);

        // C should now have an embedding (initialized from mean of A and B)
        let emb_c = store.get(c).unwrap();
        assert_eq!(emb_c.len(), 3);

        // The initial embedding should be close to the mean of neighbors
        // Mean of [1,0,0] and [0,1,0] = [0.5, 0.5, 0.0]
        // Then nudged toward A, so x should be > 0.5
        assert!(emb_c[0] > 0.0, "C should have non-zero x from neighbor mean + nudge");
    }

    #[test]
    fn test_lazy_init_no_neighbors() {
        let mut store = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 3,
            learning_rate: 0.1,
        });

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        // Neither has embeddings, no neighbors provided
        let empty_neighbors = HashMap::new();
        store.spring_nudge(a, b, 1.0, &empty_neighbors);

        // Both should have been initialized (with hash-based pseudo-random values)
        assert!(store.get(a).is_some(), "A should have been lazy-initialized");
        assert!(store.get(b).is_some(), "B should have been lazy-initialized");
    }

    #[test]
    fn test_multiple_inserts_track_structure() {
        let mut store = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 4,
            learning_rate: 0.1,
        });

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();

        // Initialize all with distinct embeddings
        store.store(a, vec![1.0, 0.0, 0.0, 0.0]).unwrap();
        store.store(b, vec![0.0, 1.0, 0.0, 0.0]).unwrap();
        store.store(c, vec![0.0, 0.0, 1.0, 0.0]).unwrap();
        store.store(d, vec![0.0, 0.0, 0.0, 1.0]).unwrap();

        // Create structure: A-B are connected, C-D are connected, but no cross-group edges
        for _ in 0..20 {
            let embs = store.all_spring_embeddings();
            store.spring_nudge(a, b, 1.0, &embs);
            let embs = store.all_spring_embeddings();
            store.spring_nudge(c, d, 1.0, &embs);
        }

        // A and B should be similar (same group)
        let sim_ab = cosine_similarity(store.get(a).unwrap(), store.get(b).unwrap());
        // C and D should be similar (same group)
        let sim_cd = cosine_similarity(store.get(c).unwrap(), store.get(d).unwrap());
        // A and C should be less similar (different groups)
        let sim_ac = cosine_similarity(store.get(a).unwrap(), store.get(c).unwrap());

        assert!(
            sim_ab > sim_ac,
            "Within-group similarity ({:.3}) should exceed cross-group ({:.3})",
            sim_ab, sim_ac
        );
        assert!(
            sim_cd > sim_ac,
            "Within-group similarity ({:.3}) should exceed cross-group ({:.3})",
            sim_cd, sim_ac
        );
    }

    #[test]
    fn test_query_nearest_by_strategy() {
        let mut store = MultiEmbeddingStore::new(3);

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let node3 = Uuid::new_v4();

        // Store spring embeddings
        store.store(node1, vec![1.0, 0.0, 0.0]).unwrap();
        store.store(node2, vec![0.9, 0.1, 0.0]).unwrap();
        store.store(node3, vec![0.0, 0.0, 1.0]).unwrap();

        // Store spectral embeddings (different arrangement)
        let mut spectral = HashMap::new();
        spectral.insert(node1, vec![0.0, 0.0, 1.0]);
        spectral.insert(node2, vec![0.0, 1.0, 0.0]);
        spectral.insert(node3, vec![0.9, 0.1, 0.0]);
        store.store_batch(EmbeddingStrategy::Spectral, spectral).unwrap();

        // Query by spring: node1 should be closest to [1,0,0]
        let spring_results = store.query_nearest_by_strategy(
            &[1.0, 0.0, 0.0], 3, EmbeddingStrategy::Spring
        ).unwrap();
        assert_eq!(spring_results[0].0, node1);

        // Query by spectral: node3 should be closest to [1,0,0] (spectral has it at [0.9,0.1,0])
        let spectral_results = store.query_nearest_by_strategy(
            &[1.0, 0.0, 0.0], 3, EmbeddingStrategy::Spectral
        ).unwrap();
        assert_eq!(spectral_results[0].0, node3);
    }

    #[test]
    fn test_node_embeddings_struct() {
        let mut ne = NodeEmbeddings::new();

        assert!(ne.get(EmbeddingStrategy::Spring).is_none());
        assert!(ne.get(EmbeddingStrategy::Node2Vec).is_none());
        assert!(ne.get(EmbeddingStrategy::Spectral).is_none());

        ne.set(EmbeddingStrategy::Spring, TimestampedEmbedding::new(vec![1.0, 2.0]));
        assert!(ne.get(EmbeddingStrategy::Spring).is_some());
        assert_eq!(ne.get(EmbeddingStrategy::Spring).unwrap().vector, vec![1.0, 2.0]);
    }

    #[test]
    fn test_edge_weight_affects_nudge_magnitude() {
        let mut store1 = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 3,
            learning_rate: 0.1,
        });
        let mut store2 = MultiEmbeddingStore::with_config(SpringConfig {
            dimensions: 3,
            learning_rate: 0.1,
        });

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        // Same initial state
        store1.store(a, vec![1.0, 0.0, 0.0]).unwrap();
        store1.store(b, vec![0.0, 1.0, 0.0]).unwrap();
        store2.store(a, vec![1.0, 0.0, 0.0]).unwrap();
        store2.store(b, vec![0.0, 1.0, 0.0]).unwrap();

        // Nudge with low weight
        let embs = store1.all_spring_embeddings();
        store1.spring_nudge(a, b, 0.1, &embs);

        // Nudge with high weight
        let embs = store2.all_spring_embeddings();
        store2.spring_nudge(a, b, 1.0, &embs);

        let sim1 = cosine_similarity(store1.get(a).unwrap(), store1.get(b).unwrap());
        let sim2 = cosine_similarity(store2.get(a).unwrap(), store2.get(b).unwrap());

        // Higher edge weight should produce more convergence
        assert!(
            sim2 > sim1,
            "Higher edge weight should produce more convergence: low_weight_sim={:.3}, high_weight_sim={:.3}",
            sim1, sim2
        );
    }
}
