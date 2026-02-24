//! Multi-dimensional fusion scoring for retrieval.
//!
//! Combines multiple retrieval signals (embedding similarity, graph distance,
//! dynamic confidence, access recency, source count) into a unified relevance score.
//!
//! Design philosophy:
//! - Weighted combination with configurable weights
//! - Signals are normalized to [0.0, 1.0] range
//! - Computed dynamically from graph topology (not stored metadata)
//! - Context-dependent: same triple can score differently in different queries

use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use crate::models::TripleId;

/// Configuration for multi-dimensional fusion scoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct FusionConfig {
    /// Weight for embedding cosine similarity (default 0.35)
    pub similarity_weight: f64,
    /// Weight for dynamic confidence from topology (default 0.25)
    pub confidence_weight: f64,
    /// Weight for last_accessed freshness (default 0.20)
    pub recency_weight: f64,
    /// Weight for inverse hop count in graph (default 0.10)
    pub graph_distance_weight: f64,
    /// Weight for number of provenance sources (default 0.10)
    pub source_count_weight: f64,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            similarity_weight: 0.35,
            confidence_weight: 0.25,
            recency_weight: 0.20,
            graph_distance_weight: 0.10,
            source_count_weight: 0.10,
        }
    }
}

impl FusionConfig {
    /// Create a new fusion config with custom weights
    pub fn new(
        similarity_weight: f64,
        confidence_weight: f64,
        recency_weight: f64,
        graph_distance_weight: f64,
        source_count_weight: f64,
    ) -> Self {
        Self {
            similarity_weight,
            confidence_weight,
            recency_weight,
            graph_distance_weight,
            source_count_weight,
        }
    }

    /// Validate that weights are non-negative and sum to approximately 1.0
    pub fn validate(&self) -> Result<(), String> {
        let weights = [
            self.similarity_weight,
            self.confidence_weight,
            self.recency_weight,
            self.graph_distance_weight,
            self.source_count_weight,
        ];

        // Check all weights are non-negative
        for (i, &weight) in weights.iter().enumerate() {
            if weight < 0.0 {
                return Err(format!("Weight {} is negative: {}", i, weight));
            }
        }

        // Check weights sum to approximately 1.0 (allow small floating point errors)
        let sum: f64 = weights.iter().sum();
        if (sum - 1.0).abs() > 0.01 {
            return Err(format!("Weights sum to {}, expected ~1.0", sum));
        }

        Ok(())
    }

    /// Create a config that emphasizes confidence and corroboration (for verification queries)
    pub fn verification_mode() -> Self {
        Self {
            similarity_weight: 0.15,
            confidence_weight: 0.40,
            recency_weight: 0.15,
            graph_distance_weight: 0.10,
            source_count_weight: 0.20,
        }
    }

    /// Create a config that emphasizes semantic similarity (for exploration queries)
    pub fn exploration_mode() -> Self {
        Self {
            similarity_weight: 0.50,
            confidence_weight: 0.15,
            recency_weight: 0.10,
            graph_distance_weight: 0.15,
            source_count_weight: 0.10,
        }
    }
}

/// Retrieval signals for a single triple.
///
/// All signals are normalized to [0.0, 1.0] range before fusion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalSignals {
    /// The triple being scored
    #[schemars(skip)]
    pub triple_id: TripleId,
    /// Embedding cosine similarity to query (0.0 to 1.0)
    pub similarity: f64,
    /// Dynamic confidence from topology (0.0 to 1.0)
    pub confidence: f64,
    /// When this triple was last accessed
    pub last_accessed: DateTime<Utc>,
    /// Graph distance (hops) from query node
    pub graph_distance: u32,
    /// Number of provenance sources
    pub source_count: u32,
}

impl RetrievalSignals {
    /// Create new retrieval signals
    pub fn new(
        triple_id: TripleId,
        similarity: f64,
        confidence: f64,
        last_accessed: DateTime<Utc>,
        graph_distance: u32,
        source_count: u32,
    ) -> Self {
        Self {
            triple_id,
            similarity,
            confidence,
            last_accessed,
            graph_distance,
            source_count,
        }
    }
}

