//! WorkingSet: the active subgraph for a query or conversation.
//!
//! A WorkingSet represents the conceptual threads currently in play. It's built
//! from a query by finding semantically similar nodes via embeddings, then expanding
//! via graph neighbors to form a coherent local view of relevant knowledge.

use std::collections::{HashSet, HashMap};
use anyhow::{Result, Context as AnyhowContext};
use serde::{Serialize, Deserialize};

use crate::{
    engine::ValenceEngine,
    embeddings::EmbeddingStore,
    models::{NodeId, Triple, TripleId},
    storage::TripleStore,
};

/// A working set is the active subgraph for a query or conversation.
///
/// It contains:
/// - The set of active node IDs
/// - The triples connecting them
/// - Confidence scores for each triple
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingSet {
    /// Active nodes in this working set
    pub nodes: HashSet<NodeId>,
    /// Triples in this working set with their confidence scores
    pub triples: HashMap<TripleId, (Triple, f64)>,
}

impl WorkingSet {
    /// Create an empty working set
    pub fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            triples: HashMap::new(),
        }
    }

    /// Add a node to the working set
    pub fn add_node(&mut self, node_id: NodeId) {
        self.nodes.insert(node_id);
    }

    /// Add a triple to the working set with its confidence score
    pub fn add_triple(&mut self, triple: Triple, confidence: f64) {
        self.triples.insert(triple.id, (triple, confidence));
    }

    /// Check if a node is in the working set
    pub fn contains_node(&self, node_id: NodeId) -> bool {
        self.nodes.contains(&node_id)
    }

    /// Check if a triple is in the working set
    pub fn contains_triple(&self, triple_id: TripleId) -> bool {
        self.triples.contains_key(&triple_id)
    }

    /// Get the number of nodes in the working set
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of triples in the working set
    pub fn triple_count(&self) -> usize {
        self.triples.len()
    }

    /// Build a working set from a query string.
    ///
    /// Process (with graceful degradation):
    /// 1. Find query node by value
    /// 2. Try embedding similarity search first
    /// 3. Fall back to graph traversal if embeddings don't exist
    /// 4. Expand via graph neighbors (1-2 hops from each result)
    /// 5. Include confidence scores for each triple
    ///
    /// # Arguments
    ///
    /// * `engine` - The ValenceEngine to query
    /// * `query` - The query string (matched against node values)
    /// * `k` - Number of nearest neighbors to find
    ///
    /// # Returns
    ///
    /// A WorkingSet containing the relevant subgraph
    pub async fn from_query(engine: &ValenceEngine, query: &str, k: usize) -> Result<Self> {
        let query_node = engine
            .store
            .find_node_by_value(query)
            .await?
            .context("Query node not found")?;

        // Try embedding search first
        let embeddings_store = engine.embeddings.read().await;
        let query_embedding = embeddings_store.get(query_node.id);

        if let Some(embedding) = query_embedding {
            // Warm mode: use embedding similarity
            let neighbors = embeddings_store
                .query_nearest(embedding, k)
                .context("Failed to query nearest neighbors")?;
            
            drop(embeddings_store); // Release lock before async operations

            let mut working_set = WorkingSet::new();
            working_set.add_node(query_node.id);

            // Step 3: Add neighbor nodes and expand via graph
            for (node_id, _similarity) in neighbors {
                working_set.add_node(node_id);

                // Expand 1-2 hops from this node
                let first_hop = engine
                    .store
                    .neighbors(node_id, 1)
                    .await
                    .context("Failed to get first-hop neighbors")?;

                for triple in first_hop {
                    working_set.add_node(triple.subject);
                    working_set.add_node(triple.object);
                    working_set.add_triple(triple.clone(), triple.weight);
                }

                // Second hop: neighbors of neighbors (but limit expansion)
                if working_set.node_count() < k * 3 {
                    let second_hop = engine
                        .store
                        .neighbors(node_id, 2)
                        .await
                        .context("Failed to get second-hop neighbors")?;

                    for triple in second_hop {
                        if working_set.node_count() >= k * 5 {
                            break;
                        }

                        working_set.add_node(triple.subject);
                        working_set.add_node(triple.object);

                        // Second hop triples get lower confidence (decay)
                        let decayed_confidence = triple.weight * 0.5;
                        working_set.add_triple(triple.clone(), decayed_confidence);
                    }
                }
            }

            Ok(working_set)
        } else {
            // Cold mode: fall back to graph-only traversal
            drop(embeddings_store);
            
            tracing::warn!(
                "No embedding found for query node '{}', falling back to graph-based traversal",
                query
            );
            
            Self::from_query_graph_only(engine, query, 2).await
        }
    }

    /// Build a working set using pure graph traversal (no embeddings).
    ///
    /// This is the cold engine fallback — works without any embeddings by
    /// traversing the graph starting from the query node.
    ///
    /// # Arguments
    ///
    /// * `engine` - The ValenceEngine to query
    /// * `query` - The query string (matched against node values)
    /// * `depth` - Maximum graph traversal depth
    ///
    /// # Returns
    ///
    /// A WorkingSet containing the graph neighborhood
    pub async fn from_query_graph_only(
        engine: &ValenceEngine,
        query: &str,
        depth: u32,
    ) -> Result<Self> {
        let mut working_set = WorkingSet::new();

        // Find query node
        let query_node = engine
            .store
            .find_node_by_value(query)
            .await?
            .context("Query node not found")?;

        working_set.add_node(query_node.id);

        // Graph traversal from query node
        let triples = engine
            .store
            .neighbors(query_node.id, depth)
            .await
            .context("Failed to traverse graph neighbors")?;

        for triple in triples {
            working_set.add_node(triple.subject);
            working_set.add_node(triple.object);
            working_set.add_triple(triple.clone(), triple.weight);
        }

        Ok(working_set)
    }

    /// Serialize the working set to JSON for inspection
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize working set to JSON")
    }
}

