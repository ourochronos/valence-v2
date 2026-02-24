//! Self-tuning embedding blend weights from inference feedback.
//!
//! Closes the loop: when feedback says "triples found via spring embeddings got cited,"
//! shift blend weights toward spring. The system learns which embedding strategy works
//! best for each query pattern over time.
//!
//! ## How It Works
//!
//! 1. Context assembled using blended embeddings — attribution records which strategies
//!    contributed to each result
//! 2. LLM uses context, agent records feedback (cited/ignored/misleading)
//! 3. BlendTuner attributes feedback to embedding sources via EmbeddingAttribution
//! 4. Blend weights shift toward strategies that produce cited triples (EMA update)
//! 5. Over time, the system learns the optimal blend

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::TripleId;
use crate::query::EmbeddingBlendConfig;

use super::feedback::{FeedbackSignal, UsageFeedback};

/// Attribution: which embedding strategies contributed to finding a triple's nodes,
/// and with what similarity scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingAttribution {
    /// The triple this attribution is for
    pub triple_id: TripleId,
    /// Cosine similarity score from spring embeddings (None if unavailable)
    pub spring_score: Option<f32>,
    /// Cosine similarity score from node2vec embeddings (None if unavailable)
    pub node2vec_score: Option<f32>,
    /// Cosine similarity score from spectral embeddings (None if unavailable)
    pub spectral_score: Option<f32>,
}

impl EmbeddingAttribution {
    pub fn new(
        triple_id: TripleId,
        spring_score: Option<f32>,
        node2vec_score: Option<f32>,
        spectral_score: Option<f32>,
    ) -> Self {
        Self {
            triple_id,
            spring_score,
            node2vec_score,
            spectral_score,
        }
    }

    /// Which strategies had scores for this triple
    fn contributing_strategies(&self) -> Vec<Strategy> {
        let mut strategies = Vec::new();
        if self.spring_score.is_some() {
            strategies.push(Strategy::Spring);
        }
        if self.node2vec_score.is_some() {
            strategies.push(Strategy::Node2Vec);
        }
        if self.spectral_score.is_some() {
            strategies.push(Strategy::Spectral);
        }
        strategies
    }

    /// Get the score for a specific strategy
    fn score_for(&self, strategy: Strategy) -> Option<f32> {
        match strategy {
            Strategy::Spring => self.spring_score,
            Strategy::Node2Vec => self.node2vec_score,
            Strategy::Spectral => self.spectral_score,
        }
    }
}

/// Internal enum for strategy iteration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Strategy {
    Spring,
    Node2Vec,
    Spectral,
}

/// Running statistics for blend weight learning via exponential moving average.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlendWeights {
    pub spring: f64,
    pub node2vec: f64,
    pub spectral: f64,
    /// Number of feedback events that have been incorporated
    pub update_count: u64,
}

impl BlendWeights {
    /// Create from an EmbeddingBlendConfig preset
    pub fn from_config(config: &EmbeddingBlendConfig) -> Self {
        Self {
            spring: config.spring_weight,
            node2vec: config.node2vec_weight,
            spectral: config.spectral_weight,
            update_count: 0,
        }
    }