/// Multi-dimensional fusion scorer.
///
/// Combines multiple retrieval signals into a single relevance score using
/// weighted combination of normalized signals.
pub struct FusionScorer {
    config: FusionConfig,
}

impl FusionScorer {
    /// Create a new fusion scorer with the given configuration
    pub fn new(config: FusionConfig) -> Self {
        Self { config }
    }

    /// Create a scorer with default configuration
    pub fn default_config() -> Self {
        Self::new(FusionConfig::default())
    }

    /// Score a single triple using all available signals
    pub fn score(&self, signals: &RetrievalSignals) -> f64 {
        let similarity_score = self.normalize_similarity(signals.similarity);
        let confidence_score = signals.confidence; // Already normalized
        let recency_score = self.normalize_recency(signals.last_accessed);
        let graph_distance_score = self.normalize_graph_distance(signals.graph_distance);
        let source_count_score = self.normalize_source_count(signals.source_count);

        // Weighted combination
        self.config.similarity_weight * similarity_score
            + self.config.confidence_weight * confidence_score
            + self.config.recency_weight * recency_score
            + self.config.graph_distance_weight * graph_distance_score
            + self.config.source_count_weight * source_count_score
    }

    /// Score a batch of triples and return them sorted by score descending
    ///
    /// Returns tuples of (index_in_batch, score) sorted by score descending.
    pub fn score_batch(&self, batch: &[RetrievalSignals]) -> Vec<(usize, f64)> {
        let mut scored: Vec<(usize, f64)> = batch
            .iter()
            .enumerate()
            .map(|(idx, signals)| (idx, self.score(signals)))
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
    }

    /// Normalize similarity score (cosine similarity is already in [-1.0, 1.0], map to [0.0, 1.0])
    fn normalize_similarity(&self, similarity: f64) -> f64 {
        // Cosine similarity ranges from -1 to 1, map to 0 to 1
        ((similarity + 1.0) / 2.0).clamp(0.0, 1.0)
    }

    /// Normalize recency using exponential decay
    ///
    /// Recent accesses score higher. Uses half-life of 30 days.
    fn normalize_recency(&self, last_accessed: DateTime<Utc>) -> f64 {
        const HALF_LIFE_DAYS: i64 = 30;

        let now = Utc::now();
        let age = now.signed_duration_since(last_accessed);
        let age_days = age.num_days();

        if age_days < 0 {
            // Future timestamp (shouldn't happen, but handle gracefully)
            return 1.0;
        }

        // Exponential decay: score = 2^(-age / half_life)
        let decay_factor = -(age_days as f64) / (HALF_LIFE_DAYS as f64);
        2_f64.powf(decay_factor).clamp(0.0, 1.0)
    }

    /// Normalize graph distance (inverse of hop count)
    ///
    /// Closer nodes score higher. Max distance is 10 hops.
    fn normalize_graph_distance(&self, graph_distance: u32) -> f64 {
        const MAX_DISTANCE: u32 = 10;

        if graph_distance == 0 {
            return 1.0; // Direct connection
        }

        if graph_distance >= MAX_DISTANCE {
            return 0.0; // Too far
        }

        // Inverse linear: 1 - (distance / max_distance)
        1.0 - (graph_distance as f64 / MAX_DISTANCE as f64)
    }

    /// Normalize source count
    ///
    /// More sources = higher confidence. Max expected sources is 10.
    fn normalize_source_count(&self, source_count: u32) -> f64 {
        const MAX_SOURCES: u32 = 10;

        if source_count == 0 {
            return 0.0;
        }

        (source_count.min(MAX_SOURCES) as f64 / MAX_SOURCES as f64).clamp(0.0, 1.0)
    }

    /// Get the current config
    pub fn config(&self) -> &FusionConfig {
        &self.config
    }
}

// === Multi-Strategy Embedding Fusion ===

/// Configuration for blending multiple embedding strategies at query time.
///
/// Each query type emphasizes different strategies:
/// - Exploratory: global structure matters (spectral-heavy)
/// - Precise: recent structure matters (spring-heavy)
/// - Discovery: random walks find serendipity (node2vec-heavy)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct EmbeddingBlendConfig {
    /// Weight for spring embeddings (real-time, approximate)
    pub spring_weight: f64,
    /// Weight for node2vec embeddings (batch, neighborhood-aware)
    pub node2vec_weight: f64,
    /// Weight for spectral embeddings (periodic, global)
    pub spectral_weight: f64,
}

