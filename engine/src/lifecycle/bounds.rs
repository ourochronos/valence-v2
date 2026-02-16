//! Hard memory bounds with intelligent eviction.
//!
//! The system enforces hard limits on triple and node counts,
//! evicting lowest-weight triples when limits are exceeded.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::storage::{TripleStore, TriplePattern};

/// Memory bounds configuration.
///
/// Enforces hard limits on the size of the knowledge graph,
/// with automatic eviction when limits are exceeded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBounds {
    /// Hard cap on triple count
    pub max_triples: usize,
    
    /// Hard cap on node count
    pub max_nodes: usize,
    
    /// Target utilization (0.0-1.0, default 0.8)
    /// When bounds are exceeded, evict down to this percentage
    pub target_utilization: f64,
}

impl Default for MemoryBounds {
    fn default() -> Self {
        Self {
            max_triples: 10_000,
            max_nodes: 5_000,
            target_utilization: 0.8,
        }
    }
}

/// Status of memory bounds check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundsStatus {
    /// Current number of triples
    pub current_triples: u64,
    
    /// Current number of nodes
    pub current_nodes: u64,
    
    /// Maximum allowed triples
    pub max_triples: usize,
    
    /// Maximum allowed nodes
    pub max_nodes: usize,
    
    /// Whether triple limit is exceeded
    pub triples_exceeded: bool,
    
    /// Whether node limit is exceeded
    pub nodes_exceeded: bool,
    
    /// Utilization percentage (0.0-1.0)
    pub utilization: f64,
}

/// Result of bounds enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforceResult {
    /// Number of triples evicted
    pub triples_evicted: u64,
    
    /// Number of nodes removed (orphaned after triple eviction)
    pub nodes_removed: u64,
    
    /// Final triple count after enforcement
    pub final_triple_count: u64,
    
    /// Final node count after enforcement
    pub final_node_count: u64,
    
    /// Whether target was reached
    pub target_reached: bool,
}

impl MemoryBounds {
    /// Create new bounds with specified limits.
    pub fn new(max_triples: usize, max_nodes: usize, target_utilization: f64) -> Self {
        Self {
            max_triples,
            max_nodes,
            target_utilization,
        }
    }
    
    /// Check if bounds are exceeded.
    ///
    /// Returns status information about current memory usage
    /// and whether limits are exceeded.
    pub async fn check(&self, store: &dyn TripleStore) -> Result<BoundsStatus> {
        let current_triples = store.count_triples().await?;
        let current_nodes = store.count_nodes().await?;
        
        let triples_exceeded = current_triples > self.max_triples as u64;
        let nodes_exceeded = current_nodes > self.max_nodes as u64;
        
        // Calculate utilization as the max of triple and node utilization
        let triple_util = current_triples as f64 / self.max_triples as f64;
        let node_util = current_nodes as f64 / self.max_nodes as f64;
        let utilization = triple_util.max(node_util);
        
        Ok(BoundsStatus {
            current_triples,
            current_nodes,
            max_triples: self.max_triples,
            max_nodes: self.max_nodes,
            triples_exceeded,
            nodes_exceeded,
            utilization,
        })
    }
    
    /// Enforce bounds by evicting lowest-weight triples.
    ///
    /// If bounds are exceeded, evict triples (starting with lowest weight)
    /// until target utilization is reached.
    ///
    /// # Algorithm
    /// 1. Check if bounds are exceeded
    /// 2. If yes, calculate target counts (max * target_utilization)
    /// 3. Sort all triples by weight (ascending)
    /// 4. Evict lowest-weight triples until target is reached
    /// 5. Orphaned nodes are automatically removed by the store
    pub async fn enforce(&self, store: &dyn TripleStore) -> Result<EnforceResult> {
        let status = self.check(store).await?;
        
        // If neither limit is exceeded, nothing to do
        if !status.triples_exceeded && !status.nodes_exceeded {
            return Ok(EnforceResult {
                triples_evicted: 0,
                nodes_removed: 0,
                final_triple_count: status.current_triples,
                final_node_count: status.current_nodes,
                target_reached: true,
            });
        }
        
        // Calculate target counts
        let target_triples = (self.max_triples as f64 * self.target_utilization) as u64;
        let target_nodes = (self.max_nodes as f64 * self.target_utilization) as u64;
        
        // Determine how many triples to evict
        let triples_to_evict = if status.triples_exceeded {
            status.current_triples.saturating_sub(target_triples)
        } else {
            // Node limit exceeded - estimate triples to remove based on node target
            // Rough heuristic: each triple connects 2 nodes on average
            let nodes_to_remove = status.current_nodes.saturating_sub(target_nodes);
            nodes_to_remove / 2
        };
        
        if triples_to_evict == 0 {
            return Ok(EnforceResult {
                triples_evicted: 0,
                nodes_removed: 0,
                final_triple_count: status.current_triples,
                final_node_count: status.current_nodes,
                target_reached: true,
            });
        }
        
        // Get all triples and sort by weight
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        };
        let mut triples = store.query_triples(pattern).await?;
        
