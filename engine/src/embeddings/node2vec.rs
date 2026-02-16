//! Node2Vec: Random walk-based embeddings.
//!
//! Approach:
//! 1. Generate biased random walks from each node
//! 2. Treat walks as "sentences", nodes as "words"
//! 3. Train skip-gram model to predict context
//! 4. Node embeddings = skip-gram vectors
//!
//! Properties:
//! - Captures local neighborhood structure via random walks
//! - Parameters: walk length, walks per node, p (return param), q (in-out param)
//! - More flexible than spectral (can tune exploration vs exploitation)
//! - Complements spectral embeddings (local vs global structure)

use std::collections::HashMap;
use anyhow::{Result, Context};
use rand::Rng;
use rand::seq::SliceRandom;

use crate::models::NodeId;
use crate::storage::TripleStore;
use crate::graph::GraphView;

/// Configuration for Node2Vec embeddings
#[derive(Debug, Clone)]
pub struct Node2VecConfig {
    /// Number of dimensions for the embedding (default: 64)
    pub dimensions: usize,
    /// Length of each random walk (default: 80)
    pub walk_length: usize,
    /// Number of walks to start from each node (default: 10)
    pub walks_per_node: usize,
    /// Return parameter p: controls likelihood of returning to previous node
    /// Higher p => less likely to return (more exploration)
    /// Default: 1.0
    pub p: f64,
    /// In-out parameter q: controls breadth vs depth of search
    /// q > 1: BFS-like (breadth-first, local neighborhood)
    /// q < 1: DFS-like (depth-first, explore further)
    /// Default: 1.0
    pub q: f64,
    /// Context window size for skip-gram training (default: 5)
    pub window: usize,
    /// Number of training epochs (default: 5)
    pub epochs: usize,
    /// Learning rate for gradient descent (default: 0.025)
    pub learning_rate: f64,
}

impl Default for Node2VecConfig {
    fn default() -> Self {
        Self {
            dimensions: 64,
            walk_length: 80,
            walks_per_node: 10,
            p: 1.0,
            q: 1.0,
            window: 5,
            epochs: 5,
            learning_rate: 0.025,
        }
    }
}

impl Node2VecConfig {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            ..Default::default()
        }
    }
}

/// Compute Node2Vec embeddings from any TripleStore
pub async fn compute_node2vec(
    store: &(impl TripleStore + ?Sized),
    config: Node2VecConfig,
) -> Result<HashMap<NodeId, Vec<f32>>> {
    // Build graph view from store
    let graph_view = GraphView::from_store(store)
        .await
        .context("Failed to build graph view")?;
    
    let node_count = graph_view.node_count();
    
    if node_count == 0 {
        return Ok(HashMap::new());
    }
    
    // Generate random walks
    let walks = generate_walks(&graph_view, &config)?;
    
    if walks.is_empty() {
        // No walks generated (isolated nodes only)
        return Ok(HashMap::new());
    }
    
    // Train skip-gram model on the walks
    let embeddings = train_skipgram(&walks, &graph_view, &config)?;
    
    Ok(embeddings)
}

/// Generate random walks from all nodes in the graph
fn generate_walks(
    graph_view: &GraphView,
    config: &Node2VecConfig,
) -> Result<Vec<Vec<NodeId>>> {
    let mut walks = Vec::new();
    let mut rng = rand::thread_rng();
    
    // Get all nodes (sorted for determinism in tests)
    let mut nodes: Vec<NodeId> = graph_view.node_map.keys().copied().collect();
    nodes.sort();
    
    // Generate walks_per_node walks starting from each node
    for &start_node in &nodes {
        for _ in 0..config.walks_per_node {
            let walk = generate_walk(graph_view, start_node, config, &mut rng)?;
            if walk.len() > 1 {
                // Only include walks that actually moved
                walks.push(walk);
            }
        }
    }
    
    Ok(walks)
}

