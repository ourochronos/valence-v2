//! Spectral embeddings from graph Laplacian.
//!
//! Pure linear algebra approach:
//! 1. Build adjacency matrix A from the graph
//! 2. Compute degree matrix D (diagonal matrix of node degrees)
//! 3. Compute Laplacian L = D - A
//! 4. Find k smallest eigenvectors of L
//! 5. Each node's embedding = its row in the eigenvector matrix
//!
//! Properties:
//! - Deterministic (same graph -> same embeddings)
//! - No parameters to learn
//! - Captures global graph structure via spectral properties
//! - Nodes with similar structural roles cluster together

use std::collections::HashMap;
use anyhow::{Result, Context};
use faer::prelude::*;
use petgraph::visit::EdgeRef;

use crate::models::NodeId;
use crate::storage::TripleStore;
use crate::graph::GraphView;

/// Configuration for spectral embedding computation
#[derive(Debug, Clone)]
pub struct SpectralConfig {
    /// Number of dimensions for the embedding (default: 64)
    pub dimensions: usize,
    /// Whether to normalize the Laplacian (default: true)
    /// Normalized Laplacian: L_norm = D^(-1/2) * L * D^(-1/2)
    pub normalize: bool,
}

impl Default for SpectralConfig {
    fn default() -> Self {
        Self {
            dimensions: 64,
            normalize: true,
        }
    }
}

impl SpectralConfig {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            normalize: true,
        }
    }
}

/// Compute spectral embeddings from any TripleStore
pub async fn compute_embeddings(
    store: &(impl TripleStore + ?Sized),
    dimensions: usize,
) -> Result<HashMap<NodeId, Vec<f32>>> {
    let config = SpectralConfig::new(dimensions);
    compute_embeddings_with_config(store, config).await
}

/// Compute spectral embeddings with custom configuration
pub async fn compute_embeddings_with_config(
    store: &(impl TripleStore + ?Sized),
    config: SpectralConfig,
) -> Result<HashMap<NodeId, Vec<f32>>> {
    // Build graph view from store
    let graph_view = GraphView::from_store(store)
        .await
        .context("Failed to build graph view")?;
    
    let node_count = graph_view.node_count();
    
    if node_count == 0 {
        return Ok(HashMap::new());
    }
    
    // Limit dimensions to node_count - 1 (eigenvector constraint)
    let dimensions = config.dimensions.min(node_count.saturating_sub(1));
    
    if dimensions == 0 {
        return Ok(HashMap::new());
    }
    
    // Build adjacency matrix and degree vector
    let (adjacency, degree, node_order) = build_adjacency_matrix(&graph_view);
    
    // Compute Laplacian
    let laplacian = if config.normalize {
        compute_normalized_laplacian(&adjacency, &degree)
    } else {
        compute_laplacian(&adjacency, &degree)
    };
    
    // Compute eigenvectors (smallest eigenvalues)
    let embeddings_matrix = compute_eigenvectors(&laplacian, dimensions)
        .context("Failed to compute eigenvectors")?;
    
    // Extract embeddings for each node
    let mut embeddings = HashMap::new();
    for (i, &node_id) in node_order.iter().enumerate() {
        let mut embedding = Vec::with_capacity(dimensions);
        for j in 0..dimensions {
            embedding.push(embeddings_matrix[(i, j)] as f32);
        }
        embeddings.insert(node_id, embedding);
    }
    
    Ok(embeddings)
}

/// Build adjacency matrix from GraphView
/// Returns (adjacency_matrix, degree_vector, node_order)
fn build_adjacency_matrix(
    graph_view: &GraphView,
) -> (Mat<f64>, Vec<f64>, Vec<NodeId>) {
    let n = graph_view.node_count();
    
    // Create node ordering (deterministic by sorting NodeIds)
    let mut node_order: Vec<NodeId> = graph_view.node_map.keys().copied().collect();
    node_order.sort();
    
    // Create index map: NodeId -> matrix index
    let node_to_idx: HashMap<NodeId, usize> = node_order
        .iter()
        .enumerate()
        .map(|(i, &node_id)| (node_id, i))
        .collect();
    
    // Build adjacency matrix
    let mut adjacency = Mat::zeros(n, n);
    let mut degree = vec![0.0; n];
    
    // Iterate over edges in the petgraph
    for edge_ref in graph_view.graph.edge_references() {
        let source_node_id = graph_view.get_node_id(edge_ref.source());
        let target_node_id = graph_view.get_node_id(edge_ref.target());
        
        if let (Some(src_id), Some(tgt_id)) = (source_node_id, target_node_id) {
            if let (Some(&i), Some(&j)) = (node_to_idx.get(&src_id), node_to_idx.get(&tgt_id)) {
                let weight = edge_ref.weight().weight;
                
                // Add edge (treat as undirected for Laplacian)
                adjacency[(i, j)] = weight;
                adjacency[(j, i)] = weight;
                
                // Update degrees
                degree[i] += weight;
                degree[j] += weight;
            }
        }
    }
    
    (adjacency, degree, node_order)
}

