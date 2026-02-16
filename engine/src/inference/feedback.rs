//! Feedback recording: track which triples were used/ignored in LLM context.
//!
//! When an LLM processes assembled context, it uses some triples and ignores others.
//! This module captures that feedback and makes it actionable for weight adjustment.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::models::TripleId;

/// Unique identifier for a feedback event.
pub type FeedbackId = Uuid;

/// Signal indicating how a triple was used (or not used) in an LLM context.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FeedbackSignal {
    /// Triple was directly cited/used in the LLM's response
    Cited,
    /// Triple was relevant context that informed the response (but not directly cited)
    Relevant,
    /// Triple was in the context window but ignored
    Ignored,
    /// Triple was misleading or caused confusion
    Misleading,
}

impl FeedbackSignal {
    /// Convert feedback signal to a weight adjustment multiplier.
    ///
    /// - Cited: strong boost (1.5x)
    /// - Relevant: moderate boost (1.2x)
    /// - Ignored: decay (0.9x)
    /// - Misleading: strong decay (0.7x)
    pub fn weight_multiplier(&self) -> f64 {
        match self {
            FeedbackSignal::Cited => 1.5,
            FeedbackSignal::Relevant => 1.2,
            FeedbackSignal::Ignored => 0.9,
            FeedbackSignal::Misleading => 0.7,
        }
    }

    /// Whether this signal represents positive usage (vs negative/decay).
    pub fn is_positive(&self) -> bool {
        matches!(self, FeedbackSignal::Cited | FeedbackSignal::Relevant)
    }
}

/// Feedback for a single triple in a context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripleFeedback {
    /// The triple being evaluated
    pub triple_id: TripleId,
    /// How the triple was used
    pub signal: FeedbackSignal,
}

/// Complete usage feedback for an assembled context.
///
/// After the LLM processes context, the agent submits feedback indicating
/// which triples were useful vs ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageFeedback {
    /// Unique identifier for this feedback event
    pub id: FeedbackId,
    /// Context identifier (links to the original query/context assembly)
    pub context_id: String,
    /// When this feedback was submitted
    pub timestamp: DateTime<Utc>,
    /// Feedback for individual triples
    pub triples: Vec<TripleFeedback>,
    /// Optional: Overall quality score for the assembled context (0.0 - 1.0)
    pub context_quality: Option<f64>,
}

impl UsageFeedback {
    /// Create new usage feedback for a context.
    pub fn new(context_id: impl Into<String>, triples: Vec<TripleFeedback>) -> Self {
        Self {
            id: Uuid::new_v4(),
            context_id: context_id.into(),
            timestamp: Utc::now(),
            triples,
            context_quality: None,
        }
    }

    /// Create new usage feedback with a quality score.
    pub fn with_quality(
        context_id: impl Into<String>,
        triples: Vec<TripleFeedback>,
        quality: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            context_id: context_id.into(),
            timestamp: Utc::now(),
            triples,
            context_quality: Some(quality.clamp(0.0, 1.0)),
        }
    }

    /// Get all triple IDs that received positive feedback.
    pub fn positive_triples(&self) -> Vec<TripleId> {
        self.triples
            .iter()
            .filter(|tf| tf.signal.is_positive())
            .map(|tf| tf.triple_id)
            .collect()
    }

    /// Get all triple IDs that received negative feedback.
    pub fn negative_triples(&self) -> Vec<TripleId> {
        self.triples
            .iter()
            .filter(|tf| !tf.signal.is_positive())
            .map(|tf| tf.triple_id)
            .collect()
    }

    /// Get feedback signals grouped by signal type.
    pub fn signals_by_type(&self) -> HashMap<FeedbackSignal, Vec<TripleId>> {
        let mut map: HashMap<FeedbackSignal, Vec<TripleId>> = HashMap::new();
        for tf in &self.triples {
            map.entry(tf.signal).or_default().push(tf.triple_id);
        }
        map
    }
}

/// Configuration for the FeedbackRecorder.
#[derive(Debug, Clone)]
pub struct FeedbackRecorderConfig {
    /// Maximum number of feedback events to keep in memory
    pub max_history: usize,
    /// Whether to automatically prune old feedback events
    pub auto_prune: bool,
}

impl Default for FeedbackRecorderConfig {
    fn default() -> Self {
        Self {
            max_history: 10_000,
            auto_prune: true,
        }
    }
}