impl Default for EmbeddingBlendConfig {
    fn default() -> Self {
        // Default: balanced blend
        Self {
            spring_weight: 0.34,
            node2vec_weight: 0.33,
            spectral_weight: 0.33,
        }
    }
}

impl EmbeddingBlendConfig {
    /// Exploratory: "what's related to X?" — global structure matters
    pub fn exploratory() -> Self {
        Self {
            spring_weight: 0.2,
            node2vec_weight: 0.3,
            spectral_weight: 0.5,
        }
    }

    /// Precise: "what do I know about X?" — recent structure matters
    pub fn precise() -> Self {
        Self {
            spring_weight: 0.5,
            node2vec_weight: 0.3,
            spectral_weight: 0.2,
        }
    }

    /// Discovery: "surprise me" — random walks find serendipity
    pub fn discovery() -> Self {
        Self {
            spring_weight: 0.2,
            node2vec_weight: 0.5,
            spectral_weight: 0.3,
        }
    }

    /// Validate that weights are non-negative and sum to approximately 1.0
    pub fn validate(&self) -> Result<(), String> {
        let weights = [self.spring_weight, self.node2vec_weight, self.spectral_weight];

        for (i, &weight) in weights.iter().enumerate() {
            if weight < 0.0 {
                return Err(format!("Embedding blend weight {} is negative: {}", i, weight));
            }
        }

        let sum: f64 = weights.iter().sum();
        if (sum - 1.0).abs() > 0.01 {
            return Err(format!("Embedding blend weights sum to {}, expected ~1.0", sum));
        }

        Ok(())
    }
}

/// Similarity scores from each embedding strategy for a single node.
#[derive(Debug, Clone)]
pub struct StrategyScores {
    /// Cosine similarity from spring embeddings (None if unavailable)
    pub spring: Option<f64>,
    /// Cosine similarity from node2vec embeddings (None if unavailable)
    pub node2vec: Option<f64>,
    /// Cosine similarity from spectral embeddings (None if unavailable)
    pub spectral: Option<f64>,
}

impl StrategyScores {
    pub fn new(spring: Option<f64>, node2vec: Option<f64>, spectral: Option<f64>) -> Self {
        Self { spring, node2vec, spectral }
    }
}

/// Blends similarity scores from multiple embedding strategies into a single score.
pub struct EmbeddingBlender {
    config: EmbeddingBlendConfig,
}

impl EmbeddingBlender {
    pub fn new(config: EmbeddingBlendConfig) -> Self {
        Self { config }
    }

    /// Compute a blended similarity score from multiple strategy scores.
    ///
    /// If a strategy's score is None (embedding unavailable), its weight is
    /// redistributed proportionally to the available strategies.
    pub fn blend(&self, scores: &StrategyScores) -> f64 {
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;

        if let Some(s) = scores.spring {
            weighted_sum += self.config.spring_weight * s;
            total_weight += self.config.spring_weight;
        }
        if let Some(n) = scores.node2vec {
            weighted_sum += self.config.node2vec_weight * n;
            total_weight += self.config.node2vec_weight;
        }
        if let Some(sp) = scores.spectral {
            weighted_sum += self.config.spectral_weight * sp;
            total_weight += self.config.spectral_weight;
        }

        if total_weight == 0.0 {
            return 0.0;
        }

        // Normalize by available weight (redistribute missing weights proportionally)
        weighted_sum / total_weight
    }

    pub fn config(&self) -> &EmbeddingBlendConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_default_config() {
        let config = FusionConfig::default();
        assert_eq!(config.similarity_weight, 0.35);
        assert_eq!(config.confidence_weight, 0.25);
        assert_eq!(config.recency_weight, 0.20);
        assert_eq!(config.graph_distance_weight, 0.10);
        assert_eq!(config.source_count_weight, 0.10);

        // Weights should sum to 1.0
        let sum = config.similarity_weight
            + config.confidence_weight
            + config.recency_weight
            + config.graph_distance_weight
            + config.source_count_weight;
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = FusionConfig::default();
        assert!(valid_config.validate().is_ok());

        let invalid_negative = FusionConfig::new(0.5, -0.1, 0.3, 0.2, 0.1);
        assert!(invalid_negative.validate().is_err());

        let invalid_sum = FusionConfig::new(0.5, 0.5, 0.5, 0.5, 0.5);
        assert!(invalid_sum.validate().is_err());
    }