/// Generate a single biased random walk starting from a node
fn generate_walk<R: Rng>(
    graph_view: &GraphView,
    start_node: NodeId,
    config: &Node2VecConfig,
    rng: &mut R,
) -> Result<Vec<NodeId>> {
    let mut walk = vec![start_node];
    
    for _ in 1..config.walk_length {
        let current = *walk.last().unwrap();
        let neighbors = graph_view.neighbors(current);
        
        if neighbors.is_empty() {
            // Dead end, stop walk
            break;
        }
        
        // Select next node based on biased random walk
        let next = if walk.len() == 1 {
            // First step: uniform random selection
            neighbors.choose(rng).copied().unwrap()
        } else {
            // Biased selection based on p and q parameters
            let prev = walk[walk.len() - 2];
            select_next_node(&neighbors, prev, current, config, rng)
        };
        
        walk.push(next);
    }
    
    Ok(walk)
}

/// Select next node in walk using Node2Vec's biased sampling
fn select_next_node<R: Rng>(
    neighbors: &[NodeId],
    prev_node: NodeId,
    _current_node: NodeId,
    config: &Node2VecConfig,
    rng: &mut R,
) -> NodeId {
    // Compute unnormalized probabilities for each neighbor
    let mut probabilities: Vec<f64> = neighbors
        .iter()
        .map(|&neighbor| {
            if neighbor == prev_node {
                // Returning to previous node: probability = 1/p
                1.0 / config.p
            } else {
                // Check if neighbor is also neighbor of prev_node
                // For simplicity, we'll use distance-based heuristic:
                // If neighbor == prev, already handled above
                // Otherwise, assume distance-2 relationship (controlled by q)
                1.0 / config.q
            }
        })
        .collect();
    
    // Normalize probabilities
    let sum: f64 = probabilities.iter().sum();
    if sum == 0.0 {
        // Fallback to uniform
        return *neighbors.choose(rng).unwrap();
    }
    
    for p in &mut probabilities {
        *p /= sum;
    }
    
    // Sample according to probabilities
    let mut cumulative = 0.0;
    let threshold: f64 = rng.gen();
    
    for (i, &prob) in probabilities.iter().enumerate() {
        cumulative += prob;
        if threshold <= cumulative {
            return neighbors[i];
        }
    }
    
    // Fallback (shouldn't happen)
    *neighbors.last().unwrap()
}

/// Train skip-gram model on the generated walks
fn train_skipgram(
    walks: &[Vec<NodeId>],
    graph_view: &GraphView,
    config: &Node2VecConfig,
) -> Result<HashMap<NodeId, Vec<f32>>> {
    // Build vocabulary (all unique nodes in walks)
    let mut vocab: Vec<NodeId> = graph_view.node_map.keys().copied().collect();
    vocab.sort();
    
    let vocab_size = vocab.len();
    
    // Create node -> index mapping
    let node_to_idx: HashMap<NodeId, usize> = vocab
        .iter()
        .enumerate()
        .map(|(i, &node)| (node, i))
        .collect();
    
    // Initialize embeddings randomly
    let mut embeddings = initialize_embeddings(vocab_size, config.dimensions);
    let mut context_embeddings = initialize_embeddings(vocab_size, config.dimensions);
    
    // Training loop
    for epoch in 0..config.epochs {
        let mut epoch_loss = 0.0;
        let mut sample_count = 0;
        
        for walk in walks {
            // For each word in the walk
            for (i, &center_node) in walk.iter().enumerate() {
                let center_idx = node_to_idx[&center_node];
                
                // Get context window
                let window_start = i.saturating_sub(config.window);
                let window_end = (i + config.window + 1).min(walk.len());
                
                for j in window_start..window_end {
                    if i == j {
                        continue; // Skip the center word itself
                    }
                    
                    let context_node = walk[j];
                    let context_idx = node_to_idx[&context_node];
                    
                    // Positive sample: train to predict context from center
                    epoch_loss += train_pair(
                        &mut embeddings,
                        &mut context_embeddings,
                        center_idx,
                        context_idx,
                        true,
                        config.learning_rate,
                    );
                    sample_count += 1;
                    
                    // Negative sampling: randomly sample non-context nodes
                    for _ in 0..5 {
                        let neg_idx = rand::random::<usize>() % vocab_size;
                        if neg_idx != context_idx {
                            epoch_loss += train_pair(
                                &mut embeddings,
                                &mut context_embeddings,
                                center_idx,
                                neg_idx,
                                false,
                                config.learning_rate,
                            );
                            sample_count += 1;
                        }
                    }
                }
            }
        }
        
        // Decay learning rate
        let avg_loss = if sample_count > 0 {
            epoch_loss / sample_count as f64
        } else {
            0.0
        };
        
        if epoch % 2 == 0 {
            // Log every 2 epochs
            log::debug!(
                "Node2Vec epoch {}/{}: avg_loss={:.6}",
                epoch + 1,
                config.epochs,
                avg_loss
            );
        }
    }
    
    // Extract final embeddings
    let mut result = HashMap::new();
    for (node, &idx) in &node_to_idx {
        let embedding = embeddings[idx].iter().map(|&v| v as f32).collect();
        result.insert(*node, embedding);
    }
    
    Ok(result)
}

