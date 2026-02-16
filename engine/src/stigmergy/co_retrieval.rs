//! Use co-access patterns to create/strengthen edges in the graph.
//!
//! The CoRetrievalEngine implements the core stigmergy mechanism: when triples are
//! frequently accessed together, create structural edges connecting them. This makes
//! the graph topology reflect actual usage patterns.

use std::sync::Arc;
use anyhow::{Result, Context};

use crate::{
    models::{Triple, TripleId},
    storage::TripleStore,
};

use super::AccessTracker;

/// Configuration for the CoRetrievalEngine.
#[derive(Debug, Clone)]
pub struct CoRetrievalConfig {
    /// Minimum co-access count before creating an edge
    pub threshold: u64,
    /// Predicate used for co-retrieval edges
    pub predicate: String,
}

impl Default for CoRetrievalConfig {
    fn default() -> Self {
        Self {
            threshold: 3,
            predicate: "co_retrieved_with".to_string(),
        }
    }
}

/// Use co-access patterns to create/strengthen edges between frequently co-accessed triples.
///
/// The CoRetrievalEngine is the mechanism by which usage patterns reshape the graph structure.
/// It reads co-access data from the AccessTracker and creates edges between the subjects/objects
/// of frequently co-accessed triples.
pub struct CoRetrievalEngine {
    /// The triple store to write edges to
    store: Arc<dyn TripleStore>,
    /// The access tracker to read patterns from
    tracker: Arc<AccessTracker>,
    /// Configuration
    config: CoRetrievalConfig,
}

impl CoRetrievalEngine {
    /// Create a new CoRetrievalEngine with default configuration.
    pub fn new(store: Arc<dyn TripleStore>, tracker: Arc<AccessTracker>) -> Self {
        Self::with_config(store, tracker, CoRetrievalConfig::default())
    }

    /// Create a new CoRetrievalEngine with custom configuration.
    pub fn with_config(
        store: Arc<dyn TripleStore>,
        tracker: Arc<AccessTracker>,
        config: CoRetrievalConfig,
    ) -> Self {
        Self {
            store,
            tracker,
            config,
        }
    }

    /// Run the reinforcement cycle: create edges between frequently co-accessed triples.
    ///
    /// This is the core stigmergy operation. For each pair of triples that have been
    /// co-accessed more than the threshold, create edges connecting their subjects/objects.
    ///
    /// Returns the number of new edges created.
    pub async fn reinforce(&self) -> Result<u64> {
        // Get all pairs above threshold
        let pairs = self.tracker
            .get_pairs_above_threshold(self.config.threshold)
            .await;

        if pairs.is_empty() {
            return Ok(0);
        }

        let mut created = 0;

        // For each high-frequency pair, create structural edges
        for ((triple_a_id, triple_b_id), _count) in pairs {
            created += self.create_co_retrieval_edges(triple_a_id, triple_b_id)
                .await
                .context("Failed to create co-retrieval edges")?;
        }

        Ok(created)
    }

    /// Create edges between the subjects/objects of two co-accessed triples.
    ///
    /// For triples A and B that are frequently accessed together:
    /// - Create edge: subject(A) --co_retrieved_with--> subject(B)
    /// - Create edge: object(A) --co_retrieved_with--> object(B)
    ///
    /// These edges make the graph structurally reflect the co-access pattern.
    async fn create_co_retrieval_edges(
        &self,
        triple_a_id: TripleId,
        triple_b_id: TripleId,
    ) -> Result<u64> {
        // Fetch both triples
        let triple_a = self.store
            .get_triple(triple_a_id)
            .await
            .context("Failed to fetch triple A")?
            .context("Triple A not found")?;

        let triple_b = self.store
            .get_triple(triple_b_id)
            .await
            .context("Failed to fetch triple B")?
            .context("Triple B not found")?;

        let mut created = 0;

        // Create edge between subjects (if different)
        if triple_a.subject != triple_b.subject {
            if !self.edge_exists(triple_a.subject, triple_b.subject).await? {
                let edge = Triple::new(
                    triple_a.subject,
                    &self.config.predicate,
                    triple_b.subject,
                );
                self.store.insert_triple(edge).await?;
                created += 1;
            }
        }

        // Create edge between objects (if different)
        if triple_a.object != triple_b.object {
            if !self.edge_exists(triple_a.object, triple_b.object).await? {
                let edge = Triple::new(
                    triple_a.object,
                    &self.config.predicate,
                    triple_b.object,
                );
                self.store.insert_triple(edge).await?;
                created += 1;
            }
        }

        Ok(created)
    }