    #[test]
    fn test_verification_mode() {
        let config = FusionConfig::verification_mode();
        // Should emphasize confidence and source count
        assert!(config.confidence_weight > config.similarity_weight);
        assert!(config.source_count_weight > FusionConfig::default().source_count_weight);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_exploration_mode() {
        let config = FusionConfig::exploration_mode();
        // Should emphasize similarity
        assert!(config.similarity_weight > config.confidence_weight);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_normalize_similarity() {
        let scorer = FusionScorer::default_config();

        // Cosine similarity of 1.0 (identical) -> 1.0
        assert!((scorer.normalize_similarity(1.0) - 1.0).abs() < 0.001);

        // Cosine similarity of 0.0 (orthogonal) -> 0.5
        assert!((scorer.normalize_similarity(0.0) - 0.5).abs() < 0.001);

        // Cosine similarity of -1.0 (opposite) -> 0.0
        assert!((scorer.normalize_similarity(-1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_normalize_recency() {
        let scorer = FusionScorer::default_config();
        let now = Utc::now();

        // Just accessed -> 1.0
        assert!((scorer.normalize_recency(now) - 1.0).abs() < 0.001);

        // 30 days ago (one half-life) -> 0.5
        let thirty_days_ago = now - Duration::days(30);
        let score = scorer.normalize_recency(thirty_days_ago);
        assert!((score - 0.5).abs() < 0.01);

        // 60 days ago (two half-lives) -> 0.25
        let sixty_days_ago = now - Duration::days(60);
        let score = scorer.normalize_recency(sixty_days_ago);
        assert!((score - 0.25).abs() < 0.01);

        // Very old -> near 0
        let very_old = now - Duration::days(365);
        assert!(scorer.normalize_recency(very_old) < 0.1);
    }

    #[test]
    fn test_normalize_graph_distance() {
        let scorer = FusionScorer::default_config();

        // Direct connection (0 hops) -> 1.0
        assert_eq!(scorer.normalize_graph_distance(0), 1.0);

        // 5 hops (mid-range) -> 0.5
        assert!((scorer.normalize_graph_distance(5) - 0.5).abs() < 0.001);

        // Max distance (10 hops) or more -> 0.0
        assert_eq!(scorer.normalize_graph_distance(10), 0.0);
        assert_eq!(scorer.normalize_graph_distance(100), 0.0);
    }

    #[test]
    fn test_normalize_source_count() {
        let scorer = FusionScorer::default_config();

        // No sources -> 0.0
        assert_eq!(scorer.normalize_source_count(0), 0.0);

        // 1 source -> 0.1
        assert!((scorer.normalize_source_count(1) - 0.1).abs() < 0.001);

        // 5 sources -> 0.5
        assert!((scorer.normalize_source_count(5) - 0.5).abs() < 0.001);

        // 10 sources (max) -> 1.0
        assert_eq!(scorer.normalize_source_count(10), 1.0);

        // More than max still caps at 1.0
        assert_eq!(scorer.normalize_source_count(20), 1.0);
    }

    #[test]
    fn test_score_basic() {
        let scorer = FusionScorer::default_config();
        let now = Utc::now();

        let signals = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.8,
            confidence: 0.9,
            last_accessed: now,
            graph_distance: 1,
            source_count: 5,
        };

        let score = scorer.score(&signals);

        // Score should be in valid range
        assert!(score >= 0.0 && score <= 1.0);

        // With high signals, should be relatively high
        assert!(score > 0.6);
    }

    #[test]
    fn test_score_zero_signals() {
        let scorer = FusionScorer::default_config();
        let very_old = Utc::now() - Duration::days(365);

        let signals = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: -1.0, // Opposite direction
            confidence: 0.0,
            last_accessed: very_old,
            graph_distance: 100,
            source_count: 0,
        };

        let score = scorer.score(&signals);

        // Should be very low but not crash
        assert!(score >= 0.0);
        assert!(score < 0.2);
    }

    #[test]
    fn test_high_confidence_beats_high_similarity() {
        // With default weights, this may not be true, but with custom weights it should be
        let config = FusionConfig::new(
            0.2, // Low similarity weight
            0.6, // High confidence weight
            0.1,
            0.05,
            0.05,
        );
        let scorer = FusionScorer::new(config);
        let now = Utc::now();

        let high_confidence = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.3, // Low similarity
            confidence: 0.95, // High confidence
            last_accessed: now,
            graph_distance: 2,
            source_count: 8,
        };

        let high_similarity = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.95, // High similarity
            confidence: 0.3, // Low confidence
            last_accessed: now,
            graph_distance: 2,
            source_count: 2,
        };

        let score_conf = scorer.score(&high_confidence);
        let score_sim = scorer.score(&high_similarity);

        // High confidence should win with these weights
        assert!(score_conf > score_sim);
    }

    #[test]
    fn test_recency_boost() {
        let scorer = FusionScorer::default_config();
        let now = Utc::now();
        let old = now - Duration::days(90);

        let recent = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.7,
            confidence: 0.7,
            last_accessed: now,
            graph_distance: 3,
            source_count: 3,
        };

        let stale = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.7,
            confidence: 0.7,
            last_accessed: old,
            graph_distance: 3,
            source_count: 3,
        };

        let score_recent = scorer.score(&recent);
        let score_stale = scorer.score(&stale);

        // Recent should score higher
        assert!(score_recent > score_stale);
    }

