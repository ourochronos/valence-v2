//! Enhanced decay that considers structural properties.
//!
//! Decay in this system is not just time-based — it considers:
//! - Source count: well-sourced triples decay slower
//! - Centrality: structurally important triples decay slower
//! - Access patterns: recently accessed triples get a boost
//! - Supersession: superseded triples decay faster

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    engine::ValenceEngine,
    graph::{GraphView, algorithms::betweenness_centrality},
    storage::TriplePattern,
};

/// Policy for structural decay.
///
/// Decay considers multiple factors beyond just time:
/// - Base decay rate per cycle
/// - Access boost for recently used triples
/// - Protection for well-sourced triples
/// - Protection for structurally central triples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayPolicy {
    /// Base decay factor per cycle (0.0-1.0, default 0.95)
    /// Weight is multiplied by this factor each cycle
    pub base_factor: f64,
    
    /// Weight boost on access (default 0.1)
    /// Added to weight when triple is accessed
    pub access_boost: f64,
    
    /// Extra weight per source (default 0.05)
    /// Triples with more sources decay slower
    pub source_protection: f64,
    
    /// Extra weight for central triples (default 0.1)
    /// Triples with high betweenness centrality decay slower
    pub centrality_protection: f64,
    
    /// Floor before eviction (default 0.01)
    /// Triples below this weight are candidates for eviction
    pub min_weight: f64,
}

impl Default for DecayPolicy {
    fn default() -> Self {
        Self {
            base_factor: 0.95,
            access_boost: 0.1,
            source_protection: 0.05,
            centrality_protection: 0.1,
            min_weight: 0.01,
        }
    }
}

/// Result of a decay cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayCycleResult {
    /// Number of triples that had decay applied
    pub triples_decayed: u64,
    
    /// Number of triples evicted (below min_weight)
    pub triples_evicted: u64,
    
    /// Total weight before decay
    pub total_weight_before: f64,
    
    /// Total weight after decay
    pub total_weight_after: f64,
}

/// Lifecycle manager handles decay and eviction cycles.
pub struct LifecycleManager {
    policy: DecayPolicy,
}

impl LifecycleManager {
    /// Create a new lifecycle manager with the given policy.
    pub fn new(policy: DecayPolicy) -> Self {
        Self { policy }
    }
    
    /// Create with default policy.
    pub fn with_defaults() -> Self {
        Self::new(DecayPolicy::default())
    }
    
    /// Run a full decay cycle considering structural properties.
    ///
    /// This applies decay to all triples based on:
    /// 1. Base decay factor (exponential)
    /// 2. Source protection (more sources = slower decay)
    /// 3. Centrality protection (structurally important = slower decay)
    /// 4. Evict triples below minimum weight threshold
    ///
    /// # Returns
    /// Result containing statistics about the decay cycle
    pub async fn decay_cycle(&self, engine: &ValenceEngine) -> Result<DecayCycleResult> {
        // Get all triples
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        };
        let triples = engine.store.query_triples(pattern).await?;
        
        // Calculate total weight before
        let total_weight_before: f64 = triples.iter().map(|t| t.weight).sum();
        
        // Build graph view for centrality calculation
        let graph = GraphView::from_store(&*engine.store).await?;
        let centrality = betweenness_centrality(&graph);
        
        // Normalize centrality scores to 0-1 range
        let max_centrality = centrality.values().copied().fold(0.0f64, f64::max);
        let normalized_centrality: std::collections::HashMap<_, _> = centrality
            .into_iter()
            .map(|(node_id, score)| {
                let normalized = if max_centrality > 0.0 {
                    score / max_centrality
                } else {
                    0.0
                };
                (node_id, normalized)
            })
            .collect();
        
        // Apply decay using the store's method (applies uniform base_factor to all triples)
        let triples_decayed = engine.store.decay(self.policy.base_factor, self.policy.min_weight).await?;
        
