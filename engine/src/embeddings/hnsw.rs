//! HNSW (Hierarchical Navigable Small World) index for O(log n) approximate nearest neighbor search.
//!
//! Custom implementation optimized for Valence's needs:
//! - Incremental inserts and updates (spring model nudges embeddings continuously)
//! - Removal support (nodes can be deleted)
//! - Cosine similarity as the distance metric (matching existing brute-force search)
//! - Multiple independent indices (one per embedding strategy: spring, node2vec, spectral)
//!
//! Based on the paper: "Efficient and robust approximate nearest neighbor search using
//! Hierarchical Navigable Small World graphs" by Malkov & Yashunin (2016, 2018).

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

use rand::Rng;

use crate::models::NodeId;

/// Configuration for the HNSW index.
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// Max number of connections per node at layers > 0 (M in the paper).
    pub max_connections: usize,
    /// Max number of connections at layer 0 (M0 in the paper, typically 2*M).
    pub max_connections_layer0: usize,
    /// Size of the dynamic candidate list during construction (ef_construction).
    pub ef_construction: usize,
    /// Size of the dynamic candidate list during search (ef_search).
    /// Must be >= k for k-nearest neighbor queries.
    pub ef_search: usize,
    /// Normalization factor for level generation (1/ln(M) in the paper).
    pub level_multiplier: f64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        let m = 16;
        Self {
            max_connections: m,
            max_connections_layer0: m * 2,
            ef_construction: 200,
            ef_search: 50,
            level_multiplier: 1.0 / (m as f64).ln(),
        }
    }
}

impl HnswConfig {
    pub fn new(max_connections: usize, ef_construction: usize, ef_search: usize) -> Self {
        Self {
            max_connections,
            max_connections_layer0: max_connections * 2,
            ef_construction,
            ef_search,
            level_multiplier: 1.0 / (max_connections as f64).ln(),
        }
    }
}

/// An element stored in the HNSW graph.
#[derive(Debug, Clone)]
struct HnswNode {
    /// The node's ID in the knowledge graph.
    node_id: NodeId,
    /// The embedding vector.
    vector: Vec<f32>,
    /// Connections at each layer: layer -> set of internal indices.
    connections: Vec<Vec<usize>>,
    /// Whether this node has been marked as deleted (lazy deletion).
    deleted: bool,
}

/// Candidate during search: (negative_similarity, internal_index).
/// Using negative similarity so BinaryHeap (max-heap) gives us min-similarity first.
#[derive(Debug, Clone)]
struct Candidate {
    similarity: f32,
    index: usize,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher similarity = better = comes first in max-heap
        self.similarity
            .partial_cmp(&other.similarity)
            .unwrap_or(Ordering::Equal)
    }
}

/// A min-heap candidate (for maintaining the worst-best boundary).
#[derive(Debug, Clone)]
struct MinCandidate {
    similarity: f32,
    index: usize,
}

impl PartialEq for MinCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl Eq for MinCandidate {}

impl PartialOrd for MinCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower similarity = comes first (min-heap via reversed comparison)
        other
            .similarity
            .partial_cmp(&self.similarity)
            .unwrap_or(Ordering::Equal)
    }
}

/// HNSW index for approximate nearest neighbor search.
///
/// Supports O(log n) search, incremental inserts, updates, and lazy deletion.
pub struct HnswIndex {
    config: HnswConfig,
    /// All nodes in the index (including deleted ones until compaction).
    nodes: Vec<HnswNode>,
    /// Map from NodeId to internal index for O(1) lookup.
    id_to_index: HashMap<NodeId, usize>,
    /// The entry point (index of node at the highest layer).
    entry_point: Option<usize>,
    /// Current maximum layer in the graph.
    max_layer: usize,
}

impl HnswIndex {
    /// Create a new empty HNSW index with default configuration.
    pub fn new() -> Self {
        Self::with_config(HnswConfig::default())
    }