impl Default for WorkingSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;

    #[test]
    fn test_empty_working_set() {
        let ws = WorkingSet::new();
        assert_eq!(ws.node_count(), 0);
        assert_eq!(ws.triple_count(), 0);
    }

    #[test]
    fn test_add_node() {
        let mut ws = WorkingSet::new();
        let node_id = uuid::Uuid::new_v4();
        
        ws.add_node(node_id);
        assert_eq!(ws.node_count(), 1);
        assert!(ws.contains_node(node_id));
    }

    #[test]
    fn test_add_triple() {
        let mut ws = WorkingSet::new();
        let s = uuid::Uuid::new_v4();
        let o = uuid::Uuid::new_v4();
        let triple = Triple::new(s, "knows", o);
        
        ws.add_triple(triple.clone(), 0.85);
        assert_eq!(ws.triple_count(), 1);
        assert!(ws.contains_triple(triple.id));
        
        let (stored_triple, confidence) = ws.triples.get(&triple.id).unwrap();
        assert_eq!(stored_triple.id, triple.id);
        assert_eq!(*confidence, 0.85);
    }

    #[test]
    fn test_serialization() {
        let mut ws = WorkingSet::new();
        let node_id = uuid::Uuid::new_v4();
        ws.add_node(node_id);
        
        let json = ws.to_json().unwrap();
        assert!(json.contains("nodes"));
        assert!(json.contains("triples"));
    }

    #[tokio::test]
    async fn test_from_query_basic() {
        let engine = ValenceEngine::new();

        // Build a small graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "likes", carol.id)).await.unwrap();

        // Recompute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        // Build working set from query
        let ws = WorkingSet::from_query(&engine, "Alice", 2).await.unwrap();

        // Should have found nodes
        assert!(ws.node_count() > 0);
        assert!(ws.triple_count() > 0);

        // Alice should be in the working set
        assert!(ws.contains_node(alice.id));
    }

    #[tokio::test]
    async fn test_from_query_expansion() {
        let engine = ValenceEngine::new();

        // Create a chain: A -> B -> C -> D
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        let c = engine.store.find_or_create_node("C").await.unwrap();
        let d = engine.store.find_or_create_node("D").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "next", b.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(b.id, "next", c.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(c.id, "next", d.id)).await.unwrap();

        // Recompute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        // Build working set from query
        let ws = WorkingSet::from_query(&engine, "A", 2).await.unwrap();

        // Should include multiple nodes due to expansion
        assert!(ws.node_count() >= 2);
        
        // Should include A
        assert!(ws.contains_node(a.id));
    }

    #[tokio::test]
    async fn test_working_set_empty_graph() {
        let engine = ValenceEngine::new();

        // Create a single node with no connections
        let _lonely = engine.store.find_or_create_node("lonely").await.unwrap();

        // Don't compute embeddings — should fall back to graph traversal
        // Build working set — should succeed with graph-only mode
        let result = WorkingSet::from_query(&engine, "lonely", 5).await;
        assert!(result.is_ok());
        
        let ws = result.unwrap();
        // Should have at least the query node
        assert_eq!(ws.node_count(), 1);
    }

    #[tokio::test]
    async fn test_from_query_graph_only() {
        let engine = ValenceEngine::new();

        // Build a small graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();

        // Use graph-only mode (no embeddings needed)
        let ws = WorkingSet::from_query_graph_only(&engine, "Alice", 2).await.unwrap();

        // Should have found nodes via graph traversal
        assert!(ws.node_count() > 0);
        assert!(ws.contains_node(alice.id));
        assert!(ws.triple_count() > 0);
    }
}