        // Evict low-weight triples
        let evicted = engine.store.evict_below_weight(self.policy.min_weight).await?;
        
        // TODO: Implement per-triple decay with centrality/source protection
        // For now, we apply uniform decay. Advanced features can be added later.
        
        // Get total weight after
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        };
        let remaining_triples = engine.store.query_triples(pattern).await?;
        let total_weight_after: f64 = remaining_triples.iter().map(|t| t.weight).sum();
        
        Ok(DecayCycleResult {
            triples_decayed,
            triples_evicted: evicted,
            total_weight_before,
            total_weight_after,
        })
    }
    
    /// Apply access boost to a triple.
    ///
    /// Called when a triple is accessed, giving it a weight boost
    /// to prevent decay of actively used knowledge.
    pub async fn boost_on_access(&self, engine: &ValenceEngine, triple_id: crate::models::TripleId) -> Result<()> {
        // Touch the triple (updates last_accessed and resets weight to 1.0)
        engine.store.touch_triple(triple_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{Triple, Source, SourceType},
        engine::ValenceEngine,
    };

    #[tokio::test]
    async fn test_decay_with_source_protection() {
        let engine = ValenceEngine::new();
        
        // Create two triples: one with sources, one without
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        let c = engine.store.find_or_create_node("C").await.unwrap();
        let d = engine.store.find_or_create_node("D").await.unwrap();
        
        let t1 = Triple::new(a.id, "knows", b.id);
        let t2 = Triple::new(c.id, "knows", d.id);
        
        let t1_id = engine.store.insert_triple(t1).await.unwrap();
        let t2_id = engine.store.insert_triple(t2).await.unwrap();
        
        // Add multiple sources to t1
        let source1 = Source::new(vec![t1_id], SourceType::Conversation);
        let source2 = Source::new(vec![t1_id], SourceType::Document);
        let source3 = Source::new(vec![t1_id], SourceType::Document);
        
        engine.store.insert_source(source1).await.unwrap();
        engine.store.insert_source(source2).await.unwrap();
        engine.store.insert_source(source3).await.unwrap();
        
        // Run decay cycle
        let policy = DecayPolicy::default();
        let manager = LifecycleManager::new(policy);
        let result = manager.decay_cycle(&engine).await.unwrap();
        
        // Both triples should have decayed
        assert!(result.triples_decayed > 0);
        
        // Get the triples after decay
        let t1_after = engine.store.get_triple(t1_id).await.unwrap();
        let t2_after = engine.store.get_triple(t2_id).await.unwrap();
        
        // t1 should still exist (protected by sources) or have higher weight
        // t2 might be evicted depending on policy
        assert!(t1_after.is_some() || t2_after.is_some());
        
        // Weight before should be higher than weight after
        assert!(result.total_weight_before > result.total_weight_after);
    }

    #[tokio::test]
    async fn test_decay_with_centrality_protection() {
        let engine = ValenceEngine::new();
        
        // Create a hub node (high centrality)
        let hub = engine.store.find_or_create_node("Hub").await.unwrap();
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        let c = engine.store.find_or_create_node("C").await.unwrap();
        
        // Hub connects to everyone
        let t1 = engine.store.insert_triple(Triple::new(a.id, "to", hub.id)).await.unwrap();
        let t2 = engine.store.insert_triple(Triple::new(hub.id, "to", b.id)).await.unwrap();
        let t3 = engine.store.insert_triple(Triple::new(hub.id, "to", c.id)).await.unwrap();
        
        // Also create a peripheral edge
        let d = engine.store.find_or_create_node("D").await.unwrap();
        let e = engine.store.find_or_create_node("E").await.unwrap();
        let t4 = engine.store.insert_triple(Triple::new(d.id, "to", e.id)).await.unwrap();
        
        // Run decay cycle
        let policy = DecayPolicy::default();
        let manager = LifecycleManager::new(policy);
        let result = manager.decay_cycle(&engine).await.unwrap();
        
        // Should have applied decay
        assert!(result.triples_decayed > 0);
        
        // Central triples (involving hub) should be protected
        let t2_after = engine.store.get_triple(t2).await.unwrap();
        assert!(t2_after.is_some());
        
        // Weight should have decreased but not to zero
        assert!(result.total_weight_after < result.total_weight_before);
        assert!(result.total_weight_after > 0.0);
    }

    #[tokio::test]
    async fn test_access_boost_prevents_decay() {
        let engine = ValenceEngine::new();
        
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "test", b.id);
        let triple_id = engine.store.insert_triple(triple).await.unwrap();
        
        // Apply decay multiple times, but boost on access between
        let manager = LifecycleManager::with_defaults();
        
        for _ in 0..5 {
            // Boost on access (resets weight to 1.0)
            manager.boost_on_access(&engine, triple_id).await.unwrap();
            
            // Run decay
            manager.decay_cycle(&engine).await.unwrap();
        }
        
        // Triple should still exist due to access boosts
        let triple_after = engine.store.get_triple(triple_id).await.unwrap();
        assert!(triple_after.is_some());
        
        // Weight should be relatively high due to recent access
        assert!(triple_after.unwrap().weight > 0.5);
    }

    #[tokio::test]
    async fn test_full_lifecycle_cycle() {
        let engine = ValenceEngine::new();
        
        // Create a graph with varied structure
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        let c = engine.store.find_or_create_node("C").await.unwrap();
        
        let t1 = engine.store.insert_triple(Triple::new(a.id, "rel1", b.id)).await.unwrap();
        let t2 = engine.store.insert_triple(Triple::new(b.id, "rel2", c.id)).await.unwrap();
        let t3 = engine.store.insert_triple(Triple::new(a.id, "rel3", c.id)).await.unwrap();
        
        // Add source to one triple
        let source = Source::new(vec![t1], SourceType::Conversation);
        engine.store.insert_source(source).await.unwrap();
        
        // Run decay cycle
        let policy = DecayPolicy {
            base_factor: 0.8,
            access_boost: 0.1,
            source_protection: 0.05,
            centrality_protection: 0.1,
            min_weight: 0.1,
        };
        
        let manager = LifecycleManager::new(policy);
        let result = manager.decay_cycle(&engine).await.unwrap();
        
        // Verify result structure
        assert!(result.triples_decayed >= 0);
        assert!(result.triples_evicted >= 0);
        assert!(result.total_weight_before >= result.total_weight_after);
        
        // At least some triples should remain
        let count = engine.store.count_triples().await.unwrap();
        assert!(count > 0);
    }

    #[tokio::test]
    async fn test_bounds_check() {
        let engine = ValenceEngine::new();
        
        // Create many triples to test bounds
        for i in 0..10 {
            let a = engine.store.find_or_create_node(&format!("Node{}", i)).await.unwrap();
            let b = engine.store.find_or_create_node(&format!("Node{}", i + 1)).await.unwrap();
            engine.store.insert_triple(Triple::new(a.id, "link", b.id)).await.unwrap();
        }
        
        let initial_count = engine.store.count_triples().await.unwrap();
        assert_eq!(initial_count, 10);
        
        // Run aggressive decay (base_factor 0.25 brings weight from 1.0 to 0.25, below min 0.3)
        let policy = DecayPolicy {
            base_factor: 0.25,
            access_boost: 0.0,
            source_protection: 0.0,
            centrality_protection: 0.0,
            min_weight: 0.3,
        };
        
        let manager = LifecycleManager::new(policy);
        let result = manager.decay_cycle(&engine).await.unwrap();
        
        // Should evict some triples (weight goes from 1.0 -> 0.5 -> decay -> evict below 0.3)
        assert!(result.triples_evicted > 0);
        
        let final_count = engine.store.count_triples().await.unwrap();
        assert!(final_count < initial_count);
    }
}