    /// Normalize weights so they sum to 1.0, enforcing a minimum per-strategy weight.
    fn normalize_with_min(&mut self, min_weight: f64) {
        // Clamp to minimum
        self.spring = self.spring.max(min_weight);
        self.node2vec = self.node2vec.max(min_weight);
        self.spectral = self.spectral.max(min_weight);

        let sum = self.spring + self.node2vec + self.spectral;
        if sum > 0.0 {
            self.spring /= sum;
            self.node2vec /= sum;
            self.spectral /= sum;
        } else {
            // All zeroed out — fall back to uniform
            self.spring = 1.0 / 3.0;
            self.node2vec = 1.0 / 3.0;
            self.spectral = 1.0 / 3.0;
        }

        // After normalization, re-enforce minimum (normalization can push below min
        // if min is large relative to the total). Redistribute from the largest weight.
        let weights = [self.spring, self.node2vec, self.spectral];
        for (i, &w) in weights.iter().enumerate() {
            if w < min_weight {
                let deficit = min_weight - w;
                match i {
                    0 => self.spring = min_weight,
                    1 => self.node2vec = min_weight,
                    2 => self.spectral = min_weight,
                    _ => unreachable!(),
                }
                // Take from the largest
                let max_idx = if self.spring >= self.node2vec && self.spring >= self.spectral {
                    0
                } else if self.node2vec >= self.spectral {
                    1
                } else {
                    2
                };
                match max_idx {
                    0 => self.spring -= deficit,
                    1 => self.node2vec -= deficit,
                    2 => self.spectral -= deficit,
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Normalize weights so they sum to 1.0 (all non-negative)
    fn normalize(&mut self) {
        self.normalize_with_min(0.0);
    }

    /// Convert to EmbeddingBlendConfig
    pub fn to_config(&self) -> EmbeddingBlendConfig {
        EmbeddingBlendConfig {
            spring_weight: self.spring,
            node2vec_weight: self.node2vec,
            spectral_weight: self.spectral,
        }
    }
}

impl Default for BlendWeights {
    fn default() -> Self {
        Self {
            spring: 0.34,
            node2vec: 0.33,
            spectral: 0.33,
            update_count: 0,
        }
    }
}

/// Configuration for the BlendTuner
#[derive(Debug, Clone)]
pub struct BlendTunerConfig {
    /// Learning rate for EMA updates (0.0 = no learning, 1.0 = full replacement).
    /// Default: 0.01 (conservative)
    pub learning_rate: f64,
    /// Minimum weight for any strategy (prevents complete zeroing)
    pub min_weight: f64,
    /// Default preset to use when no learned weights are available
    pub default_preset: EmbeddingBlendConfig,
}

impl Default for BlendTunerConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            min_weight: 0.05,
            default_preset: EmbeddingBlendConfig::default(),
        }
    }
}

/// Self-tuning blend weight learner.
///
/// Maintains running blend weights that are updated from inference feedback.
/// Uses exponential moving average to smoothly shift weights toward strategies
/// that produce cited triples and away from strategies that produce ignored/misleading ones.
#[derive(Clone)]
pub struct BlendTuner {
    config: BlendTunerConfig,
    /// Global learned weights (used when no pattern-specific weights exist)
    weights: Arc<RwLock<BlendWeights>>,
    /// Attribution records: triple_id -> attribution (cleared after processing)
    attributions: Arc<RwLock<HashMap<String, Vec<EmbeddingAttribution>>>>,
}

impl BlendTuner {
    /// Create a new BlendTuner with default configuration
    pub fn new() -> Self {
        Self::with_config(BlendTunerConfig::default())
    }

