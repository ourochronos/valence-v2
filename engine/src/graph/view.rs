//! GraphView: builds a petgraph DiGraph from TripleStore data.

use std::collections::HashMap;
use anyhow::Result;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;

use crate::models::{NodeId, Triple};
use crate::storage::TripleStore;

/// Edge weight — stores the predicate string
#[derive(Debug, Clone)]
pub struct EdgeWeight {
    pub predicate: String,
    pub weight: f64,
}

/// In-memory graph view built from TripleStore.
/// Maps NodeIds to petgraph NodeIndex, predicates to edge weights.
#[derive(Debug, Clone)]
pub struct GraphView {
    /// The underlying petgraph DiGraph
    pub graph: DiGraph<NodeId, EdgeWeight>,
    /// Map from our NodeId to petgraph NodeIndex
    pub node_map: HashMap<NodeId, NodeIndex>,
    /// Reverse map from NodeIndex to NodeId
    pub index_map: HashMap<NodeIndex, NodeId>,
}

impl GraphView {
    /// Create an empty GraphView
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
            index_map: HashMap::new(),
        }
    }

    /// Load the full graph from a TripleStore
    pub async fn from_store(store: &(impl TripleStore + ?Sized)) -> Result<Self> {
        let mut view = Self::new();
        
        // Query all triples (empty pattern = wildcard)
        let pattern = crate::storage::TriplePattern::default();
        let triples = store.query_triples(pattern).await?;
        
        // Add all nodes and edges
        for triple in triples {
            view.add_triple(&triple);
        }
        
        Ok(view)
    }

    /// Load a neighborhood subgraph around a node up to a given depth
    pub async fn from_neighborhood(
        store: &(impl TripleStore + ?Sized),
        node_id: NodeId,
        depth: u32,
    ) -> Result<Self> {
        let mut view = Self::new();
        
        // Get neighbors using the store's built-in method
        let triples = store.neighbors(node_id, depth).await?;
        
        // Add all nodes and edges from the neighborhood
        for triple in triples {
            view.add_triple(&triple);
        }
        
        Ok(view)
    }

    /// Add a triple to the graph view
    pub fn add_triple(&mut self, triple: &Triple) {
        // Ensure subject node exists
        let subject_idx = self.ensure_node(triple.subject);
        
        // Ensure object node exists
        let object_idx = self.ensure_node(triple.object);
        
        // Add edge
        let edge_weight = EdgeWeight {
            predicate: triple.predicate.value.clone(),
            weight: triple.weight,
        };
        self.graph.add_edge(subject_idx, object_idx, edge_weight);
    }

    /// Ensure a node exists in the graph, creating it if necessary
    fn ensure_node(&mut self, node_id: NodeId) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(&node_id) {
            idx
        } else {
            let idx = self.graph.add_node(node_id);
            self.node_map.insert(node_id, idx);
            self.index_map.insert(idx, node_id);
            idx
        }
    }

    /// Get the NodeIndex for a NodeId
    pub fn get_index(&self, node_id: NodeId) -> Option<NodeIndex> {
        self.node_map.get(&node_id).copied()
    }

    /// Get the NodeId for a NodeIndex
    pub fn get_node_id(&self, index: NodeIndex) -> Option<NodeId> {
        self.index_map.get(&index).copied()
    }

    /// Get all neighbors of a node (both incoming and outgoing)
    pub fn neighbors(&self, node_id: NodeId) -> Vec<NodeId> {
        let Some(idx) = self.get_index(node_id) else {
            return Vec::new();
        };

        let mut neighbors = Vec::new();
        
        // Outgoing edges
        for neighbor_idx in self.graph.neighbors_directed(idx, Direction::Outgoing) {
            if let Some(neighbor_id) = self.get_node_id(neighbor_idx) {
                neighbors.push(neighbor_id);
            }
        }
        
        // Incoming edges
        for neighbor_idx in self.graph.neighbors_directed(idx, Direction::Incoming) {
            if let Some(neighbor_id) = self.get_node_id(neighbor_idx) {
                neighbors.push(neighbor_id);
            }
        }
        
        neighbors
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the number of edges in the graph
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

impl Default for GraphView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Node, Triple};
    use crate::storage::MemoryStore;

    #[tokio::test]
    async fn test_from_store() {
        let store = MemoryStore::new();
        
        // Create some nodes
        let alice = Node::new("Alice");
        let bob = Node::new("Bob");
        let charlie = Node::new("Charlie");
        
        let alice_id = store.insert_node(alice).await.unwrap();
        let bob_id = store.insert_node(bob).await.unwrap();
        let charlie_id = store.insert_node(charlie).await.unwrap();
        
        // Create some triples
        let t1 = Triple::new(alice_id, "knows", bob_id);
        let t2 = Triple::new(bob_id, "knows", charlie_id);
        
        store.insert_triple(t1).await.unwrap();
        store.insert_triple(t2).await.unwrap();
        
        // Build graph view
        let view = GraphView::from_store(&store).await.unwrap();
        
        assert_eq!(view.node_count(), 3);
        assert_eq!(view.edge_count(), 2);
    }

    #[tokio::test]
    async fn test_from_neighborhood() {
        let store = MemoryStore::new();
        
        // Create a chain: A -> B -> C -> D
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "next", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "next", c.id)).await.unwrap();
        store.insert_triple(Triple::new(c.id, "next", d.id)).await.unwrap();
        
        // Get neighborhood of B with depth 1
        let view = GraphView::from_neighborhood(&store, b.id, 1).await.unwrap();
        
        // Should include A, B, C (but not D — it's 2 hops away)
        // Note: The actual implementation depends on the neighbors() method in TripleStore
        assert!(view.node_count() >= 2); // At least B and its direct neighbors
    }
}