    #[test]
    fn test_score_batch() {
        let scorer = FusionScorer::default_config();
        let now = Utc::now();

        let signals_batch = vec![
            RetrievalSignals {
                triple_id: uuid::Uuid::new_v4(),
                similarity: 0.5,
                confidence: 0.5,
                last_accessed: now,
                graph_distance: 5,
                source_count: 2,
            },
            RetrievalSignals {
                triple_id: uuid::Uuid::new_v4(),
                similarity: 0.9,
                confidence: 0.9,
                last_accessed: now,
                graph_distance: 1,
                source_count: 8,
            },
            RetrievalSignals {
                triple_id: uuid::Uuid::new_v4(),
                similarity: 0.2,
                confidence: 0.3,
                last_accessed: now - Duration::days(60),
                graph_distance: 8,
                source_count: 1,
            },
        ];

        let ranked = scorer.score_batch(&signals_batch);

        // Should return 3 results
        assert_eq!(ranked.len(), 3);

        // Should be sorted by score descending
        for i in 1..ranked.len() {
            assert!(ranked[i - 1].1 >= ranked[i].1);
        }

        // Highest scoring should be index 1 (all high signals)
        assert_eq!(ranked[0].0, 1);

        // Lowest scoring should be index 2 (all low signals)
        assert_eq!(ranked[2].0, 2);
    }

    #[test]
    fn test_score_batch_empty() {
        let scorer = FusionScorer::default_config();
        let empty: Vec<RetrievalSignals> = vec![];

        let ranked = scorer.score_batch(&empty);

        assert_eq!(ranked.len(), 0);
    }

    #[test]
    fn test_custom_weights() {
        // Custom config that heavily weights graph distance
        let config = FusionConfig::new(0.1, 0.1, 0.1, 0.6, 0.1);
        let scorer = FusionScorer::new(config);
        let now = Utc::now();

        let close = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.3,
            confidence: 0.3,
            last_accessed: now,
            graph_distance: 0, // Very close
            source_count: 2,
        };

        let distant = RetrievalSignals {
            triple_id: uuid::Uuid::new_v4(),
            similarity: 0.9,
            confidence: 0.9,
            last_accessed: now,
            graph_distance: 9, // Very far
            source_count: 8,
        };

        let score_close = scorer.score(&close);
        let score_distant = scorer.score(&distant);