    /// Create a new empty HNSW index with custom configuration.
    pub fn with_config(config: HnswConfig) -> Self {
        Self {
            config,
            nodes: Vec::new(),
            id_to_index: HashMap::new(),
            entry_point: None,
            max_layer: 0,
        }
    }

    /// Insert a node into the index. If the node already exists, updates its embedding.
    pub fn insert(&mut self, node_id: NodeId, vector: Vec<f32>) {
        if let Some(&existing_idx) = self.id_to_index.get(&node_id) {
            // Update existing node's vector and reconnect
            self.update_internal(existing_idx, vector);
            return;
        }

        let level = self.random_level();
        let idx = self.nodes.len();

        // Create connections for each layer up to this node's level
        let connections = (0..=level).map(|_| Vec::new()).collect();

        self.nodes.push(HnswNode {
            node_id,
            vector,
            connections,
            deleted: false,
        });
        self.id_to_index.insert(node_id, idx);

        if self.entry_point.is_none() {
            // First node
            self.entry_point = Some(idx);
            self.max_layer = level;
            return;
        }

        let entry_point = self.entry_point.unwrap();

        // Phase 1: Traverse from top layer down to the node's level + 1
        // using greedy search to find the closest node at each layer
        let mut current_nearest = entry_point;

        if self.max_layer > level {
            for layer in (level + 1..=self.max_layer).rev() {
                current_nearest = self.greedy_search(idx, current_nearest, layer);
            }
        }

        // Phase 2: From the node's level down to layer 0,
        // find ef_construction nearest neighbors and connect
        let mut ep = vec![current_nearest];

        for layer in (0..=level.min(self.max_layer)).rev() {
            let candidates = self.search_layer(idx, &ep, self.config.ef_construction, layer);

            // Select neighbors to connect to (simple heuristic: take closest)
            let max_conn = if layer == 0 {
                self.config.max_connections_layer0
            } else {
                self.config.max_connections
            };

            let neighbors = self.select_neighbors(&candidates, max_conn);

            // Add bidirectional connections
            // Safety: we need to carefully handle the borrow checker here
            // by collecting indices first, then mutating
            let neighbor_indices: Vec<usize> = neighbors.iter().map(|c| c.index).collect();

            self.nodes[idx].connections[layer] = neighbor_indices.clone();

            for &neighbor_idx in &neighbor_indices {
                let node_layer = self.nodes[neighbor_idx].connections.len();
                if layer < node_layer {
                    self.nodes[neighbor_idx].connections[layer].push(idx);

                    // Prune if too many connections
                    let conn_count = self.nodes[neighbor_idx].connections[layer].len();
                    if conn_count > max_conn {
                        self.prune_connections(neighbor_idx, layer, max_conn);
                    }
                }
            }

            // Use the found candidates as entry points for the next layer
            ep = neighbor_indices;
            if ep.is_empty() {
                ep = vec![current_nearest];
            }
        }

        // Update entry point if this node has a higher level
        if level > self.max_layer {
            self.entry_point = Some(idx);
            self.max_layer = level;
        }
    }

    /// Search for the k nearest neighbors to a query vector.
    /// Returns Vec<(NodeId, similarity)> sorted by descending similarity.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(NodeId, f32)> {
        if self.entry_point.is_none() || k == 0 {
            return Vec::new();
        }

        let entry_point = self.entry_point.unwrap();

        // Phase 1: Greedy traverse from top to layer 1
        let mut current_nearest = entry_point;
        for layer in (1..=self.max_layer).rev() {
            current_nearest = self.greedy_search_query(query, current_nearest, layer);
        }

        // Phase 2: Search layer 0 with ef_search candidates
        let ef = self.config.ef_search.max(k);
        let candidates = self.search_layer_query(query, &[current_nearest], ef, 0);