/// Record and retrieve usage feedback for context assemblies.
///
/// The FeedbackRecorder maintains a history of feedback events and provides
/// queries for analyzing usage patterns over time.
#[derive(Clone)]
pub struct FeedbackRecorder {
    config: FeedbackRecorderConfig,
    /// History of feedback events, indexed by feedback ID
    feedback_history: Arc<RwLock<HashMap<FeedbackId, UsageFeedback>>>,
    /// Index: context_id -> feedback IDs for that context
    context_index: Arc<RwLock<HashMap<String, Vec<FeedbackId>>>>,
    /// Index: triple_id -> feedback IDs mentioning that triple
    triple_index: Arc<RwLock<HashMap<TripleId, Vec<FeedbackId>>>>,
}

impl FeedbackRecorder {
    /// Create a new FeedbackRecorder with default configuration.
    pub fn new() -> Self {
        Self::with_config(FeedbackRecorderConfig::default())
    }

    /// Create a new FeedbackRecorder with custom configuration.
    pub fn with_config(config: FeedbackRecorderConfig) -> Self {
        Self {
            config,
            feedback_history: Arc::new(RwLock::new(HashMap::new())),
            context_index: Arc::new(RwLock::new(HashMap::new())),
            triple_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record usage feedback for an assembled context.
    ///
    /// This indexes the feedback event for later retrieval and analysis.
    pub async fn record(&self, feedback: UsageFeedback) {
        let feedback_id = feedback.id;
        let context_id = feedback.context_id.clone();
        let triple_ids: Vec<TripleId> = feedback.triples.iter().map(|tf| tf.triple_id).collect();

        // Store the feedback
        let mut history = self.feedback_history.write().await;
        history.insert(feedback_id, feedback);

        // Auto-prune if needed
        if self.config.auto_prune && history.len() > self.config.max_history {
            // Remove oldest 10% to avoid pruning on every insert
            let prune_count = history.len() / 10;
            let mut sorted: Vec<_> = history.iter().collect();
            sorted.sort_by_key(|(_, fb)| fb.timestamp);
            let to_remove: Vec<FeedbackId> = sorted.iter().take(prune_count).map(|(id, _)| **id).collect();
            for id in to_remove {
                history.remove(&id);
            }
        }
        drop(history);

        // Update context index
        let mut ctx_idx = self.context_index.write().await;
        ctx_idx.entry(context_id).or_default().push(feedback_id);
        drop(ctx_idx);

        // Update triple index
        let mut triple_idx = self.triple_index.write().await;
        for triple_id in triple_ids {
            triple_idx.entry(triple_id).or_default().push(feedback_id);
        }
    }

    /// Retrieve feedback by ID.
    pub async fn get_feedback(&self, id: FeedbackId) -> Option<UsageFeedback> {
        let history = self.feedback_history.read().await;
        history.get(&id).cloned()
    }

    /// Get all feedback events for a specific context.
    pub async fn get_context_feedback(&self, context_id: &str) -> Vec<UsageFeedback> {
        let ctx_idx = self.context_index.read().await;
        let feedback_ids = match ctx_idx.get(context_id) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };
        drop(ctx_idx);

        let history = self.feedback_history.read().await;
        feedback_ids
            .iter()
            .filter_map(|id| history.get(id).cloned())
            .collect()
    }

    /// Get all feedback events mentioning a specific triple.
    pub async fn get_triple_feedback(&self, triple_id: TripleId) -> Vec<UsageFeedback> {
        let triple_idx = self.triple_index.read().await;
        let feedback_ids = match triple_idx.get(&triple_id) {
            Some(ids) => ids.clone(),
            None => return vec![],
        };
        drop(triple_idx);

        let history = self.feedback_history.read().await;
        feedback_ids
            .iter()
            .filter_map(|id| history.get(id).cloned())
            .collect()
    }

    /// Get usage statistics for a triple across all feedback.
    ///
    /// Returns counts of each signal type the triple has received.
    pub async fn get_triple_stats(&self, triple_id: TripleId) -> HashMap<FeedbackSignal, usize> {
        let feedback_events = self.get_triple_feedback(triple_id).await;
        let mut stats: HashMap<FeedbackSignal, usize> = HashMap::new();

        for event in feedback_events {
            for tf in event.triples {
                if tf.triple_id == triple_id {
                    *stats.entry(tf.signal).or_default() += 1;
                }
            }
        }

        stats
    }

    /// Get the average context quality score from recent feedback.
    pub async fn average_context_quality(&self, limit: usize) -> Option<f64> {
        let history = self.feedback_history.read().await;
        let mut sorted: Vec<_> = history.values().collect();
        sorted.sort_by_key(|fb| fb.timestamp);
        sorted.reverse(); // Most recent first

        let qualities: Vec<f64> = sorted
            .iter()
            .take(limit)
            .filter_map(|fb| fb.context_quality)
            .collect();

        if qualities.is_empty() {
            None
        } else {
            Some(qualities.iter().sum::<f64>() / qualities.len() as f64)
        }
    }

    /// Get total number of feedback events recorded.
    pub async fn feedback_count(&self) -> usize {
        self.feedback_history.read().await.len()
    }
}

impl Default for FeedbackRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_feedback_signal_multipliers() {
        assert_eq!(FeedbackSignal::Cited.weight_multiplier(), 1.5);
        assert_eq!(FeedbackSignal::Relevant.weight_multiplier(), 1.2);
        assert_eq!(FeedbackSignal::Ignored.weight_multiplier(), 0.9);
        assert_eq!(FeedbackSignal::Misleading.weight_multiplier(), 0.7);
    }