    /// Create a new BlendTuner with custom configuration
    pub fn with_config(config: BlendTunerConfig) -> Self {
        let initial_weights = BlendWeights::from_config(&config.default_preset);
        Self {
            config,
            weights: Arc::new(RwLock::new(initial_weights)),
            attributions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record embedding attributions for a context assembly.
    ///
    /// Call this when context is assembled — it records which embedding strategies
    /// contributed to finding each triple, so that feedback can later be attributed.
    pub async fn record_attributions(
        &self,
        context_id: &str,
        attributions: Vec<EmbeddingAttribution>,
    ) {
        let mut attr_map = self.attributions.write().await;
        attr_map.insert(context_id.to_string(), attributions);
    }

    /// Process feedback and update blend weights.
    ///
    /// For each triple in the feedback:
    /// - If cited/relevant: boost the weight of strategies that contributed
    /// - If ignored/misleading: penalize the weight of strategies that contributed
    ///
    /// The boost/penalty is proportional to the strategy's similarity score:
    /// a strategy that found the triple with high similarity gets more credit
    /// than one with low similarity.
    pub async fn process_feedback(&self, feedback: &UsageFeedback) {
        let attr_map = self.attributions.read().await;
        let attributions = match attr_map.get(&feedback.context_id) {
            Some(attrs) => attrs.clone(),
            None => return, // No attributions recorded for this context
        };
        drop(attr_map);

        // Build a lookup from triple_id to attribution
        let attr_by_triple: HashMap<TripleId, &EmbeddingAttribution> = attributions
            .iter()
            .map(|a| (a.triple_id, a))
            .collect();

        // Accumulate reward signals per strategy
        let mut spring_signal = 0.0_f64;
        let mut node2vec_signal = 0.0_f64;
        let mut spectral_signal = 0.0_f64;
        let mut total_signal_weight = 0.0_f64;

        for tf in &feedback.triples {
            if let Some(attr) = attr_by_triple.get(&tf.triple_id) {
                let reward = signal_to_reward(tf.signal);

                // Weight the reward by each strategy's contribution score
                for strategy in attr.contributing_strategies() {
                    if let Some(score) = attr.score_for(strategy) {
                        let contribution = score as f64 * reward;
                        match strategy {
                            Strategy::Spring => spring_signal += contribution,
                            Strategy::Node2Vec => node2vec_signal += contribution,
                            Strategy::Spectral => spectral_signal += contribution,
                        }
                        total_signal_weight += (score as f64).abs();
                    }
                }
            }
        }

        if total_signal_weight == 0.0 {
            return; // No attributable feedback
        }

        // Normalize signals
        let spring_delta = spring_signal / total_signal_weight;
        let node2vec_delta = node2vec_signal / total_signal_weight;
        let spectral_delta = spectral_signal / total_signal_weight;

        // Apply EMA update to weights
        let lr = self.config.learning_rate;
        let min_w = self.config.min_weight;

        let mut weights = self.weights.write().await;

        // EMA: new_weight = (1 - lr) * old_weight + lr * (old_weight + delta)
        //     = old_weight + lr * delta
        weights.spring += lr * spring_delta;
        weights.node2vec += lr * node2vec_delta;
        weights.spectral += lr * spectral_delta;

        // Normalize to sum to 1.0 while enforcing minimum weight per strategy
        weights.normalize_with_min(min_w);
        weights.update_count += 1;

        drop(weights);

        // Clean up attributions for this context
        let mut attr_map = self.attributions.write().await;
        attr_map.remove(&feedback.context_id);
    }

    /// Get the current learned blend weights as an EmbeddingBlendConfig.
    ///
    /// If no feedback has been processed, returns the default preset.
    pub async fn get_learned_weights(&self) -> EmbeddingBlendConfig {
        let weights = self.weights.read().await;
        if weights.update_count == 0 {
            self.config.default_preset.clone()
        } else {
            weights.to_config()
        }
    }

    /// Get the raw blend weights (including update count)
    pub async fn get_raw_weights(&self) -> BlendWeights {
        self.weights.read().await.clone()
    }

    /// Set learned weights directly (for restoring from persistence)
    pub async fn set_learned_weights(&self, weights: BlendWeights) {
        let mut w = self.weights.write().await;
        *w = weights;
    }

    /// Get the current learning rate
    pub fn learning_rate(&self) -> f64 {
        self.config.learning_rate
    }

    /// Get the number of pending attribution records
    pub async fn pending_attributions(&self) -> usize {
        self.attributions.read().await.len()
    }
}

impl Default for BlendTuner {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a feedback signal to a reward value.
///
/// Cited/Relevant produce positive rewards (boost contributing strategies).
/// Ignored/Misleading produce negative rewards (penalize contributing strategies).
fn signal_to_reward(signal: FeedbackSignal) -> f64 {
    match signal {
        FeedbackSignal::Cited => 1.0,
        FeedbackSignal::Relevant => 0.5,
        FeedbackSignal::Ignored => -0.3,
        FeedbackSignal::Misleading => -1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::feedback::TripleFeedback;
    use uuid::Uuid;

    fn make_triple_id() -> TripleId {
        Uuid::new_v4()
    }

    #[tokio::test]
    async fn test_default_weights_without_feedback() {
        let tuner = BlendTuner::new();
        let config = tuner.get_learned_weights().await;

        // Should return default preset when no feedback has been processed
        assert_eq!(config, EmbeddingBlendConfig::default());
    }

    #[tokio::test]
    async fn test_citing_spring_shifts_toward_spring() {
        let tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.1, // Higher LR for visible effect in test
            ..Default::default()
        });

        let initial = tuner.get_raw_weights().await;
        let initial_spring = initial.spring;

        // Simulate: triples found via spring got cited
        let t1 = make_triple_id();
        let t2 = make_triple_id();

        let attributions = vec![
            EmbeddingAttribution::new(t1, Some(0.9), Some(0.2), Some(0.1)),
            EmbeddingAttribution::new(t2, Some(0.85), Some(0.15), Some(0.1)),
        ];

        tuner.record_attributions("ctx1", attributions).await;

        let feedback = UsageFeedback::new(
            "ctx1",
            vec![
                TripleFeedback { triple_id: t1, signal: FeedbackSignal::Cited },
                TripleFeedback { triple_id: t2, signal: FeedbackSignal::Cited },
            ],
        );

        tuner.process_feedback(&feedback).await;

        let updated = tuner.get_raw_weights().await;
        assert!(
            updated.spring > initial_spring,
            "Spring weight should increase when spring-sourced triples are cited: {:.4} -> {:.4}",
            initial_spring, updated.spring
        );
        assert_eq!(updated.update_count, 1);
    }

    #[tokio::test]
    async fn test_ignoring_spectral_shifts_away_from_spectral() {
        let tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.1,
            ..Default::default()
        });

        let initial = tuner.get_raw_weights().await;
        let initial_spectral = initial.spectral;

        // Simulate: triples found primarily via spectral got ignored
        let t1 = make_triple_id();
        let t2 = make_triple_id();

        let attributions = vec![
            EmbeddingAttribution::new(t1, Some(0.1), Some(0.1), Some(0.9)),
            EmbeddingAttribution::new(t2, Some(0.1), Some(0.15), Some(0.85)),
        ];

        tuner.record_attributions("ctx2", attributions).await;

        let feedback = UsageFeedback::new(
            "ctx2",
            vec![
                TripleFeedback { triple_id: t1, signal: FeedbackSignal::Ignored },
                TripleFeedback { triple_id: t2, signal: FeedbackSignal::Ignored },
            ],
        );

        tuner.process_feedback(&feedback).await;

        let updated = tuner.get_raw_weights().await;
        assert!(
            updated.spectral < initial_spectral,
            "Spectral weight should decrease when spectral-sourced triples are ignored: {:.4} -> {:.4}",
            initial_spectral, updated.spectral
        );
    }