        // Sort by weight ascending (lowest first)
        triples.sort_by(|a, b| {
            a.weight.partial_cmp(&b.weight).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Evict lowest-weight triples
        let mut evicted = 0u64;
        for triple in triples.iter().take(triples_to_evict as usize) {
            store.delete_triple(triple.id).await?;
            evicted += 1;
        }
        
        // Get final counts
        let final_triple_count = store.count_triples().await?;
        let final_node_count = store.count_nodes().await?;
        
        // Calculate nodes removed (orphaned nodes may be cleaned up by store)
        let nodes_removed = status.current_nodes.saturating_sub(final_node_count);
        
        // Check if target was reached
        let target_reached = final_triple_count <= target_triples && final_node_count <= target_nodes;
        
        Ok(EnforceResult {
            triples_evicted: evicted,
            nodes_removed,
            final_triple_count,
            final_node_count,
            target_reached,
        })
    }
    
    /// Get utilization percentage (0.0-1.0).
    pub async fn utilization(&self, store: &dyn TripleStore) -> Result<f64> {
        let status = self.check(store).await?;
        Ok(status.utilization)
    }
    
    /// Check if bounds are exceeded.
    pub async fn is_exceeded(&self, store: &dyn TripleStore) -> Result<bool> {
        let status = self.check(store).await?;
        Ok(status.triples_exceeded || status.nodes_exceeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::Triple,
        storage::MemoryStore,
    };

    #[tokio::test]
    async fn test_check_bounds_not_exceeded() {
        let store = MemoryStore::new();
        
        // Add a few triples
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();
        
        let bounds = MemoryBounds {
            max_triples: 10,
            max_nodes: 10,
            target_utilization: 0.8,
        };
        
        let status = bounds.check(&store).await.unwrap();
        
        assert!(!status.triples_exceeded);
        assert!(!status.nodes_exceeded);
        assert_eq!(status.current_triples, 1);
        assert_eq!(status.current_nodes, 2);
        assert!(status.utilization < 1.0);
    }

    #[tokio::test]
    async fn test_check_bounds_exceeded() {
        let store = MemoryStore::new();
        
        // Add many triples to exceed bounds
        for i in 0..15 {
            let a = store.find_or_create_node(&format!("N{}", i)).await.unwrap();
            let b = store.find_or_create_node(&format!("N{}", i + 1)).await.unwrap();
            store.insert_triple(Triple::new(a.id, "link", b.id)).await.unwrap();
        }
        
        let bounds = MemoryBounds {
            max_triples: 10,
            max_nodes: 10,
            target_utilization: 0.8,
        };
        
        let status = bounds.check(&store).await.unwrap();
        
        assert!(status.triples_exceeded);
        assert!(status.nodes_exceeded);
        assert_eq!(status.current_triples, 15);
        assert!(status.utilization > 1.0);
    }

    #[tokio::test]
    async fn test_enforce_bounds_evicts_lowest_weight() {
        let store = MemoryStore::new();
        
        // Create triples with varying weights
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        let d = store.find_or_create_node("D").await.unwrap();
        let e = store.find_or_create_node("E").await.unwrap();
        
        let t1 = store.insert_triple(Triple::new(a.id, "r1", b.id)).await.unwrap();
        let t2 = store.insert_triple(Triple::new(b.id, "r2", c.id)).await.unwrap();
        let t3 = store.insert_triple(Triple::new(c.id, "r3", d.id)).await.unwrap();
        let t4 = store.insert_triple(Triple::new(d.id, "r4", e.id)).await.unwrap();
        
        // Manually decay some triples to create weight differences
        store.decay(0.5, 0.0).await.unwrap(); // All at 0.5
        
        // Boost t3 and t4 by accessing them
        store.touch_triple(t3).await.unwrap(); // Back to 1.0
        store.touch_triple(t4).await.unwrap(); // Back to 1.0
        
        // Now weights are: t1=0.5, t2=0.5, t3=1.0, t4=1.0
        
        let bounds = MemoryBounds {
            max_triples: 3,
            max_nodes: 10,
            target_utilization: 0.8,
        };
        
        let result = bounds.enforce(&store).await.unwrap();
        
        // Should evict 1 triple (4 - 3*0.8 = 4 - 2.4 ≈ 2, but we round)
        assert!(result.triples_evicted > 0);
        assert!(result.final_triple_count <= 3);
        
        // Check that t3 and t4 still exist (higher weight)
        let t3_after = store.get_triple(t3).await.unwrap();
        let t4_after = store.get_triple(t4).await.unwrap();
        
        assert!(t3_after.is_some());
        assert!(t4_after.is_some());
        
        // t1 or t2 should be evicted (lower weight)
        let t1_after = store.get_triple(t1).await.unwrap();
        let t2_after = store.get_triple(t2).await.unwrap();
        
        // At least one should be gone
        assert!(t1_after.is_none() || t2_after.is_none());
    }

    #[tokio::test]
    async fn test_enforce_bounds_no_action_when_ok() {
        let store = MemoryStore::new();
        
        // Add just 2 triples (under limit)
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "rel", c.id)).await.unwrap();
        
        let bounds = MemoryBounds {
            max_triples: 10,
            max_nodes: 10,
            target_utilization: 0.8,
        };
        
        let result = bounds.enforce(&store).await.unwrap();
        
        assert_eq!(result.triples_evicted, 0);
        assert_eq!(result.final_triple_count, 2);
        assert!(result.target_reached);
    }