        // With heavy graph distance weighting, close should win despite other signals
        assert!(score_close > score_distant);
    }

    // === Embedding Blend Tests ===

    #[test]
    fn test_embedding_blend_default() {
        let config = EmbeddingBlendConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_embedding_blend_presets() {
        let exploratory = EmbeddingBlendConfig::exploratory();
        assert!(exploratory.validate().is_ok());
        assert!(exploratory.spectral_weight > exploratory.spring_weight);

        let precise = EmbeddingBlendConfig::precise();
        assert!(precise.validate().is_ok());
        assert!(precise.spring_weight > precise.spectral_weight);

        let discovery = EmbeddingBlendConfig::discovery();
        assert!(discovery.validate().is_ok());
        assert!(discovery.node2vec_weight > discovery.spring_weight);
    }

    #[test]
    fn test_embedding_blend_validation() {
        let bad = EmbeddingBlendConfig {
            spring_weight: -0.1,
            node2vec_weight: 0.6,
            spectral_weight: 0.5,
        };
        assert!(bad.validate().is_err());

        let bad_sum = EmbeddingBlendConfig {
            spring_weight: 0.5,
            node2vec_weight: 0.5,
            spectral_weight: 0.5,
        };
        assert!(bad_sum.validate().is_err());
    }

    #[test]
    fn test_blender_all_strategies() {
        let blender = EmbeddingBlender::new(EmbeddingBlendConfig::precise());

        let scores = StrategyScores::new(Some(0.9), Some(0.7), Some(0.5));
        let blended = blender.blend(&scores);

        // precise: spring=0.5, n2v=0.3, spectral=0.2
        // expected: 0.5*0.9 + 0.3*0.7 + 0.2*0.5 = 0.45 + 0.21 + 0.10 = 0.76
        assert!((blended - 0.76).abs() < 0.01, "Expected ~0.76, got {}", blended);
    }

    #[test]
    fn test_blender_missing_strategy() {
        let blender = EmbeddingBlender::new(EmbeddingBlendConfig {
            spring_weight: 0.5,
            node2vec_weight: 0.3,
            spectral_weight: 0.2,
        });

        // Only spring available
        let scores = StrategyScores::new(Some(0.8), None, None);
        let blended = blender.blend(&scores);

        // Should redistribute: only spring weight matters, normalized to 1.0
        // Result = 0.8 (since it's the only signal)
        assert!((blended - 0.8).abs() < 0.01, "Expected ~0.8, got {}", blended);
    }

    #[test]
    fn test_blender_no_strategies() {
        let blender = EmbeddingBlender::new(EmbeddingBlendConfig::default());

        let scores = StrategyScores::new(None, None, None);
        let blended = blender.blend(&scores);

        assert_eq!(blended, 0.0);
    }

    #[test]
    fn test_blender_two_strategies() {
        let blender = EmbeddingBlender::new(EmbeddingBlendConfig {
            spring_weight: 0.5,
            node2vec_weight: 0.3,
            spectral_weight: 0.2,
        });

        // Spring + spectral, no node2vec
        let scores = StrategyScores::new(Some(0.9), None, Some(0.5));

        let blended = blender.blend(&scores);

        // Available weight = 0.5 + 0.2 = 0.7
        // Weighted sum = 0.5*0.9 + 0.2*0.5 = 0.45 + 0.10 = 0.55
        // Normalized = 0.55 / 0.7 = 0.7857...
        assert!((blended - 0.7857).abs() < 0.01, "Expected ~0.786, got {}", blended);
    }

    #[test]
    fn test_blender_exploratory_prefers_spectral() {
        let blender = EmbeddingBlender::new(EmbeddingBlendConfig::exploratory());

        // Spectral high, spring low
        let spectral_wins = StrategyScores::new(Some(0.3), Some(0.5), Some(0.9));
        // Spectral low, spring high
        let spring_wins = StrategyScores::new(Some(0.9), Some(0.5), Some(0.3));

        let score_spectral = blender.blend(&spectral_wins);
        let score_spring = blender.blend(&spring_wins);

        // With exploratory weights (spectral=0.5, n2v=0.3, spring=0.2),
        // the high-spectral case should score higher
        assert!(
            score_spectral > score_spring,
            "Exploratory should prefer high-spectral: spectral_blend={:.3}, spring_blend={:.3}",
            score_spectral, score_spring
        );
    }
}