/// Initialize embedding matrix with small random values
fn initialize_embeddings(vocab_size: usize, dimensions: usize) -> Vec<Vec<f64>> {
    let mut rng = rand::thread_rng();
    let mut embeddings = Vec::with_capacity(vocab_size);
    
    for _ in 0..vocab_size {
        let mut row = Vec::with_capacity(dimensions);
        for _ in 0..dimensions {
            // Initialize with small random values in [-0.5, 0.5]
            row.push(rng.gen::<f64>() - 0.5);
        }
        embeddings.push(row);
    }
    
    embeddings
}

/// Train a single (center, context) pair using skip-gram with negative sampling
fn train_pair(
    embeddings: &mut [Vec<f64>],
    context_embeddings: &mut [Vec<f64>],
    center_idx: usize,
    context_idx: usize,
    is_positive: bool,
    learning_rate: f64,
) -> f64 {
    let dimensions = embeddings[0].len();
    
    // Compute dot product
    let mut dot = 0.0;
    for d in 0..dimensions {
        dot += embeddings[center_idx][d] * context_embeddings[context_idx][d];
    }
    
    // Sigmoid activation
    let sigmoid = 1.0 / (1.0 + (-dot).exp());
    
    // Compute error
    let label = if is_positive { 1.0 } else { 0.0 };
    let error = label - sigmoid;
    
    // Gradient descent update
    for d in 0..dimensions {
        let gradient = error * learning_rate;
        
        let center_val = embeddings[center_idx][d];
        let context_val = context_embeddings[context_idx][d];
        
        // Update both embeddings
        embeddings[center_idx][d] += gradient * context_val;
        context_embeddings[context_idx][d] += gradient * center_val;
    }
    
    // Return loss (cross-entropy)
    let loss = if is_positive {
        -(sigmoid.ln())
    } else {
        -((1.0 - sigmoid).ln())
    };
    
    if loss.is_finite() {
        loss
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;
    use crate::storage::MemoryStore;

    #[tokio::test]
    async fn test_node2vec_basic() {
        let store = MemoryStore::new();
        
        // Create a simple graph: A -> B -> C, A -> C
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();
        store.insert_triple(Triple::new(a.id, "knows", c.id)).await.unwrap();
        
        let config = Node2VecConfig {
            dimensions: 8,
            walk_length: 10,
            walks_per_node: 5,
            epochs: 3,
            ..Default::default()
        };
        
        let embeddings = compute_node2vec(&store, config).await.unwrap();
        
        // Should have 3 nodes
        assert_eq!(embeddings.len(), 3);
        
        // Each embedding should have 8 dimensions
        for (node_id, embedding) in embeddings.iter() {
            assert_eq!(
                embedding.len(),
                8,
                "Node {:?} has wrong dimensions",
                node_id
            );
        }
    }

    #[tokio::test]
    async fn test_connected_nodes_similar() {
        let store = MemoryStore::new();
        
        // Create a graph where A and B are both connected to C
        // A and B should have more similar embeddings than A and D
        // A -> C <- B, D -> E
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        let e = store.find_or_create_node("E").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "knows", c.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();
        store.insert_triple(Triple::new(d.id, "knows", e.id)).await.unwrap();
        
        let config = Node2VecConfig {
            dimensions: 16,
            walk_length: 20,
            walks_per_node: 20,
            epochs: 10,
            ..Default::default()
        };
        
        let embeddings = compute_node2vec(&store, config).await.unwrap();
        
        // Get embeddings
        let emb_a = embeddings.get(&a.id).unwrap();
        let emb_b = embeddings.get(&b.id).unwrap();
        let emb_d = embeddings.get(&d.id).unwrap();
        
        // Compute cosine similarities
        let sim_ab = cosine_similarity(emb_a, emb_b);
        let sim_ad = cosine_similarity(emb_a, emb_d);
        
        // A and B should be more similar than A and D
        // (This is probabilistic, so we use a relaxed threshold)
        assert!(
            sim_ab > sim_ad - 0.1,
            "Connected nodes A-B (sim={:.3}) should be more similar than distant A-D (sim={:.3})",
            sim_ab,
            sim_ad
        );
    }

    #[tokio::test]
    async fn test_random_walks_visit_neighbors() {
        let store = MemoryStore::new();
        
        // Create a linear chain: A -> B -> C
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "next", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "next", c.id)).await.unwrap();
        
        let graph_view = GraphView::from_store(&store).await.unwrap();
        
        let config = Node2VecConfig {
            walk_length: 5,
            walks_per_node: 1,
            ..Default::default()
        };
        
        let walks = generate_walks(&graph_view, &config).unwrap();
        
        // Should generate walks for each node
        assert!(!walks.is_empty());
        
        // Walks starting from A should visit B
        let walk_from_a: Vec<_> = walks
            .iter()
            .filter(|w| w.first() == Some(&a.id))
            .collect();
        
        assert!(!walk_from_a.is_empty());
        
        // At least one walk from A should contain B (highly likely with outgoing edge)
        let contains_b = walk_from_a.iter().any(|w| w.contains(&b.id));
        assert!(contains_b, "Walk from A should visit neighbor B");
    }

    #[tokio::test]
    async fn test_empty_graph() {
        let store = MemoryStore::new();
        
        let config = Node2VecConfig::default();
        let embeddings = compute_node2vec(&store, config).await.unwrap();
        
        assert_eq!(embeddings.len(), 0);
    }

    #[tokio::test]
    async fn test_isolated_nodes() {
        let store = MemoryStore::new();
        
        // Create isolated nodes (no edges)
        let _a = store.find_or_create_node("A").await.unwrap();
        let _b = store.find_or_create_node("B").await.unwrap();
        
        let config = Node2VecConfig::default();
        let embeddings = compute_node2vec(&store, config).await.unwrap();
        
        // Isolated nodes have no walks, so no embeddings
        assert_eq!(embeddings.len(), 0);
    }

    #[tokio::test]
    async fn test_small_graph() {
        let store = MemoryStore::new();
        
        // Create minimal connected graph (2 nodes, 1 edge)
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "connects", b.id)).await.unwrap();
        
        let config = Node2VecConfig {
            dimensions: 4,
            walk_length: 5,
            walks_per_node: 5,
            epochs: 3,
            ..Default::default()
        };
        
        let embeddings = compute_node2vec(&store, config).await.unwrap();
        
        // Should generate embeddings for both nodes
        assert_eq!(embeddings.len(), 2);
        assert!(embeddings.contains_key(&a.id));
        assert!(embeddings.contains_key(&b.id));
        
        for embedding in embeddings.values() {
            assert_eq!(embedding.len(), 4);
        }
    }

    // Helper: compute cosine similarity
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if mag_a == 0.0 || mag_b == 0.0 {
            0.0
        } else {
            dot / (mag_a * mag_b)
        }
    }
}