    #[tokio::test]
    async fn test_usage_feedback_creation() {
        let triple1 = Uuid::new_v4();
        let triple2 = Uuid::new_v4();

        let feedback = UsageFeedback::new(
            "ctx_123",
            vec![
                TripleFeedback {
                    triple_id: triple1,
                    signal: FeedbackSignal::Cited,
                },
                TripleFeedback {
                    triple_id: triple2,
                    signal: FeedbackSignal::Ignored,
                },
            ],
        );

        assert_eq!(feedback.context_id, "ctx_123");
        assert_eq!(feedback.triples.len(), 2);
        assert_eq!(feedback.positive_triples(), vec![triple1]);
        assert_eq!(feedback.negative_triples(), vec![triple2]);
    }

    #[tokio::test]
    async fn test_feedback_recorder() {
        let recorder = FeedbackRecorder::new();

        let triple1 = Uuid::new_v4();
        let feedback = UsageFeedback::new(
            "ctx_123",
            vec![TripleFeedback {
                triple_id: triple1,
                signal: FeedbackSignal::Cited,
            }],
        );

        let feedback_id = feedback.id;
        recorder.record(feedback.clone()).await;

        // Test retrieval by ID
        let retrieved = recorder.get_feedback(feedback_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().context_id, "ctx_123");

        // Test retrieval by context
        let ctx_feedback = recorder.get_context_feedback("ctx_123").await;
        assert_eq!(ctx_feedback.len(), 1);

        // Test retrieval by triple
        let triple_feedback = recorder.get_triple_feedback(triple1).await;
        assert_eq!(triple_feedback.len(), 1);

        // Test stats
        let stats = recorder.get_triple_stats(triple1).await;
        assert_eq!(stats.get(&FeedbackSignal::Cited), Some(&1));
    }

    #[tokio::test]
    async fn test_feedback_recorder_auto_prune() {
        let config = FeedbackRecorderConfig {
            max_history: 10,
            auto_prune: true,
        };
        let recorder = FeedbackRecorder::with_config(config);

        // Insert 15 feedback events
        for i in 0..15 {
            let feedback = UsageFeedback::new(
                format!("ctx_{}", i),
                vec![TripleFeedback {
                    triple_id: Uuid::new_v4(),
                    signal: FeedbackSignal::Cited,
                }],
            );
            recorder.record(feedback).await;
        }

        // Should have pruned to stay under max_history
        let count = recorder.feedback_count().await;
        assert!(count <= 10);
    }

    #[tokio::test]
    async fn test_signals_by_type() {
        let triple1 = Uuid::new_v4();
        let triple2 = Uuid::new_v4();
        let triple3 = Uuid::new_v4();

        let feedback = UsageFeedback::new(
            "ctx_123",
            vec![
                TripleFeedback {
                    triple_id: triple1,
                    signal: FeedbackSignal::Cited,
                },
                TripleFeedback {
                    triple_id: triple2,
                    signal: FeedbackSignal::Cited,
                },
                TripleFeedback {
                    triple_id: triple3,
                    signal: FeedbackSignal::Ignored,
                },
            ],
        );

        let grouped = feedback.signals_by_type();
        assert_eq!(grouped.get(&FeedbackSignal::Cited).unwrap().len(), 2);
        assert_eq!(grouped.get(&FeedbackSignal::Ignored).unwrap().len(), 1);
    }
}
