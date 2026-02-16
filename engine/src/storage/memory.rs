use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use anyhow::Result;
use async_trait::async_trait;

use crate::models::{Triple, TripleId, Node, NodeId, Source, SourceId};
use super::traits::{TripleStore, TriplePattern};

/// In-memory implementation of TripleStore using HashMaps.
///
/// Fast, simple, but ephemeral - data is lost on restart. Uses RwLocks for concurrent access.
/// Suitable for testing, prototyping, or small-scale deployments.
#[derive(Clone)]
pub struct MemoryStore {
    nodes: Arc<RwLock<HashMap<NodeId, Node>>>,
    triples: Arc<RwLock<HashMap<TripleId, Triple>>>,
    sources: Arc<RwLock<HashMap<SourceId, Source>>>,
    /// Index: node value -> node ID for quick lookups
    node_value_index: Arc<RwLock<HashMap<String, NodeId>>>,
    /// Index: triple ID -> source IDs
    triple_sources: Arc<RwLock<HashMap<TripleId, Vec<SourceId>>>>,
}

impl MemoryStore {
    /// Create a new empty MemoryStore.
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            triples: Arc::new(RwLock::new(HashMap::new())),
            sources: Arc::new(RwLock::new(HashMap::new())),
            node_value_index: Arc::new(RwLock::new(HashMap::new())),
            triple_sources: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TripleStore for MemoryStore {
    async fn insert_node(&self, node: Node) -> Result<NodeId> {
        let id = node.id;
        let value = node.value.clone();
        
        self.nodes.write().unwrap().insert(id, node);
        self.node_value_index.write().unwrap().insert(value, id);
        
        Ok(id)
    }

    async fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        Ok(self.nodes.read().unwrap().get(&id).cloned())
    }