/// Compute unnormalized Laplacian: L = D - A
fn compute_laplacian(adjacency: &Mat<f64>, degree: &[f64]) -> Mat<f64> {
    let n = adjacency.nrows();
    let mut laplacian = adjacency.clone();
    
    // L = -A (negate adjacency)
    for i in 0..n {
        for j in 0..n {
            laplacian[(i, j)] = -laplacian[(i, j)];
        }
    }
    
    // Add degree matrix to diagonal: L = D - A
    for i in 0..n {
        laplacian[(i, i)] += degree[i];
    }
    
    laplacian
}

/// Compute normalized Laplacian: L_norm = D^(-1/2) * L * D^(-1/2)
fn compute_normalized_laplacian(adjacency: &Mat<f64>, degree: &[f64]) -> Mat<f64> {
    let n = adjacency.nrows();
    
    // Compute D^(-1/2)
    let mut d_inv_sqrt = vec![0.0; n];
    for i in 0..n {
        d_inv_sqrt[i] = if degree[i] > 0.0 {
            1.0 / degree[i].sqrt()
        } else {
            0.0
        };
    }
    
    // Compute normalized Laplacian
    let mut laplacian = Mat::zeros(n, n);
    
    for i in 0..n {
        for j in 0..n {
            if i == j {
                // Diagonal: 1 for connected nodes, 0 for isolated
                laplacian[(i, i)] = if degree[i] > 0.0 { 1.0 } else { 0.0 };
            } else {
                // Off-diagonal: -A[i,j] / sqrt(d[i] * d[j])
                laplacian[(i, j)] = -adjacency[(i, j)] * d_inv_sqrt[i] * d_inv_sqrt[j];
            }
        }
    }
    
    laplacian
}

