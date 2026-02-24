//! Weight adjustment: strengthen used paths, decay ignored ones.
//!
//! The WeightAdjuster applies feedback signals to triple weights, integrating with
//! the stigmergy module to update access patterns.

use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;

use crate::{
    error::{ValenceError, Result},
    models::TripleId,
    storage::TripleStore,
    stigmergy::AccessTracker,
};

use super::feedback::{UsageFeedback, FeedbackSignal};

/// Strategy for weight adjustment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdjustmentStrategy {
    /// Multiplicative: multiply current weight by signal multiplier
    Multiplicative,
    /// Additive: add/subtract fixed amounts based on signal
    Additive,
    /// Hybrid: combine multiplicative and additive with dampening
    Hybrid,
}

/// Configuration for weight adjustment.
#[derive(Debug, Clone)]
pub struct WeightAdjusterConfig {
    /// Adjustment strategy to use
    pub strategy: AdjustmentStrategy,
    /// Minimum weight (floor to prevent complete decay)
    pub min_weight: f64,
    /// Maximum weight (ceiling to prevent runaway growth)
    pub max_weight: f64,
    /// Dampening factor for hybrid strategy (0.0 - 1.0)
    pub dampening: f64,
    /// Whether to update last_accessed timestamp on positive feedback
    pub update_timestamps: bool,
    /// Whether to integrate with stigmergy (record co-access patterns)
    pub stigmergy_integration: bool,
}

impl Default for WeightAdjusterConfig {
    fn default() -> Self {
        Self {
            strategy: AdjustmentStrategy::Multiplicative,
            min_weight: 0.1,
            max_weight: 10.0,
            dampening: 0.5,
            update_timestamps: true,
            stigmergy_integration: true,
        }
    }
}

/// Apply weight adjustments based on usage feedback.
///
/// The WeightAdjuster is the actuator of the inference loop: it takes feedback
/// about which triples were useful and updates the substrate accordingly.
pub struct WeightAdjuster {
    config: WeightAdjusterConfig,
    store: Arc<RwLock<Box<dyn TripleStore>>>,
    access_tracker: Option<Arc<AccessTracker>>,
}

impl WeightAdjuster {
    /// Create a new WeightAdjuster with default configuration.
    pub fn new(store: Arc<RwLock<Box<dyn TripleStore>>>) -> Self {
        Self::with_config(store, WeightAdjusterConfig::default(), None)
    }

    /// Create a new WeightAdjuster with custom configuration and optional stigmergy integration.
    pub fn with_config(
        store: Arc<RwLock<Box<dyn TripleStore>>>,
        config: WeightAdjusterConfig,
        access_tracker: Option<Arc<AccessTracker>>,
    ) -> Self {
        Self {
            config,
            store,
            access_tracker,
        }
    }

    /// Apply feedback to the substrate: adjust weights and update access patterns.
    ///
    /// This is the core method that closes the inference loop.
    pub async fn apply_feedback(&self, feedback: &UsageFeedback) -> Result<AdjustmentSummary> {
        let mut summary = AdjustmentSummary::new(feedback.context_id.clone());

        // Group feedback by signal type for efficient processing
        let signals_by_type = feedback.signals_by_type();

        for (signal, triple_ids) in signals_by_type {
            for triple_id in triple_ids {
                match self.adjust_triple_weight(triple_id, signal).await {
                    Ok(adjustment) => summary.add_adjustment(adjustment),
                    Err(e) => {
                        summary.add_error(triple_id, e);
                    }
                }
            }
        }

        // Update stigmergy if enabled: record co-access patterns for positive triples
        if self.config.stigmergy_integration {
            if let Some(tracker) = &self.access_tracker {
                let positive_triples = feedback.positive_triples();
                if !positive_triples.is_empty() {
                    tracker
                        .record_access(&positive_triples, &feedback.context_id)
                        .await;
                    summary.stigmergy_updated = true;
                }
            }
        }

        Ok(summary)
    }

    /// Adjust the weight of a single triple based on a feedback signal.
    async fn adjust_triple_weight(
        &self,
        triple_id: TripleId,
        signal: FeedbackSignal,
    ) -> Result<TripleAdjustment> {
        let mut store = self.store.write().await;

        // Fetch current triple
        let mut triple = store
            .get_triple(triple_id)
            .await?
            .ok_or_else(|| ValenceError::Storage(
                crate::error::StorageError::TripleNotFound(format!("Triple {} not found", triple_id))
            ))?;

        let old_weight = triple.local_weight;

        // Calculate new weight based on strategy
        let new_weight = match self.config.strategy {
            AdjustmentStrategy::Multiplicative => {
                old_weight * signal.weight_multiplier()
            }
            AdjustmentStrategy::Additive => {
                let delta = if signal.is_positive() { 0.5 } else { -0.3 };
                old_weight + delta
            }
            AdjustmentStrategy::Hybrid => {
                let multiplicative = old_weight * signal.weight_multiplier();
                let additive = if signal.is_positive() { 0.5 } else { -0.3 };
                let dampening = self.config.dampening;
                (multiplicative * dampening) + (additive * (1.0 - dampening))
            }
        };

        // Clamp to configured bounds
        triple.local_weight = new_weight.clamp(self.config.min_weight, self.config.max_weight);

        // Update timestamp and access count if positive feedback
        if self.config.update_timestamps && signal.is_positive() {
            triple.last_accessed = Some(Utc::now());
            triple.access_count += 1;
        }

        // Persist the updated triple
        store.update_triple(triple.clone()).await?;

        Ok(TripleAdjustment {
            triple_id,
            signal,
            old_weight,
            new_weight: triple.local_weight,
        })
    }