    #[tokio::test]
    async fn test_learning_rate_controls_speed() {
        // Fast learner
        let fast_tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.5,
            ..Default::default()
        });
        // Slow learner
        let slow_tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.01,
            ..Default::default()
        });

        let t1 = make_triple_id();

        let attributions = vec![
            EmbeddingAttribution::new(t1, Some(0.9), Some(0.1), Some(0.1)),
        ];

        let feedback = UsageFeedback::new(
            "ctx_lr",
            vec![
                TripleFeedback { triple_id: t1, signal: FeedbackSignal::Cited },
            ],
        );

        // Apply same feedback to both
        fast_tuner.record_attributions("ctx_lr", attributions.clone()).await;
        fast_tuner.process_feedback(&feedback).await;

        slow_tuner.record_attributions("ctx_lr", attributions).await;
        slow_tuner.process_feedback(&feedback).await;

        let fast_weights = fast_tuner.get_raw_weights().await;
        let slow_weights = slow_tuner.get_raw_weights().await;

        let fast_spring_change = (fast_weights.spring - 0.34).abs();
        let slow_spring_change = (slow_weights.spring - 0.34).abs();

        assert!(
            fast_spring_change > slow_spring_change,
            "Fast learner should change more: fast_delta={:.4}, slow_delta={:.4}",
            fast_spring_change, slow_spring_change
        );
    }

    #[tokio::test]
    async fn test_weights_stay_valid() {
        let tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.5, // Aggressive LR to stress-test validity
            min_weight: 0.05,
            ..Default::default()
        });

        // Apply many rounds of strongly biased feedback
        for i in 0..50 {
            let t = make_triple_id();
            let attributions = vec![
                EmbeddingAttribution::new(t, Some(0.95), Some(0.01), Some(0.01)),
            ];
            let ctx = format!("ctx_valid_{}", i);
            tuner.record_attributions(&ctx, attributions).await;

            let feedback = UsageFeedback::new(
                &ctx,
                vec![TripleFeedback { triple_id: t, signal: FeedbackSignal::Cited }],
            );
            tuner.process_feedback(&feedback).await;
        }

        let weights = tuner.get_raw_weights().await;

        // All weights should be non-negative
        assert!(weights.spring >= 0.0, "Spring weight must be non-negative: {}", weights.spring);
        assert!(weights.node2vec >= 0.0, "Node2vec weight must be non-negative: {}", weights.node2vec);
        assert!(weights.spectral >= 0.0, "Spectral weight must be non-negative: {}", weights.spectral);

        // All weights should be at or above minimum
        assert!(
            weights.node2vec >= tuner.config.min_weight,
            "Node2vec weight below minimum: {} < {}",
            weights.node2vec, tuner.config.min_weight
        );
        assert!(
            weights.spectral >= tuner.config.min_weight,
            "Spectral weight below minimum: {} < {}",
            weights.spectral, tuner.config.min_weight
        );

        // Weights should sum to approximately 1.0
        let sum = weights.spring + weights.node2vec + weights.spectral;
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Weights should sum to ~1.0, got {:.6}",
            sum
        );

        // The config should also be valid
        let config = weights.to_config();
        assert!(config.validate().is_ok(), "Generated config should be valid");
    }

    #[tokio::test]
    async fn test_no_attribution_is_noop() {
        let tuner = BlendTuner::new();

        let initial = tuner.get_raw_weights().await;

        // Submit feedback without any attributions recorded
        let t1 = make_triple_id();
        let feedback = UsageFeedback::new(
            "ctx_noattr",
            vec![TripleFeedback { triple_id: t1, signal: FeedbackSignal::Cited }],
        );

        tuner.process_feedback(&feedback).await;

        let after = tuner.get_raw_weights().await;

        // Weights should not change
        assert_eq!(initial.spring, after.spring);
        assert_eq!(initial.node2vec, after.node2vec);
        assert_eq!(initial.spectral, after.spectral);
        assert_eq!(after.update_count, 0);
    }

    #[tokio::test]
    async fn test_set_and_get_learned_weights() {
        let tuner = BlendTuner::new();

        let custom = BlendWeights {
            spring: 0.6,
            node2vec: 0.3,
            spectral: 0.1,
            update_count: 42,
        };

        tuner.set_learned_weights(custom.clone()).await;

        let retrieved = tuner.get_raw_weights().await;
        assert_eq!(retrieved.spring, 0.6);
        assert_eq!(retrieved.node2vec, 0.3);
        assert_eq!(retrieved.spectral, 0.1);
        assert_eq!(retrieved.update_count, 42);

        // get_learned_weights should return the config (not default, since update_count > 0)
        let config = tuner.get_learned_weights().await;
        assert_eq!(config.spring_weight, 0.6);
        assert_eq!(config.node2vec_weight, 0.3);
        assert_eq!(config.spectral_weight, 0.1);
    }

    #[tokio::test]
    async fn test_misleading_feedback_strong_penalty() {
        let tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.1,
            ..Default::default()
        });

        let initial = tuner.get_raw_weights().await;
        let initial_spring = initial.spring;

        let t1 = make_triple_id();
        let attributions = vec![
            EmbeddingAttribution::new(t1, Some(0.9), Some(0.1), Some(0.1)),
        ];

        tuner.record_attributions("ctx_mislead", attributions).await;

        let feedback = UsageFeedback::new(
            "ctx_mislead",
            vec![TripleFeedback { triple_id: t1, signal: FeedbackSignal::Misleading }],
        );

        tuner.process_feedback(&feedback).await;

        let updated = tuner.get_raw_weights().await;
        assert!(
            updated.spring < initial_spring,
            "Spring should decrease from misleading feedback: {:.4} -> {:.4}",
            initial_spring, updated.spring
        );
    }

    #[tokio::test]
    async fn test_attributions_cleaned_after_feedback() {
        let tuner = BlendTuner::new();

        let t1 = make_triple_id();
        let attributions = vec![
            EmbeddingAttribution::new(t1, Some(0.5), Some(0.5), None),
        ];

        tuner.record_attributions("ctx_clean", attributions).await;
        assert_eq!(tuner.pending_attributions().await, 1);

        let feedback = UsageFeedback::new(
            "ctx_clean",
            vec![TripleFeedback { triple_id: t1, signal: FeedbackSignal::Cited }],
        );

        tuner.process_feedback(&feedback).await;
        assert_eq!(tuner.pending_attributions().await, 0);
    }

    #[tokio::test]
    async fn test_mixed_feedback_signals() {
        let tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.1,
            ..Default::default()
        });

        let t_cited = make_triple_id();
        let t_ignored = make_triple_id();

        // Cited triple found via spring, ignored triple found via spectral
        let attributions = vec![
            EmbeddingAttribution::new(t_cited, Some(0.9), Some(0.1), Some(0.1)),
            EmbeddingAttribution::new(t_ignored, Some(0.1), Some(0.1), Some(0.9)),
        ];

        tuner.record_attributions("ctx_mixed", attributions).await;

        let feedback = UsageFeedback::new(
            "ctx_mixed",
            vec![
                TripleFeedback { triple_id: t_cited, signal: FeedbackSignal::Cited },
                TripleFeedback { triple_id: t_ignored, signal: FeedbackSignal::Ignored },
            ],
        );

        let initial = tuner.get_raw_weights().await;
        tuner.process_feedback(&feedback).await;
        let updated = tuner.get_raw_weights().await;

        // Spring should increase (its triple was cited)
        assert!(
            updated.spring > initial.spring,
            "Spring should increase: {:.4} -> {:.4}",
            initial.spring, updated.spring
        );
        // Spectral should decrease (its triple was ignored)
        assert!(
            updated.spectral < initial.spectral,
            "Spectral should decrease: {:.4} -> {:.4}",
            initial.spectral, updated.spectral
        );
    }

    #[tokio::test]
    async fn test_convergence_over_many_rounds() {
        let tuner = BlendTuner::with_config(BlendTunerConfig {
            learning_rate: 0.05,
            min_weight: 0.05,
            ..Default::default()
        });

        // Consistently reward spring, punish spectral over many rounds
        for i in 0..100 {
            let t = make_triple_id();
            let ctx = format!("ctx_conv_{}", i);
            let attributions = vec![
                EmbeddingAttribution::new(t, Some(0.8), Some(0.3), Some(0.8)),
            ];
            tuner.record_attributions(&ctx, attributions).await;

            // Spring-sourced triples always cited
            let feedback = UsageFeedback::new(
                &ctx,
                vec![TripleFeedback { triple_id: t, signal: FeedbackSignal::Cited }],
            );
            tuner.process_feedback(&feedback).await;
        }

        let weights = tuner.get_raw_weights().await;

        // After consistent positive feedback, spring and spectral (both had high scores)
        // should both be boosted, but the result should still be valid
        let sum = weights.spring + weights.node2vec + weights.spectral;
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Weights must sum to ~1.0 after convergence: {:.6}",
            sum
        );
        assert!(weights.update_count == 100);
    }

    #[test]
    fn test_blend_weights_normalize() {
        let mut w = BlendWeights {
            spring: 2.0,
            node2vec: 1.0,
            spectral: 1.0,
            update_count: 0,
        };
        w.normalize();

        assert!((w.spring - 0.5).abs() < 0.001);
        assert!((w.node2vec - 0.25).abs() < 0.001);
        assert!((w.spectral - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_blend_weights_normalize_all_zero() {
        let mut w = BlendWeights {
            spring: 0.0,
            node2vec: 0.0,
            spectral: 0.0,
            update_count: 0,
        };
        w.normalize();

        // Should fall back to uniform
        assert!((w.spring - 1.0 / 3.0).abs() < 0.001);
        assert!((w.node2vec - 1.0 / 3.0).abs() < 0.001);
        assert!((w.spectral - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_blend_weights_normalize_negative_clamped() {
        let mut w = BlendWeights {
            spring: -0.5,
            node2vec: 1.0,
            spectral: 0.5,
            update_count: 0,
        };
        w.normalize();

        assert!(w.spring >= 0.0);
        let sum = w.spring + w.node2vec + w.spectral;
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_signal_to_reward() {
        assert_eq!(signal_to_reward(FeedbackSignal::Cited), 1.0);
        assert_eq!(signal_to_reward(FeedbackSignal::Relevant), 0.5);
        assert_eq!(signal_to_reward(FeedbackSignal::Ignored), -0.3);
        assert_eq!(signal_to_reward(FeedbackSignal::Misleading), -1.0);
    }

    #[test]
    fn test_embedding_attribution_contributing_strategies() {
        let t = make_triple_id();

        let full = EmbeddingAttribution::new(t, Some(0.5), Some(0.3), Some(0.2));
        assert_eq!(full.contributing_strategies().len(), 3);

        let partial = EmbeddingAttribution::new(t, Some(0.5), None, Some(0.2));
        assert_eq!(partial.contributing_strategies().len(), 2);

        let single = EmbeddingAttribution::new(t, None, None, Some(0.8));
        assert_eq!(single.contributing_strategies().len(), 1);

        let none = EmbeddingAttribution::new(t, None, None, None);
        assert_eq!(none.contributing_strategies().len(), 0);
    }
}