    /// Check if a co-retrieval edge already exists between two nodes.
    async fn edge_exists(&self, from: uuid::Uuid, to: uuid::Uuid) -> Result<bool> {
        use crate::storage::TriplePattern;

        // Check both directions (co-retrieval is bidirectional)
        let pattern_forward = TriplePattern {
            subject: Some(from),
            predicate: Some(self.config.predicate.clone()),
            object: Some(to),
        };

        let pattern_reverse = TriplePattern {
            subject: Some(to),
            predicate: Some(self.config.predicate.clone()),
            object: Some(from),
        };

        let forward_results = self.store.query_triples(pattern_forward).await?;
        let reverse_results = self.store.query_triples(pattern_reverse).await?;

        Ok(!forward_results.is_empty() || !reverse_results.is_empty())
    }

    /// Run a full maintenance cycle: reinforce based on current patterns, then apply decay.
    ///
    /// This is the recommended way to run the stigmergy loop regularly:
    /// 1. Create new edges based on current frequent patterns
    /// 2. Then decay old access events from the tracker
    ///
    /// Returns (edges_created, events_decayed).
    pub async fn run_maintenance_cycle(&self) -> Result<(u64, usize)> {
        // First, reinforce based on current patterns
        let created = self.reinforce().await?;

        // Then decay old access events
        let decayed = self.tracker.apply_decay().await;

        Ok((created, decayed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStore;
    use crate::stigmergy::AccessTrackerConfig;

    #[tokio::test]
    async fn test_reinforce_creates_edges() {
        let store = Arc::new(MemoryStore::new());
        let tracker = Arc::new(AccessTracker::new());

        // Create some triples
        let node_a = store.find_or_create_node("Alice").await.unwrap();
        let node_b = store.find_or_create_node("Bob").await.unwrap();
        let node_c = store.find_or_create_node("Charlie").await.unwrap();
        let node_d = store.find_or_create_node("David").await.unwrap();

        let triple1 = Triple::new(node_a.id, "knows", node_b.id);
        let triple2 = Triple::new(node_c.id, "knows", node_d.id);

        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();

        // Record co-accesses (threshold is 3 by default)
        for _ in 0..5 {
            tracker.record_access(&[id1, id2], "query").await;
        }

        // Create engine and run reinforcement
        let engine = CoRetrievalEngine::new(store.clone(), tracker);
        let created = engine.reinforce().await.unwrap();

        // Should create 2 edges: Alice--Bob and Charlie--David
        assert_eq!(created, 2);

        // Verify edges exist
        let total_triples = store.count_triples().await.unwrap();
        assert_eq!(total_triples, 4); // Original 2 + 2 co-retrieval edges
    }

    #[tokio::test]
    async fn test_threshold_respected() {
        let store = Arc::new(MemoryStore::new());
        let tracker = Arc::new(AccessTracker::new());

        // Create triples
        let node_a = store.find_or_create_node("Alice").await.unwrap();
        let node_b = store.find_or_create_node("Bob").await.unwrap();
        let node_c = store.find_or_create_node("Charlie").await.unwrap();
        let node_d = store.find_or_create_node("David").await.unwrap();

        let triple1 = Triple::new(node_a.id, "knows", node_b.id);
        let triple2 = Triple::new(node_c.id, "knows", node_d.id);

        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();

        // Record only 2 co-accesses (below threshold of 3)
        tracker.record_access(&[id1, id2], "query1").await;
        tracker.record_access(&[id1, id2], "query2").await;

        // Create engine and run reinforcement
        let engine = CoRetrievalEngine::new(store.clone(), tracker);
        let created = engine.reinforce().await.unwrap();

        // Should not create any edges
        assert_eq!(created, 0);

        // Still only original triples
        let total_triples = store.count_triples().await.unwrap();
        assert_eq!(total_triples, 2);
    }

    #[tokio::test]
    async fn test_no_duplicate_edges() {
        let store = Arc::new(MemoryStore::new());
        let tracker = Arc::new(AccessTracker::new());

        // Create triples
        let node_a = store.find_or_create_node("Alice").await.unwrap();
        let node_b = store.find_or_create_node("Bob").await.unwrap();
        let node_c = store.find_or_create_node("Charlie").await.unwrap();
        let node_d = store.find_or_create_node("David").await.unwrap();

        let triple1 = Triple::new(node_a.id, "knows", node_b.id);
        let triple2 = Triple::new(node_c.id, "knows", node_d.id);

        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();

        // Record co-accesses above threshold
        for _ in 0..5 {
            tracker.record_access(&[id1, id2], "query").await;
        }

        // Create engine and run reinforcement twice
        let engine = CoRetrievalEngine::new(store.clone(), tracker);
        let created1 = engine.reinforce().await.unwrap();
        let created2 = engine.reinforce().await.unwrap();

        // First run creates edges, second run creates nothing (duplicates prevented)
        assert_eq!(created1, 2);
        assert_eq!(created2, 0);

        let total_triples = store.count_triples().await.unwrap();
        assert_eq!(total_triples, 4); // Original 2 + 2 co-retrieval edges
    }

    #[tokio::test]
    async fn test_same_node_no_self_edge() {
        let store = Arc::new(MemoryStore::new());
        let tracker = Arc::new(AccessTracker::new());

        // Create two triples with the same subject
        let node_a = store.find_or_create_node("Alice").await.unwrap();
        let node_b = store.find_or_create_node("Bob").await.unwrap();
        let node_c = store.find_or_create_node("Charlie").await.unwrap();

        let triple1 = Triple::new(node_a.id, "knows", node_b.id);
        let triple2 = Triple::new(node_a.id, "likes", node_c.id); // Same subject

        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();

        // Record co-accesses
        for _ in 0..5 {
            tracker.record_access(&[id1, id2], "query").await;
        }

        // Create engine and run reinforcement
        let engine = CoRetrievalEngine::new(store.clone(), tracker);
        let created = engine.reinforce().await.unwrap();

        // Should create 1 edge: Bob--Charlie (not Alice--Alice)
        assert_eq!(created, 1);
    }

    #[tokio::test]
    async fn test_maintenance_cycle() {
        let store = Arc::new(MemoryStore::new());
        
        // Use short decay window for testing
        let tracker_config = AccessTrackerConfig {
            window_size: 10_000,
            decay_hours: 0, // Immediate decay for testing
        };
        let tracker = Arc::new(AccessTracker::with_config(tracker_config));

        // Create triples
        let node_a = store.find_or_create_node("Alice").await.unwrap();
        let node_b = store.find_or_create_node("Bob").await.unwrap();
        let node_c = store.find_or_create_node("Charlie").await.unwrap();
        let node_d = store.find_or_create_node("David").await.unwrap();

        let triple1 = Triple::new(node_a.id, "knows", node_b.id);
        let triple2 = Triple::new(node_c.id, "knows", node_d.id);

        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();

        // Record co-accesses
        for _ in 0..5 {
            tracker.record_access(&[id1, id2], "query").await;
        }

        let engine = CoRetrievalEngine::new(store.clone(), tracker);

        // Run maintenance cycle
        let (created, decayed) = engine.run_maintenance_cycle().await.unwrap();

        // Edges should be created first (before decay)
        assert_eq!(created, 2);
        // Then with 0-hour decay window, all events should be decayed
        assert_eq!(decayed, 5);
    }

    #[tokio::test]
    async fn test_custom_config() {
        let store = Arc::new(MemoryStore::new());
        let tracker = Arc::new(AccessTracker::new());

        // Custom config with higher threshold and different predicate
        let config = CoRetrievalConfig {
            threshold: 10,
            predicate: "strongly_related".to_string(),
        };

        let node_a = store.find_or_create_node("Alice").await.unwrap();
        let node_b = store.find_or_create_node("Bob").await.unwrap();
        let node_c = store.find_or_create_node("Charlie").await.unwrap();
        let node_d = store.find_or_create_node("David").await.unwrap();

        let triple1 = Triple::new(node_a.id, "knows", node_b.id);
        let triple2 = Triple::new(node_c.id, "knows", node_d.id);

        let id1 = store.insert_triple(triple1).await.unwrap();
        let id2 = store.insert_triple(triple2).await.unwrap();

        // Record 12 co-accesses (above new threshold of 10)
        for _ in 0..12 {
            tracker.record_access(&[id1, id2], "query").await;
        }

        let engine = CoRetrievalEngine::with_config(store.clone(), tracker, config.clone());
        let created = engine.reinforce().await.unwrap();

        // Should create edges with custom predicate
        assert_eq!(created, 2);

        // Verify custom predicate used - check for either direction
        use crate::storage::TriplePattern;
        let pattern = TriplePattern {
            subject: None,
            predicate: Some(config.predicate.clone()),
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        // Should have 2 edges with the custom predicate
        assert_eq!(results.len(), 2);
        
        // Check that the predicate is correct
        assert!(results.iter().all(|t| t.predicate.value == "strongly_related"));
    }
}