    /// Apply feedback to multiple contexts in batch.
    pub async fn apply_batch(&self, feedback_batch: &[UsageFeedback]) -> Result<Vec<AdjustmentSummary>> {
        let mut summaries = Vec::new();
        for feedback in feedback_batch {
            let summary = self.apply_feedback(feedback).await?;
            summaries.push(summary);
        }
        Ok(summaries)
    }
}

/// Summary of weight adjustments made for a single feedback event.
#[derive(Debug, Clone)]
pub struct AdjustmentSummary {
    /// Context ID this summary is for
    pub context_id: String,
    /// Individual triple adjustments
    pub adjustments: Vec<TripleAdjustment>,
    /// Errors encountered during adjustment
    pub errors: Vec<(TripleId, String)>,
    /// Whether stigmergy was updated
    pub stigmergy_updated: bool,
}

impl AdjustmentSummary {
    fn new(context_id: String) -> Self {
        Self {
            context_id,
            adjustments: Vec::new(),
            errors: Vec::new(),
            stigmergy_updated: false,
        }
    }

    fn add_adjustment(&mut self, adjustment: TripleAdjustment) {
        self.adjustments.push(adjustment);
    }

    fn add_error(&mut self, triple_id: TripleId, error: ValenceError) {
        self.errors.push((triple_id, error.to_string()));
    }

    /// Get count of successfully adjusted triples.
    pub fn success_count(&self) -> usize {
        self.adjustments.len()
    }

    /// Get count of failed adjustments.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Get average weight change across all adjustments.
    pub fn average_weight_change(&self) -> f64 {
        if self.adjustments.is_empty() {
            return 0.0;
        }
        let total_change: f64 = self
            .adjustments
            .iter()
            .map(|adj| adj.new_weight - adj.old_weight)
            .sum();
        total_change / self.adjustments.len() as f64
    }
}

/// Details of a single triple weight adjustment.
#[derive(Debug, Clone)]
pub struct TripleAdjustment {
    /// The triple that was adjusted
    pub triple_id: TripleId,
    /// The feedback signal that triggered adjustment
    pub signal: FeedbackSignal,
    /// Weight before adjustment
    pub old_weight: f64,
    /// Weight after adjustment
    pub new_weight: f64,
}

impl TripleAdjustment {
    /// Calculate the change in weight (new - old).
    pub fn weight_delta(&self) -> f64 {
        self.new_weight - self.old_weight
    }

    /// Calculate the percentage change in weight.
    pub fn weight_change_percent(&self) -> f64 {
        if self.old_weight == 0.0 {
            return 0.0;
        }
        ((self.new_weight - self.old_weight) / self.old_weight) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{Triple, Node, Predicate},
        storage::MemoryStore,
    };
    use uuid::Uuid;

    async fn create_test_store_with_triple() -> (Arc<RwLock<Box<dyn TripleStore>>>, TripleId) {
        let store = MemoryStore::new();
        let subject = Node::new("Alice");
        let object = Node::new("Bob");
        let subject_id = subject.id;
        let object_id = object.id;
        
        store.insert_node(subject).await.unwrap();
        store.insert_node(object).await.unwrap();
        
        let triple = Triple::new(subject_id, "knows", object_id);
        let triple_id = triple.id;
        store.insert_triple(triple).await.unwrap();
        
        let boxed: Box<dyn TripleStore> = Box::new(store);
        (Arc::new(RwLock::new(boxed)), triple_id)
    }

    #[tokio::test]
    async fn test_multiplicative_adjustment() {
        let (store, triple_id) = create_test_store_with_triple().await;
        let adjuster = WeightAdjuster::new(store.clone());

        let adjustment = adjuster
            .adjust_triple_weight(triple_id, FeedbackSignal::Cited)
            .await
            .unwrap();

        assert_eq!(adjustment.old_weight, 1.0);
        assert_eq!(adjustment.new_weight, 1.5); // 1.0 * 1.5
        assert!(adjustment.weight_delta() > 0.0);
    }

    #[tokio::test]
    async fn test_weight_bounds() {
        let (store, triple_id) = create_test_store_with_triple().await;
        let config = WeightAdjusterConfig {
            max_weight: 2.0,
            ..Default::default()
        };
        let adjuster = WeightAdjuster::with_config(store.clone(), config, None);

        // Apply positive feedback multiple times
        for _ in 0..5 {
            adjuster
                .adjust_triple_weight(triple_id, FeedbackSignal::Cited)
                .await
                .unwrap();
        }

        // Check that weight is clamped to max
        let store_lock = store.read().await;
        let triple = store_lock.get_triple(triple_id).await.unwrap().unwrap();
        assert!(triple.local_weight <= 2.0);
    }

    #[tokio::test]
    async fn test_apply_feedback() {
        let (store, triple_id) = create_test_store_with_triple().await;
        let adjuster = WeightAdjuster::new(store.clone());

        let feedback = UsageFeedback::new(
            "test_context",
            vec![super::super::feedback::TripleFeedback {
                triple_id,
                signal: FeedbackSignal::Cited,
            }],
        );

        let summary = adjuster.apply_feedback(&feedback).await.unwrap();

        assert_eq!(summary.success_count(), 1);
        assert_eq!(summary.error_count(), 0);
        assert!(summary.average_weight_change() > 0.0);
    }

    #[tokio::test]
    async fn test_adjustment_summary() {
        let adjustment = TripleAdjustment {
            triple_id: Uuid::new_v4(),
            signal: FeedbackSignal::Cited,
            old_weight: 1.0,
            new_weight: 1.5,
        };

        assert_eq!(adjustment.weight_delta(), 0.5);
        assert_eq!(adjustment.weight_change_percent(), 50.0);
    }
}