    async fn find_node_by_value(&self, value: &str) -> Result<Option<Node>> {
        let node_id = self.node_value_index.read().unwrap().get(value).copied();
        if let Some(id) = node_id {
            Ok(self.nodes.read().unwrap().get(&id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn find_or_create_node(&self, value: &str) -> Result<Node> {
        if let Some(node) = self.find_node_by_value(value).await? {
            Ok(node)
        } else {
            let node = Node::new(value.to_string());
            self.insert_node(node.clone()).await?;
            Ok(node)
        }
    }

    async fn insert_triple(&self, triple: Triple) -> Result<TripleId> {
        let id = triple.id;
        self.triples.write().unwrap().insert(id, triple);
        Ok(id)
    }

    async fn get_triple(&self, id: TripleId) -> Result<Option<Triple>> {
        Ok(self.triples.read().unwrap().get(&id).cloned())
    }

    async fn query_triples(&self, pattern: TriplePattern) -> Result<Vec<Triple>> {
        let triples = self.triples.read().unwrap();
        let results: Vec<Triple> = triples
            .values()
            .filter(|t| {
                if let Some(ref subj) = pattern.subject {
                    if &t.subject != subj {
                        return false;
                    }
                }
                if let Some(ref pred) = pattern.predicate {
                    if &t.predicate.value != pred {
                        return false;
                    }
                }
                if let Some(ref obj) = pattern.object {
                    if &t.object != obj {
                        return false;
                    }
                }
                true
                })
                .cloned()
                .collect();
        Ok(results)
    }

    async fn touch_triple(&self, id: TripleId) -> Result<()> {
        let mut triples = self.triples.write().unwrap();
        if let Some(triple) = triples.get_mut(&id) {
            triple.touch();
        }
        Ok(())
    }

    async fn delete_triple(&self, id: TripleId) -> Result<()> {
        self.triples.write().unwrap().remove(&id);
        // Also clean up source mappings
        self.triple_sources.write().unwrap().remove(&id);
        Ok(())
    }

    async fn insert_source(&self, source: Source) -> Result<SourceId> {
        let id = source.id;
        let triple_ids = source.triple_ids.clone();
        
        self.sources.write().unwrap().insert(id, source);
        
        // Update reverse index: triple -> sources
        let mut triple_sources = self.triple_sources.write().unwrap();
        for triple_id in triple_ids {
            triple_sources
                .entry(triple_id)
                .or_insert_with(Vec::new)
                .push(id);
        }
        
        Ok(id)
    }

    async fn get_sources_for_triple(&self, triple_id: TripleId) -> Result<Vec<Source>> {
        let triple_sources = self.triple_sources.read().unwrap();
        let source_ids = triple_sources.get(&triple_id).cloned().unwrap_or_default();
        
        let sources_map = self.sources.read().unwrap();
        let sources: Vec<Source> = source_ids
            .iter()
            .filter_map(|id| sources_map.get(id).cloned())
            .collect();
        
        Ok(sources)
    }

    async fn neighbors(&self, node_id: NodeId, depth: u32) -> Result<Vec<Triple>> {
        if depth == 0 {
            return Ok(Vec::new());
        }

        let triples = self.triples.read().unwrap();
        
        // Find all triples where node_id is subject or object
        let mut result: Vec<Triple> = triples
            .values()
            .filter(|t| t.subject == node_id || t.object == node_id)
            .cloned()
            .collect();
        
        // For depth > 1, recursively find neighbors
        if depth > 1 {
            let mut seen = std::collections::HashSet::new();
            seen.insert(node_id);
            
            let mut current_level = result.clone();
            for _ in 1..depth {
                let mut next_level = Vec::new();
                for triple in &current_level {
                    // Get connected nodes
                    let connected = [triple.subject, triple.object];
                    for &conn_node in &connected {
                        if seen.insert(conn_node) {
                            let neighbors: Vec<Triple> = triples
                                .values()
                                .filter(|t| {
                                    (t.subject == conn_node || t.object == conn_node) &&
                                    !result.iter().any(|r| r.id == t.id)
                                })
                                .cloned()
                                .collect();
                            next_level.extend(neighbors.clone());
                            result.extend(neighbors);
                        }
                    }
                }
                current_level = next_level;
            }
        }
        
        Ok(result)
    }

    async fn count_triples(&self) -> Result<u64> {
        Ok(self.triples.read().unwrap().len() as u64)
    }

    async fn count_nodes(&self) -> Result<u64> {
        Ok(self.nodes.read().unwrap().len() as u64)
    }

    async fn decay(&self, factor: f64, min_weight: f64) -> Result<u64> {
        let mut triples = self.triples.write().unwrap();
        let mut decayed_count = 0u64;
        
        for triple in triples.values_mut() {
            triple.weight *= factor;
            if triple.weight < min_weight {
                triple.weight = min_weight;
            }
            decayed_count += 1;
        }
        
        Ok(decayed_count)
    }

    async fn evict_below_weight(&self, threshold: f64) -> Result<u64> {
        let mut triples = self.triples.write().unwrap();
        let initial_count = triples.len();
        
        triples.retain(|_, triple| triple.weight >= threshold);
        
        let evicted = initial_count - triples.len();
        Ok(evicted as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Node, Triple, Source, SourceType};

    #[tokio::test]
    async fn test_insert_and_retrieve_node() {
        let store = MemoryStore::new();
        let node = Node::new("test_value");
        let id = store.insert_node(node.clone()).await.unwrap();
        
        let retrieved = store.get_node(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, "test_value");
    }

    #[tokio::test]
    async fn test_find_node_by_value() {
        let store = MemoryStore::new();
        let node = Node::new("unique_value");
        store.insert_node(node.clone()).await.unwrap();
        
        let found = store.find_node_by_value("unique_value").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().value, "unique_value");
        
        let not_found = store.find_node_by_value("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_find_or_create_node() {
        let store = MemoryStore::new();
        
        let node1 = store.find_or_create_node("test").await.unwrap();
        let node2 = store.find_or_create_node("test").await.unwrap();
        
        assert_eq!(node1.id, node2.id);
        assert_eq!(store.count_nodes().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_insert_and_query_triples() {
        let store = MemoryStore::new();
        
        let subj = Node::new("Alice");
        let obj = Node::new("Bob");
        let subj_id = store.insert_node(subj).await.unwrap();
        let obj_id = store.insert_node(obj).await.unwrap();
        
        let triple = Triple::new(subj_id, "knows", obj_id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        // Query by subject
        let pattern = TriplePattern {
            subject: Some(subj_id),
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, triple_id);
        
        // Query by predicate
        let pattern = TriplePattern {
            subject: None,
            predicate: Some("knows".to_string()),
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
        
        // Query by object
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: Some(obj_id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_touch_triple() {
        let store = MemoryStore::new();
        
        let subj = Node::new("A");
        let obj = Node::new("B");
        let subj_id = store.insert_node(subj).await.unwrap();
        let obj_id = store.insert_node(obj).await.unwrap();
        
        let triple = Triple::new(subj_id, "rel", obj_id);
        let triple_id = triple.id;
        store.insert_triple(triple).await.unwrap();
        
        let before = store.get_triple(triple_id).await.unwrap().unwrap();
        let access_count_before = before.access_count;
        
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        store.touch_triple(triple_id).await.unwrap();
        
        let after = store.get_triple(triple_id).await.unwrap().unwrap();
        assert_eq!(after.access_count, access_count_before + 1);
        assert!(after.last_accessed > before.last_accessed);
        assert_eq!(after.weight, 1.0);
    }

    #[tokio::test]
    async fn test_neighbors() {
        let store = MemoryStore::new();
        
        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let carol = store.find_or_create_node("Carol").await.unwrap();
        
        let t1 = Triple::new(alice.id, "knows", bob.id);
        let t2 = Triple::new(bob.id, "knows", carol.id);
        
        store.insert_triple(t1.clone()).await.unwrap();
        store.insert_triple(t2.clone()).await.unwrap();
        
        // Depth 1: Alice knows Bob
        let neighbors = store.neighbors(alice.id, 1).await.unwrap();
        assert_eq!(neighbors.len(), 1);
        
        // Depth 2: Alice -> Bob -> Carol
        let neighbors = store.neighbors(alice.id, 2).await.unwrap();
        assert_eq!(neighbors.len(), 2);
    }

    #[tokio::test]
    async fn test_decay_and_eviction() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        store.insert_triple(triple.clone()).await.unwrap();
        
        // Initial weight should be 1.0
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 1.0);
        
        // Decay by 0.5
        store.decay(0.5, 0.0).await.unwrap();
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 0.5);
        
        // Decay again
        store.decay(0.5, 0.0).await.unwrap();
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 0.25);
        
        // Evict below 0.3
        let evicted = store.evict_below_weight(0.3).await.unwrap();
        assert_eq!(evicted, 1);
        assert_eq!(store.count_triples().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_source_tracking() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        let source = Source::new(vec![triple_id], SourceType::UserInput)
            .with_reference("user-123");
        
        store.insert_source(source.clone()).await.unwrap();
        
        let sources = store.get_sources_for_triple(triple_id).await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_type, SourceType::UserInput);
        assert_eq!(sources[0].reference.as_deref(), Some("user-123"));
    }

    // === EDGE CASE TESTS ===

    #[tokio::test]
    async fn test_query_all_wildcard() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "rel1", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "rel2", c.id)).await.unwrap();
        store.insert_triple(Triple::new(c.id, "rel3", a.id)).await.unwrap();
        
        // Query with all wildcards should return all triples
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_insert_duplicate_triple() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        // Insert same triple twice (different IDs since Triple::new generates new UUIDs)
        let triple1 = Triple::new(a.id, "knows", b.id);
        let triple2 = Triple::new(a.id, "knows", b.id);
        
        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();
        
        // Both should be stored (different IDs)
        assert_ne!(id1, id2);
        assert_eq!(store.count_triples().await.unwrap(), 2);
        
        // Query should return both
        let pattern = TriplePattern {
            subject: Some(a.id),
            predicate: Some("knows".to_string()),
            object: Some(b.id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_triple() {
        let store = MemoryStore::new();
        
        // Try to delete a triple that doesn't exist (should not error)
        let fake_id = uuid::Uuid::new_v4();
        let result = store.delete_triple(fake_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_nonexistent_node() {
        let store = MemoryStore::new();
        
        // Try to get a node that doesn't exist
        let fake_id = uuid::Uuid::new_v4();
        let result = store.get_node(fake_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_decay_with_factor_greater_than_one() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        store.insert_triple(triple.clone()).await.unwrap();
        
        // Decay with factor > 1.0 (weights will increase, which might not be desired)
        store.decay(1.5, 0.0).await.unwrap();
        
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 1.5);
    }

    #[tokio::test]
    async fn test_evict_with_threshold_zero() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        store.insert_triple(triple).await.unwrap();
        
        // Decay to very small weight
        store.decay(0.001, 0.0).await.unwrap();
        
        // Evict with threshold 0.0 should evict nothing (weights are >= 0.0)
        let evicted = store.evict_below_weight(0.0).await.unwrap();
        assert_eq!(evicted, 0);
        assert_eq!(store.count_triples().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_evict_with_threshold_above_one() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        store.insert_triple(triple).await.unwrap();
        
        // Evict with threshold > 1.0 should evict everything (initial weight is 1.0)
        let evicted = store.evict_below_weight(1.1).await.unwrap();
        assert_eq!(evicted, 1);
        assert_eq!(store.count_triples().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_large_batch_insert() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        
        // Insert 1000+ triples
        let mut triple_ids = Vec::new();
        for i in 0..1500 {
            let node = store.find_or_create_node(&format!("Node_{}", i)).await.unwrap();
            let triple = Triple::new(a.id, "connects_to", node.id);
            let id = store.insert_triple(triple).await.unwrap();
            triple_ids.push(id);
        }
        
        // Verify count
        assert_eq!(store.count_triples().await.unwrap(), 1500);
        
        // Verify we can query them
        let pattern = TriplePattern {
            subject: Some(a.id),
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1500);
        
        // Verify we can retrieve each triple
        for id in triple_ids.iter().take(10) {
            let triple = store.get_triple(*id).await.unwrap();
            assert!(triple.is_some());
        }
    }

    #[tokio::test]
    async fn test_neighbors_depth_zero() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        
        // Depth 0 should return empty
        let neighbors = store.neighbors(a.id, 0).await.unwrap();
        assert_eq!(neighbors.len(), 0);
    }

    #[tokio::test]
    async fn test_find_or_create_node_idempotency() {
        let store = MemoryStore::new();
        
        // Call find_or_create multiple times with same value
        let node1 = store.find_or_create_node("test_value").await.unwrap();
        let node2 = store.find_or_create_node("test_value").await.unwrap();
        let node3 = store.find_or_create_node("test_value").await.unwrap();
        
        // All should return the same node ID
        assert_eq!(node1.id, node2.id);
        assert_eq!(node2.id, node3.id);
        
        // Should only have created one node
        assert_eq!(store.count_nodes().await.unwrap(), 1);
    }
}
