//! Track which triples are accessed together in the same query context.
//!
//! The AccessTracker maintains a sliding window of recent accesses and records
//! co-occurrence patterns. This data feeds the co-retrieval clustering mechanism.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Duration, Utc};

use crate::models::TripleId;

/// A single access event: a set of triples accessed together in a context.
#[derive(Debug, Clone)]
struct AccessEvent {
    /// The triples that were accessed together
    triples: Vec<TripleId>,
    /// Context identifier (e.g., query ID, session ID)
    context: String,
    /// When this access occurred
    timestamp: DateTime<Utc>,
}

/// Configuration for the AccessTracker.
#[derive(Debug, Clone)]
pub struct AccessTrackerConfig {
    /// Maximum number of access events to keep in the sliding window
    pub window_size: usize,
    /// How long to keep access events before they decay (in hours)
    pub decay_hours: i64,
}

impl Default for AccessTrackerConfig {
    fn default() -> Self {
        Self {
            window_size: 10_000,
            decay_hours: 24,
        }
    }
}

/// Track which triples are accessed together in the same query context.
///
/// The AccessTracker maintains a sliding window of recent accesses and provides
/// queries about co-access patterns. This is the foundation of stigmergic
/// self-organization: frequently co-accessed triples should become structurally closer.
#[derive(Clone)]
pub struct AccessTracker {
    /// Configuration
    config: AccessTrackerConfig,
    /// Sliding window of access events (most recent at the back)
    events: Arc<RwLock<VecDeque<AccessEvent>>>,
    /// Co-access counts: (triple_a, triple_b) -> count
    /// Always stored with smaller ID first for consistency
    co_access_counts: Arc<RwLock<HashMap<(TripleId, TripleId), u64>>>,
}

impl AccessTracker {
    /// Create a new AccessTracker with default configuration.
    pub fn new() -> Self {
        Self::with_config(AccessTrackerConfig::default())
    }