/// Compute k smallest eigenvectors of the Laplacian
fn compute_eigenvectors(laplacian: &Mat<f64>, k: usize) -> Result<Mat<f64>> {
    let n = laplacian.nrows();
    
    // Compute eigendecomposition
    // For real symmetric matrices, use symmetric eigendecomposition
    let eigendecomp = laplacian.selfadjoint_eigendecomposition(faer::Side::Lower);
    
    let eigenvalues = eigendecomp.s().column_vector();
    let eigenvectors = eigendecomp.u();
    
    // Sort eigenvalues and get indices of k smallest
    let mut eigen_pairs: Vec<(usize, f64)> = eigenvalues
        .iter()
        .enumerate()
        .map(|(i, &val)| (i, val))
        .collect();
    
    // Sort by eigenvalue (ascending)
    eigen_pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Take k smallest (skip the first one if it's ~0, which is the constant eigenvector)
    let start_idx = if eigen_pairs[0].1.abs() < 1e-10 { 1 } else { 0 };
    let selected_indices: Vec<usize> = eigen_pairs
        .iter()
        .skip(start_idx)
        .take(k)
        .map(|(i, _)| *i)
        .collect();
    
    // Extract selected eigenvectors
    let mut result = Mat::zeros(n, k);
    for (col, &eigen_idx) in selected_indices.iter().enumerate() {
        for row in 0..n {
            result[(row, col)] = eigenvectors[(row, eigen_idx)];
        }
    }
    
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;
    use crate::storage::{MemoryStore, TripleStore};

    #[tokio::test]
    async fn test_spectral_embeddings_dimensionality() {
        let store = MemoryStore::new();
        
        // Create a small graph: A -> B -> C, A -> C
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();
        store.insert_triple(Triple::new(a.id, "knows", c.id)).await.unwrap();
        
        // Compute embeddings with dimension 2
        let embeddings = compute_embeddings(&store, 2).await.unwrap();
        
        // Should have 3 nodes
        assert_eq!(embeddings.len(), 3);
        
        // Each embedding should have 2 dimensions
        for (node_id, embedding) in embeddings.iter() {
            assert_eq!(embedding.len(), 2, "Node {:?} has wrong dimensions", node_id);
        }
    }

    #[tokio::test]
    async fn test_nearby_nodes_similar_embeddings() {
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
        
        // Compute embeddings
        let embeddings = compute_embeddings(&store, 4).await.unwrap();
        
        // Get embeddings
        let emb_a = embeddings.get(&a.id).unwrap();
        let emb_b = embeddings.get(&b.id).unwrap();
        let emb_d = embeddings.get(&d.id).unwrap();
        
        // Compute cosine similarities
        let sim_ab = cosine_similarity(emb_a, emb_b);
        let sim_ad = cosine_similarity(emb_a, emb_d);
        
        // A and B should be more similar than A and D
        assert!(
            sim_ab > sim_ad,
            "Connected nodes A-B (sim={:.3}) should be more similar than distant A-D (sim={:.3})",
            sim_ab, sim_ad
        );
    }

    #[tokio::test]
    async fn test_empty_graph() {
        let store = MemoryStore::new();
        let embeddings = compute_embeddings(&store, 64).await.unwrap();
        assert_eq!(embeddings.len(), 0);
    }

    #[tokio::test]
    async fn test_single_node() {
        let store = MemoryStore::new();
        let _a = store.find_or_create_node("A").await.unwrap();
        
        // Single isolated node has no edges, can't compute meaningful embeddings
        let embeddings = compute_embeddings(&store, 64).await.unwrap();
        assert_eq!(embeddings.len(), 0);
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

    // === EDGE CASE TESTS ===

    #[tokio::test]
    async fn test_spectral_embeddings_fewer_nodes_than_dimensions() {
        let store = MemoryStore::new();
        
        // Create only 2 nodes with one edge
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "connects", b.id)).await.unwrap();
        
        // Request 64 dimensions but only have 2 nodes
        // Should return min(dimensions, node_count - 1) = min(64, 1) = 1
        let embeddings = compute_embeddings(&store, 64).await.unwrap();
        
        // Should have 2 nodes
        assert_eq!(embeddings.len(), 2);
        
        // Each embedding should have 1 dimension (node_count - 1)
        for embedding in embeddings.values() {
            assert_eq!(embedding.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_spectral_embeddings_disconnected_graph() {
        let store = MemoryStore::new();
        
        // Create two disconnected components
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        
        // Component 1: A <-> B
        store.insert_triple(Triple::new(a.id, "connects", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "connects", a.id)).await.unwrap();
        
        // Component 2: C <-> D
        store.insert_triple(Triple::new(c.id, "connects", d.id)).await.unwrap();
        store.insert_triple(Triple::new(d.id, "connects", c.id)).await.unwrap();
        
        // Compute embeddings
        let embeddings = compute_embeddings(&store, 3).await.unwrap();
        
        // Should have 4 nodes
        assert_eq!(embeddings.len(), 4);
        
        // Each embedding should have 3 dimensions
        for embedding in embeddings.values() {
            assert_eq!(embedding.len(), 3);
        }
        
        // Nodes in the same component should be more similar
        let emb_a = embeddings.get(&a.id).unwrap();
        let emb_b = embeddings.get(&b.id).unwrap();
        let emb_c = embeddings.get(&c.id).unwrap();
        let emb_d = embeddings.get(&d.id).unwrap();
        
        let sim_ab = cosine_similarity(emb_a, emb_b);
        let sim_ac = cosine_similarity(emb_a, emb_c);
        let sim_cd = cosine_similarity(emb_c, emb_d);
        
        // A-B should be similar (same component)
        // C-D should be similar (same component)
        // A-C should be less similar (different components)
        assert!(sim_ab.abs() > 0.1 || sim_cd.abs() > 0.1, 
            "Within-component similarity should be non-zero");
        
        // Check that cross-component similarity is lower
        assert!(sim_ac.abs() < sim_ab.abs().max(sim_cd.abs()), 
            "Cross-component similarity should be lower than within-component");
    }

    #[tokio::test]
    async fn test_spectral_embeddings_three_nodes() {
        let store = MemoryStore::new();
        
        // Create minimal graph with 3 nodes (edge case for eigenvector computation)
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "connects", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "connects", c.id)).await.unwrap();
        
        // Request 2 dimensions (should work with 3 nodes: max is node_count - 1 = 2)
        let embeddings = compute_embeddings(&store, 2).await.unwrap();
        
        assert_eq!(embeddings.len(), 3);
        for embedding in embeddings.values() {
            assert_eq!(embedding.len(), 2);
        }
    }
}