    #[tokio::test]
    async fn test_utilization_calculation() {
        let store = MemoryStore::new();
        
        // Add 7 triples out of 10 max
        for i in 0..7 {
            let a = store.find_or_create_node(&format!("N{}", i)).await.unwrap();
            let b = store.find_or_create_node(&format!("N{}", i + 1)).await.unwrap();
            store.insert_triple(Triple::new(a.id, "link", b.id)).await.unwrap();
        }
        
        let bounds = MemoryBounds {
            max_triples: 10,
            max_nodes: 20,
            target_utilization: 0.8,
        };
        
        let util = bounds.utilization(&store).await.unwrap();
        
        // Should be 7/10 = 0.7
        assert!((util - 0.7).abs() < 0.1);
        
        let exceeded = bounds.is_exceeded(&store).await.unwrap();
        assert!(!exceeded);
    }

    #[tokio::test]
    async fn test_enforce_reaches_target() {
        let store = MemoryStore::new();
        
        // Create 20 triples (way over limit)
        for i in 0..20 {
            let a = store.find_or_create_node(&format!("N{}", i)).await.unwrap();
            let b = store.find_or_create_node(&format!("N{}", i + 1)).await.unwrap();
            store.insert_triple(Triple::new(a.id, "link", b.id)).await.unwrap();
        }
        
        let bounds = MemoryBounds {
            max_triples: 10,
            max_nodes: 15,
            target_utilization: 0.8,
        };
        
        let result = bounds.enforce(&store).await.unwrap();
        
        // Should evict down to ~8 triples (10 * 0.8)
        assert!(result.triples_evicted >= 10);
        assert!(result.final_triple_count <= 10);
        
        // Target should be reached
        let status = bounds.check(&store).await.unwrap();
        assert!(!status.triples_exceeded);
    }

    #[tokio::test]
    async fn test_bounds_with_target_utilization() {
        let store = MemoryStore::new();
        
        // Add exactly 10 triples (at limit)
        for i in 0..10 {
            let a = store.find_or_create_node(&format!("N{}", i)).await.unwrap();
            let b = store.find_or_create_node(&format!("N{}", i + 1)).await.unwrap();
            store.insert_triple(Triple::new(a.id, "link", b.id)).await.unwrap();
        }
        
        let bounds = MemoryBounds {
            max_triples: 10,
            max_nodes: 20,
            target_utilization: 0.6, // Lower target
        };
        
        let result = bounds.enforce(&store).await.unwrap();
        
        // Should evict some to reach 60% of max
        assert!(result.triples_evicted > 0);
        assert!(result.final_triple_count <= 6); // 10 * 0.6 = 6
    }
}