    /// Create a new AccessTracker with custom configuration.
    pub fn with_config(config: AccessTrackerConfig) -> Self {
        Self {
            config,
            events: Arc::new(RwLock::new(VecDeque::new())),
            co_access_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record an access event: a set of triples accessed together.
    ///
    /// This updates the sliding window and increments co-access counts for all pairs
    /// within the accessed set.
    ///
    /// # Arguments
    /// * `triple_ids` - The triples that were accessed together
    /// * `context` - A string identifying the query context (e.g., "query_abc123")
    pub async fn record_access(&self, triple_ids: &[TripleId], context: &str) {
        if triple_ids.is_empty() {
            return;
        }

        let event = AccessEvent {
            triples: triple_ids.to_vec(),
            context: context.to_string(),
            timestamp: Utc::now(),
        };

        // Add event to sliding window
        let mut events = self.events.write().await;
        events.push_back(event);

        // Enforce window size limit
        while events.len() > self.config.window_size {
            if let Some(old_event) = events.pop_front() {
                // Decrement co-access counts for the removed event
                self.decrement_pairs(&old_event.triples).await;
            }
        }

        drop(events); // Release lock before next operation

        // Increment co-access counts for all pairs in this access
        self.increment_pairs(triple_ids).await;
    }

    /// Get the co-access count for a specific pair of triples.
    ///
    /// Returns how many times the two triples have been accessed together
    /// within the sliding window.
    pub async fn get_co_access_count(&self, a: TripleId, b: TripleId) -> u64 {
        let key = Self::normalize_pair(a, b);
        let counts = self.co_access_counts.read().await;
        *counts.get(&key).unwrap_or(&0)
    }

    /// Get all co-access pairs above a threshold.
    ///
    /// Returns a vector of ((triple_a, triple_b), count) for all pairs
    /// with count >= min_count.
    pub async fn get_pairs_above_threshold(&self, min_count: u64) -> Vec<((TripleId, TripleId), u64)> {
        let counts = self.co_access_counts.read().await;
        counts
            .iter()
            .filter(|(_, &count)| count >= min_count)
            .map(|(&pair, &count)| (pair, count))
            .collect()
    }

    /// Apply decay: remove events older than the configured decay window.
    ///
    /// Returns the number of events removed.
    pub async fn apply_decay(&self) -> usize {
        let cutoff = Utc::now() - Duration::hours(self.config.decay_hours);
        let mut events = self.events.write().await;
        let initial_len = events.len();

        // Remove events older than cutoff
        while let Some(front) = events.front() {
            if front.timestamp < cutoff {
                let old_event = events.pop_front().unwrap();
                self.decrement_pairs(&old_event.triples).await;
            } else {
                break; // Events are ordered by time
            }
        }

        initial_len - events.len()
    }

    /// Get the current number of tracked access events.
    pub async fn event_count(&self) -> usize {
        let events = self.events.read().await;
        events.len()
    }

    /// Get the number of tracked co-access pairs.
    pub async fn pair_count(&self) -> usize {
        let counts = self.co_access_counts.read().await;
        counts.len()
    }

    /// Clear all tracking data.
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        let mut counts = self.co_access_counts.write().await;
        events.clear();
        counts.clear();
    }

    /// Increment co-access counts for all pairs in a set of triples.
    async fn increment_pairs(&self, triple_ids: &[TripleId]) {
        if triple_ids.len() < 2 {
            return; // Need at least 2 triples for a pair
        }

        let mut counts = self.co_access_counts.write().await;

        // For each unique pair in the set
        for i in 0..triple_ids.len() {
            for j in (i + 1)..triple_ids.len() {
                let key = Self::normalize_pair(triple_ids[i], triple_ids[j]);
                *counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    /// Decrement co-access counts for all pairs in a set of triples.
    async fn decrement_pairs(&self, triple_ids: &[TripleId]) {
        if triple_ids.len() < 2 {
            return;
        }

        let mut counts = self.co_access_counts.write().await;

        // For each unique pair in the set
        for i in 0..triple_ids.len() {
            for j in (i + 1)..triple_ids.len() {
                let key = Self::normalize_pair(triple_ids[i], triple_ids[j]);
                if let Some(count) = counts.get_mut(&key) {
                    if *count > 1 {
                        *count -= 1;
                    } else {
                        counts.remove(&key);
                    }
                }
            }
        }
    }

    /// Normalize a pair of triple IDs to ensure consistent ordering.
    fn normalize_pair(a: TripleId, b: TripleId) -> (TripleId, TripleId) {
        if a < b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

impl Default for AccessTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_record_access() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let t3 = Uuid::new_v4();

        // Record first access
        tracker.record_access(&[t1, t2], "query1").await;
        
        assert_eq!(tracker.event_count().await, 1);
        assert_eq!(tracker.get_co_access_count(t1, t2).await, 1);
        assert_eq!(tracker.get_co_access_count(t1, t3).await, 0);
    }

    #[tokio::test]
    async fn test_multiple_accesses() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();

        // Record same pair multiple times
        tracker.record_access(&[t1, t2], "query1").await;
        tracker.record_access(&[t1, t2], "query2").await;
        tracker.record_access(&[t1, t2], "query3").await;

        assert_eq!(tracker.get_co_access_count(t1, t2).await, 3);
        assert_eq!(tracker.event_count().await, 3);
    }

    #[tokio::test]
    async fn test_pair_normalization() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();

        // Record in different orders
        tracker.record_access(&[t1, t2], "query1").await;
        tracker.record_access(&[t2, t1], "query2").await;

        // Should be counted as the same pair
        assert_eq!(tracker.get_co_access_count(t1, t2).await, 2);
        assert_eq!(tracker.get_co_access_count(t2, t1).await, 2);
    }

    #[tokio::test]
    async fn test_multiple_triples_in_access() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let t3 = Uuid::new_v4();

        // Record three triples accessed together
        tracker.record_access(&[t1, t2, t3], "query1").await;

        // Should create all three pairs: (t1,t2), (t1,t3), (t2,t3)
        assert_eq!(tracker.get_co_access_count(t1, t2).await, 1);
        assert_eq!(tracker.get_co_access_count(t1, t3).await, 1);
        assert_eq!(tracker.get_co_access_count(t2, t3).await, 1);
        assert_eq!(tracker.pair_count().await, 3);
    }

    #[tokio::test]
    async fn test_window_size_limit() {
        let config = AccessTrackerConfig {
            window_size: 3,
            decay_hours: 24,
        };
        let tracker = AccessTracker::with_config(config);

        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let t3 = Uuid::new_v4();
        let t4 = Uuid::new_v4();

        // Add 4 events (window size is 3)
        tracker.record_access(&[t1, t2], "query1").await;
        tracker.record_access(&[t2, t3], "query2").await;
        tracker.record_access(&[t3, t4], "query3").await;
        tracker.record_access(&[t1, t4], "query4").await; // Should evict first event

        // Window should only keep last 3
        assert_eq!(tracker.event_count().await, 3);

        // First pair (t1, t2) should have been decremented when evicted
        assert_eq!(tracker.get_co_access_count(t1, t2).await, 0);
        
        // Other pairs should still be present
        assert_eq!(tracker.get_co_access_count(t2, t3).await, 1);
        assert_eq!(tracker.get_co_access_count(t3, t4).await, 1);
        assert_eq!(tracker.get_co_access_count(t1, t4).await, 1);
    }

    #[tokio::test]
    async fn test_get_pairs_above_threshold() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let t3 = Uuid::new_v4();

        // Create pairs with different counts
        tracker.record_access(&[t1, t2], "query1").await;
        tracker.record_access(&[t1, t2], "query2").await;
        tracker.record_access(&[t1, t2], "query3").await; // count = 3
        
        tracker.record_access(&[t2, t3], "query4").await;
        tracker.record_access(&[t2, t3], "query5").await; // count = 2

        tracker.record_access(&[t1, t3], "query6").await; // count = 1

        // Get pairs with count >= 2
        let pairs = tracker.get_pairs_above_threshold(2).await;
        assert_eq!(pairs.len(), 2);

        // Get pairs with count >= 3
        let pairs = tracker.get_pairs_above_threshold(3).await;
        assert_eq!(pairs.len(), 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();

        tracker.record_access(&[t1, t2], "query1").await;
        
        assert_eq!(tracker.event_count().await, 1);
        assert_eq!(tracker.pair_count().await, 1);

        tracker.clear().await;

        assert_eq!(tracker.event_count().await, 0);
        assert_eq!(tracker.pair_count().await, 0);
    }

    #[tokio::test]
    async fn test_empty_access() {
        let tracker = AccessTracker::new();
        
        // Recording empty access should be a no-op
        tracker.record_access(&[], "query1").await;
        
        assert_eq!(tracker.event_count().await, 0);
    }

    #[tokio::test]
    async fn test_single_triple_access() {
        let tracker = AccessTracker::new();
        
        let t1 = Uuid::new_v4();
        
        // Recording single triple should add event but no pairs
        tracker.record_access(&[t1], "query1").await;
        
        assert_eq!(tracker.event_count().await, 1);
        assert_eq!(tracker.pair_count().await, 0);
    }
}