        // Return top k results, excluding deleted nodes
        let mut results: Vec<(NodeId, f32)> = candidates
            .into_iter()
            .filter(|c| !self.nodes[c.index].deleted)
            .map(|c| (self.nodes[c.index].node_id, c.similarity))
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        results.truncate(k);
        results
    }

    /// Remove a node from the index (lazy deletion).
    /// The node's connections remain but it won't appear in search results.
    pub fn remove(&mut self, node_id: NodeId) -> bool {
        if let Some(&idx) = self.id_to_index.get(&node_id) {
            self.nodes[idx].deleted = true;
            // Don't remove from id_to_index so we can still find it for connection purposes
            true
        } else {
            false
        }
    }

    /// Update a node's embedding vector. If the node doesn't exist, inserts it.
    pub fn update(&mut self, node_id: NodeId, vector: Vec<f32>) {
        self.insert(node_id, vector);
    }

    /// Number of active (non-deleted) nodes in the index.
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| !n.deleted).count()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a node exists in the index (and is not deleted).
    pub fn contains(&self, node_id: NodeId) -> bool {
        self.id_to_index
            .get(&node_id)
            .map(|&idx| !self.nodes[idx].deleted)
            .unwrap_or(false)
    }

    // --- Internal methods ---

    /// Generate a random level for a new node.
    fn random_level(&self) -> usize {
        let mut rng = rand::thread_rng();
        let uniform: f64 = rng.gen();
        let level = (-uniform.ln() * self.config.level_multiplier).floor() as usize;
        // Cap at a reasonable maximum to prevent degenerate cases
        level.min(16)
    }

    /// Update an existing node's vector and reconnect it.
    fn update_internal(&mut self, idx: usize, vector: Vec<f32>) {
        self.nodes[idx].vector = vector;
        self.nodes[idx].deleted = false;

        // Re-establish connections at each layer
        let num_layers = self.nodes[idx].connections.len();

        if self.active_node_count() <= 1 {
            return;
        }

        let entry_point = match self.entry_point {
            Some(ep) => ep,
            None => return,
        };

        let mut current_nearest = entry_point;

        // Navigate from top to this node's top layer
        let node_max_layer = num_layers.saturating_sub(1);
        for layer in (node_max_layer + 1..=self.max_layer).rev() {
            current_nearest = self.greedy_search(idx, current_nearest, layer);
        }

        // Reconnect at each layer
        let mut ep = vec![current_nearest];
        for layer in (0..num_layers).rev() {
            let candidates = self.search_layer(idx, &ep, self.config.ef_construction, layer);

            let max_conn = if layer == 0 {
                self.config.max_connections_layer0
            } else {
                self.config.max_connections
            };

            let neighbors = self.select_neighbors(&candidates, max_conn);

            // Remove old connections from neighbors pointing to this node
            let old_connections: Vec<usize> = self.nodes[idx].connections[layer].clone();
            for &old_neighbor in &old_connections {
                if old_neighbor < self.nodes.len() {
                    let node_layer_count = self.nodes[old_neighbor].connections.len();
                    if layer < node_layer_count {
                        self.nodes[old_neighbor].connections[layer].retain(|&x| x != idx);
                    }
                }
            }

            // Set new connections
            let neighbor_indices: Vec<usize> = neighbors.iter().map(|c| c.index).collect();
            self.nodes[idx].connections[layer] = neighbor_indices.clone();

            // Add bidirectional connections
            for &neighbor_idx in &neighbor_indices {
                let node_layer_count = self.nodes[neighbor_idx].connections.len();
                if layer < node_layer_count {
                    if !self.nodes[neighbor_idx].connections[layer].contains(&idx) {
                        self.nodes[neighbor_idx].connections[layer].push(idx);
                    }
                    let conn_count = self.nodes[neighbor_idx].connections[layer].len();
                    if conn_count > max_conn {
                        self.prune_connections(neighbor_idx, layer, max_conn);
                    }
                }
            }

            ep = neighbor_indices;
            if ep.is_empty() {
                ep = vec![current_nearest];
            }
        }
    }

    /// Count active (non-deleted) nodes.
    fn active_node_count(&self) -> usize {
        self.nodes.iter().filter(|n| !n.deleted).count()
    }

    /// Greedy search at a single layer starting from entry_point, finding the
    /// closest node to the target node (by internal index).
    fn greedy_search(&self, target_idx: usize, entry_point: usize, layer: usize) -> usize {
        let target_vec = &self.nodes[target_idx].vector;
        self.greedy_search_query(target_vec, entry_point, layer)
    }

    /// Greedy search at a single layer for a query vector.
    fn greedy_search_query(&self, query: &[f32], entry_point: usize, layer: usize) -> usize {
        let mut current = entry_point;
        let mut current_sim = cosine_similarity(query, &self.nodes[current].vector);

        loop {
            let mut changed = false;

            if layer < self.nodes[current].connections.len() {
                for &neighbor in &self.nodes[current].connections[layer] {
                    if neighbor >= self.nodes.len() {
                        continue;
                    }
                    let sim = cosine_similarity(query, &self.nodes[neighbor].vector);
                    if sim > current_sim {
                        current = neighbor;
                        current_sim = sim;
                        changed = true;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        current
    }

    /// Search a single layer for the ef nearest neighbors of a target node.
    fn search_layer(
        &self,
        target_idx: usize,
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Vec<Candidate> {
        let target_vec = &self.nodes[target_idx].vector;
        self.search_layer_query(target_vec, entry_points, ef, layer)
    }

    /// Search a single layer for the ef nearest neighbors of a query vector.
    fn search_layer_query(
        &self,
        query: &[f32],
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Vec<Candidate> {
        let mut visited = HashSet::new();
        // Max-heap of candidates to explore (best first)
        let mut candidates = BinaryHeap::new();
        // Min-heap of results (worst first, for pruning)
        let mut results = BinaryHeap::<MinCandidate>::new();

        for &ep in entry_points {
            if ep >= self.nodes.len() {
                continue;
            }
            visited.insert(ep);
            let sim = cosine_similarity(query, &self.nodes[ep].vector);
            candidates.push(Candidate {
                similarity: sim,
                index: ep,
            });
            results.push(MinCandidate {
                similarity: sim,
                index: ep,
            });
        }

        while let Some(current) = candidates.pop() {
            // Get the worst result's similarity
            let worst_sim = results.peek().map(|r| r.similarity).unwrap_or(f32::NEG_INFINITY);

            // If the best candidate is worse than our worst result and we have enough, stop
            if current.similarity < worst_sim && results.len() >= ef {
                break;
            }

            // Explore neighbors
            let node_idx = current.index;
            if layer < self.nodes[node_idx].connections.len() {
                for &neighbor in &self.nodes[node_idx].connections[layer] {
                    if neighbor >= self.nodes.len() || !visited.insert(neighbor) {
                        continue;
                    }

                    let sim = cosine_similarity(query, &self.nodes[neighbor].vector);

                    let worst_sim =
                        results.peek().map(|r| r.similarity).unwrap_or(f32::NEG_INFINITY);

                    if sim > worst_sim || results.len() < ef {
                        candidates.push(Candidate {
                            similarity: sim,
                            index: neighbor,
                        });
                        results.push(MinCandidate {
                            similarity: sim,
                            index: neighbor,
                        });

                        if results.len() > ef {
                            results.pop(); // Remove worst
                        }
                    }
                }
            }
        }

        // Convert results to Vec<Candidate>
        results
            .into_sorted_vec()
            .into_iter()
            .map(|mc| Candidate {
                similarity: mc.similarity,
                index: mc.index,
            })
            .collect()
    }

    /// Select neighbors from candidates (simple strategy: take the closest ones).
    fn select_neighbors(&self, candidates: &[Candidate], max_neighbors: usize) -> Vec<Candidate> {
        let mut sorted = candidates.to_vec();
        sorted.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(Ordering::Equal));
        sorted.truncate(max_neighbors);
        sorted
    }

    /// Prune connections for a node at a given layer to max_connections.
    fn prune_connections(&mut self, node_idx: usize, layer: usize, max_connections: usize) {
        let node_vec = self.nodes[node_idx].vector.clone();
        let connections = self.nodes[node_idx].connections[layer].clone();

        // Score all connections by similarity to this node
        let mut scored: Vec<(usize, f32)> = connections
            .iter()
            .filter(|&&idx| idx < self.nodes.len())
            .map(|&idx| {
                let sim = cosine_similarity(&node_vec, &self.nodes[idx].vector);
                (idx, sim)
            })
            .collect();

        // Keep the most similar ones
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        scored.truncate(max_connections);

        self.nodes[node_idx].connections[layer] = scored.into_iter().map(|(idx, _)| idx).collect();
    }
}

impl Default for HnswIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for HnswIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HnswIndex")
            .field("node_count", &self.len())
            .field("total_nodes", &self.nodes.len())
            .field("max_layer", &self.max_layer)
            .finish()
    }
}

/// Compute cosine similarity between two vectors.
/// Returns value in range [-1, 1] where 1 = identical, -1 = opposite.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut mag_a = 0.0f32;
    let mut mag_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }

    let mag_a = mag_a.sqrt();
    let mag_b = mag_b.sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn random_vector(dim: usize) -> Vec<f32> {
        let mut rng = rand::thread_rng();
        (0..dim).map(|_| rng.gen::<f32>() * 2.0 - 1.0).collect()
    }

    fn brute_force_nearest(
        embeddings: &[(NodeId, Vec<f32>)],
        query: &[f32],
        k: usize,
    ) -> Vec<(NodeId, f32)> {
        let mut results: Vec<(NodeId, f32)> = embeddings
            .iter()
            .map(|(id, vec)| (*id, cosine_similarity(query, vec)))
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }

    #[test]
    fn test_insert_and_search_basic() {
        let mut index = HnswIndex::new();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        index.insert(id1, vec![1.0, 0.0, 0.0]);
        index.insert(id2, vec![0.9, 0.1, 0.0]);
        index.insert(id3, vec![0.0, 0.0, 1.0]);

        let results = index.search(&[1.0, 0.0, 0.0], 3);

        assert_eq!(results.len(), 3);
        // First result should be id1 (identical vector)
        assert_eq!(results[0].0, id1);
        assert!((results[0].1 - 1.0).abs() < 0.001);
        // Second should be id2 (very similar)
        assert_eq!(results[1].0, id2);
        assert!(results[1].1 > 0.9);
    }

    #[test]
    fn test_search_empty_index() {
        let index = HnswIndex::new();
        let results = index.search(&[1.0, 0.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_k_zero() {
        let mut index = HnswIndex::new();
        index.insert(Uuid::new_v4(), vec![1.0, 0.0, 0.0]);
        let results = index.search(&[1.0, 0.0, 0.0], 0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_remove() {
        let mut index = HnswIndex::new();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        index.insert(id1, vec![1.0, 0.0, 0.0]);
        index.insert(id2, vec![0.0, 1.0, 0.0]);

        assert_eq!(index.len(), 2);
        assert!(index.contains(id1));

        // Remove id1
        assert!(index.remove(id1));
        assert_eq!(index.len(), 1);
        assert!(!index.contains(id1));

        // Search should not return removed node
        let results = index.search(&[1.0, 0.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, id2);
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut index = HnswIndex::new();
        assert!(!index.remove(Uuid::new_v4()));
    }

    #[test]
    fn test_update_embedding() {
        let mut index = HnswIndex::new();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // Insert with initial embeddings
        index.insert(id1, vec![1.0, 0.0, 0.0]);
        index.insert(id2, vec![0.0, 1.0, 0.0]);
        index.insert(id3, vec![0.0, 0.0, 1.0]);

        // id1 is closest to [1,0,0]
        let results = index.search(&[1.0, 0.0, 0.0], 1);
        assert_eq!(results[0].0, id1);

        // Update id1's embedding to be near id3
        index.update(id1, vec![0.0, 0.0, 0.95]);

        // Now id3 should be closest to [0,0,1]
        let results = index.search(&[0.0, 0.0, 1.0], 2);
        // Both id1 and id3 should be near [0,0,1] now
        let result_ids: Vec<NodeId> = results.iter().map(|r| r.0).collect();
        assert!(result_ids.contains(&id1));
        assert!(result_ids.contains(&id3));
    }

    #[test]
    fn test_correctness_vs_brute_force() {
        let dim = 32;
        let n = 200;
        let k = 10;

        let mut index = HnswIndex::new();
        let mut embeddings = Vec::new();

        for _ in 0..n {
            let id = Uuid::new_v4();
            let vec = random_vector(dim);
            index.insert(id, vec.clone());
            embeddings.push((id, vec));
        }

        // Run multiple queries and check recall
        let num_queries = 20;
        let mut total_recall = 0.0;

        for _ in 0..num_queries {
            let query = random_vector(dim);

            let hnsw_results = index.search(&query, k);
            let brute_results = brute_force_nearest(&embeddings, &query, k);

            // Calculate recall: how many of brute-force top-k are in HNSW results?
            let hnsw_ids: HashSet<NodeId> = hnsw_results.iter().map(|r| r.0).collect();
            let brute_ids: HashSet<NodeId> = brute_results.iter().map(|r| r.0).collect();

            let recall = hnsw_ids.intersection(&brute_ids).count() as f64 / k as f64;
            total_recall += recall;
        }

        let avg_recall = total_recall / num_queries as f64;
        // HNSW should have high recall (>= 0.8 typically)
        assert!(
            avg_recall >= 0.7,
            "Average recall {:.3} is too low (expected >= 0.7)",
            avg_recall
        );
    }

    #[test]
    fn test_incremental_update_changes_results() {
        let dim = 16;
        let mut index = HnswIndex::new();

        let target_id = Uuid::new_v4();
        let other_ids: Vec<NodeId> = (0..20).map(|_| Uuid::new_v4()).collect();

        // Insert target far from the query point
        index.insert(target_id, vec![0.0; dim]);

        // Insert other nodes with random embeddings
        for &id in &other_ids {
            index.insert(id, random_vector(dim));
        }

        // Query near [1,0,...,0]
        let query: Vec<f32> = std::iter::once(1.0)
            .chain(std::iter::repeat(0.0).take(dim - 1))
            .collect();

        let results_before = index.search(&query, 5);
        let before_ids: Vec<NodeId> = results_before.iter().map(|r| r.0).collect();

        // Target should NOT be in top-5 (it's at the origin)
        assert!(
            !before_ids.contains(&target_id),
            "Target should not be in results before update"
        );

        // Update target to be very close to the query
        let mut new_vec = vec![0.0; dim];
        new_vec[0] = 0.99;
        index.update(target_id, new_vec);

        let results_after = index.search(&query, 5);
        let after_ids: Vec<NodeId> = results_after.iter().map(|r| r.0).collect();

        // Target SHOULD now be in top-5
        assert!(
            after_ids.contains(&target_id),
            "Target should be in results after update"
        );
    }

    #[test]
    fn test_scale_10k_nodes() {
        let dim = 128;
        let n = 10_000;
        let k = 10;

        // Use smaller parameters for faster test
        let config = HnswConfig::new(12, 100, 50);
        let mut index = HnswIndex::with_config(config);

        // Insert 10K nodes
        for _ in 0..n {
            let id = Uuid::new_v4();
            let vec = random_vector(dim);
            index.insert(id, vec);
        }

        assert_eq!(index.len(), n);

        // Measure search time (should be sub-ms)
        let query = random_vector(dim);
        let start = std::time::Instant::now();
        let results = index.search(&query, k);
        let elapsed = start.elapsed();

        assert_eq!(results.len(), k);
        assert!(
            elapsed.as_millis() < 50,
            "Search took {}ms, expected < 50ms",
            elapsed.as_millis()
        );

        // Verify results are sorted by similarity descending
        for i in 1..results.len() {
            assert!(
                results[i - 1].1 >= results[i].1,
                "Results not sorted: {} < {}",
                results[i - 1].1,
                results[i].1
            );
        }
    }

    #[test]
    fn test_single_node() {
        let mut index = HnswIndex::new();
        let id = Uuid::new_v4();
        index.insert(id, vec![1.0, 0.0, 0.0]);

        let results = index.search(&[1.0, 0.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, id);
        assert!((results[0].1 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_two_nodes() {
        let mut index = HnswIndex::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        index.insert(id1, vec![1.0, 0.0]);
        index.insert(id2, vec![0.0, 1.0]);

        let results = index.search(&[0.7, 0.3], 2);
        assert_eq!(results.len(), 2);
        // id1 should be more similar to [0.7, 0.3]
        assert_eq!(results[0].0, id1);
    }

    #[test]
    fn test_k_greater_than_index_size() {
        let mut index = HnswIndex::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        index.insert(id1, vec![1.0, 0.0, 0.0]);
        index.insert(id2, vec![0.0, 1.0, 0.0]);

        let results = index.search(&[1.0, 0.0, 0.0], 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_multiple_independent_indices() {
        // Verify that separate HNSW indices work independently
        let mut spring_index = HnswIndex::new();
        let mut spectral_index = HnswIndex::new();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Same nodes, different embeddings in each index
        spring_index.insert(id1, vec![1.0, 0.0, 0.0]);
        spring_index.insert(id2, vec![0.0, 1.0, 0.0]);

        spectral_index.insert(id1, vec![0.0, 1.0, 0.0]);
        spectral_index.insert(id2, vec![1.0, 0.0, 0.0]);

        let query = vec![1.0, 0.0, 0.0];

        // Spring index: id1 is closest
        let spring_results = spring_index.search(&query, 1);
        assert_eq!(spring_results[0].0, id1);

        // Spectral index: id2 is closest (reversed embeddings)
        let spectral_results = spectral_index.search(&query, 1);
        assert_eq!(spectral_results[0].0, id2);
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut index = HnswIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);

        let id = Uuid::new_v4();
        index.insert(id, vec![1.0, 0.0]);
        assert!(!index.is_empty());
        assert_eq!(index.len(), 1);

        index.remove(id);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_contains() {
        let mut index = HnswIndex::new();
        let id = Uuid::new_v4();

        assert!(!index.contains(id));

        index.insert(id, vec![1.0]);
        assert!(index.contains(id));

        index.remove(id);
        assert!(!index.contains(id));
    }

    #[test]
    fn test_duplicate_insert_updates() {
        let mut index = HnswIndex::new();
        let id = Uuid::new_v4();

        index.insert(id, vec![1.0, 0.0, 0.0]);
        assert_eq!(index.len(), 1);

        // Re-insert same ID with different vector — should update, not add
        index.insert(id, vec![0.0, 1.0, 0.0]);
        assert_eq!(index.len(), 1);

        // Search should reflect the updated vector
        let results = index.search(&[0.0, 1.0, 0.0], 1);
        assert_eq!(results[0].0, id);
        assert!((results[0].1 - 1.0).abs() < 0.001);
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

        // Different lengths
        let sim = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]);
        assert_eq!(sim, 0.0);
    }
}
