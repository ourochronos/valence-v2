//! Unified configuration for the Valence Engine.
//!
//! This module provides a single `EngineConfig` that encompasses all subsystem configurations,
//! with support for loading from TOML files and environment variables.

use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::{Context, Result};

use crate::tiered_store::{PromotionPolicy, DemotionPolicy};

/// Storage backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StorageConfig {
    /// In-memory storage (ephemeral, no persistence)
    Memory,
    
    /// PostgreSQL storage (persistent)
    #[cfg(feature = "postgres")]
    Postgres {
        /// Database connection URL
        #[serde(default = "default_database_url")]
        url: String,
    },
    
    /// Tiered storage (hot memory tier + cold persistent tier)
    #[cfg(feature = "postgres")]
    Tiered {
        /// Database connection URL for cold tier
        #[serde(default = "default_database_url")]
        database_url: String,
        
        /// Maximum triples in hot tier (0 = unlimited)
        #[serde(default = "default_hot_capacity")]
        hot_capacity: usize,
        
        /// Promotion policy
        #[serde(default)]
        promotion_policy: PromotionPolicyConfig,
        
        /// Demotion policy
        #[serde(default)]
        demotion_policy: DemotionPolicyConfig,
        
        /// Demotion sweep interval in seconds
        #[serde(default = "default_demotion_interval")]
        demotion_interval_secs: u64,
        
        /// Track access patterns
        #[serde(default = "default_true")]
        track_accesses: bool,
    },
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::Memory
    }
}

#[cfg(feature = "postgres")]
fn default_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/valence".to_string())
}

#[cfg(feature = "postgres")]
fn default_hot_capacity() -> usize {
    10_000
}

#[cfg(feature = "postgres")]
fn default_demotion_interval() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

/// Promotion policy configuration (serializable version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PromotionPolicyConfig {
    /// Promote after N accesses
    AccessThreshold {
        #[serde(default = "default_min_accesses")]
        min_accesses: u64,
    },
    /// Promote based on frequency
    FrequencyThreshold {
        #[serde(default = "default_min_frequency")]
        min_frequency: f64,
    },
    /// Promote immediately on access
    Immediate,
}

impl Default for PromotionPolicyConfig {
    fn default() -> Self {
        Self::AccessThreshold { min_accesses: 3 }
    }
}

impl From<PromotionPolicyConfig> for PromotionPolicy {
    fn from(config: PromotionPolicyConfig) -> Self {
        match config {
            PromotionPolicyConfig::AccessThreshold { min_accesses } => {
                PromotionPolicy::AccessThreshold { min_accesses }
            }
            PromotionPolicyConfig::FrequencyThreshold { min_frequency } => {
                PromotionPolicy::FrequencyThreshold { min_frequency }
            }
            PromotionPolicyConfig::Immediate => PromotionPolicy::Immediate,
        }
    }
}

fn default_min_accesses() -> u64 {
    3
}

fn default_min_frequency() -> f64 {
    1.0
}

/// Demotion policy configuration (serializable version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DemotionPolicyConfig {
    /// Demote after idle timeout
    IdleTimeout {
        #[serde(default = "default_idle_hours")]
        hours: i64,
    },
    /// LRU eviction
    LeastRecentlyUsed,
    /// Never demote
    Never,
}

impl Default for DemotionPolicyConfig {
    fn default() -> Self {
        Self::LeastRecentlyUsed
    }
}

impl From<DemotionPolicyConfig> for DemotionPolicy {
    fn from(config: DemotionPolicyConfig) -> Self {
        match config {
            DemotionPolicyConfig::IdleTimeout { hours } => DemotionPolicy::IdleTimeout { hours },
            DemotionPolicyConfig::LeastRecentlyUsed => DemotionPolicy::LeastRecentlyUsed,
            DemotionPolicyConfig::Never => DemotionPolicy::Never,
        }
    }
}

fn default_idle_hours() -> i64 {
    24
}

/// Embeddings configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    /// Spectral embedding settings
    #[serde(default)]
    pub spectral: SpectralConfig,
    
    /// Node2Vec embedding settings
    #[serde(default)]
    pub node2vec: Node2VecConfig,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            spectral: SpectralConfig::default(),
            node2vec: Node2VecConfig::default(),
        }
    }
}

/// Spectral embedding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectralConfig {
    /// Number of dimensions
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,
    
    /// Normalize the Laplacian
    #[serde(default = "default_true")]
    pub normalize: bool,
}

impl Default for SpectralConfig {
    fn default() -> Self {
        Self {
            dimensions: 64,
            normalize: true,
        }
    }
}

fn default_embedding_dimensions() -> usize {
    64
}

/// Node2Vec embedding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node2VecConfig {
    /// Number of dimensions
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,
    
    /// Walk length
    #[serde(default = "default_walk_length")]
    pub walk_length: usize,
    
    /// Walks per node
    #[serde(default = "default_walks_per_node")]
    pub walks_per_node: usize,
    
    /// Return parameter
    #[serde(default = "default_p")]
    pub p: f64,
    
    /// In-out parameter
    #[serde(default = "default_q")]
    pub q: f64,
    
    /// Context window size
    #[serde(default = "default_window")]
    pub window: usize,
    
    /// Training epochs
    #[serde(default = "default_epochs")]
    pub epochs: usize,
    
    /// Learning rate
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,
}

impl Default for Node2VecConfig {
    fn default() -> Self {
        Self {
            dimensions: 64,
            walk_length: 80,
            walks_per_node: 10,
            p: 1.0,
            q: 1.0,
            window: 5,
            epochs: 5,
            learning_rate: 0.025,
        }
    }
}

fn default_walk_length() -> usize {
    80
}

fn default_walks_per_node() -> usize {
    10
}

fn default_p() -> f64 {
    1.0
}

fn default_q() -> f64 {
    1.0
}

fn default_window() -> usize {
    5
}

fn default_epochs() -> usize {
    5
}

fn default_learning_rate() -> f64 {
    0.025
}

/// Stigmergy (access tracking and co-retrieval) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StigmergyConfig {
    /// Access tracker settings
    #[serde(default)]
    pub access_tracker: AccessTrackerConfig,
    
    /// Co-retrieval engine settings
    #[serde(default)]
    pub co_retrieval: CoRetrievalConfig,
}

impl Default for StigmergyConfig {
    fn default() -> Self {
        Self {
            access_tracker: AccessTrackerConfig::default(),
            co_retrieval: CoRetrievalConfig::default(),
        }
    }
}

/// Access tracker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessTrackerConfig {
    /// Maximum access events in sliding window
    #[serde(default = "default_window_size")]
    pub window_size: usize,
    
    /// Access event decay time in hours
    #[serde(default = "default_decay_hours")]
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

fn default_window_size() -> usize {
    10_000
}

fn default_decay_hours() -> i64 {
    24
}

/// Co-retrieval configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoRetrievalConfig {
    /// Minimum co-access count before creating edge
    #[serde(default = "default_threshold")]
    pub threshold: u64,
    
    /// Predicate for co-retrieval edges
    #[serde(default = "default_predicate")]
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

fn default_threshold() -> u64 {
    3
}

fn default_predicate() -> String {
    "co_retrieved_with".to_string()
}

/// Lifecycle (decay and bounds) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    /// Decay policy
    #[serde(default)]
    pub decay: DecayConfig,
    
    /// Memory bounds
    #[serde(default)]
    pub bounds: BoundsConfig,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            decay: DecayConfig::default(),
            bounds: BoundsConfig::default(),
        }
    }
}

/// Decay policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayConfig {
    /// Base decay factor per cycle (0.0-1.0)
    #[serde(default = "default_base_factor")]
    pub base_factor: f64,
    
    /// Weight boost on access
    #[serde(default = "default_access_boost")]
    pub access_boost: f64,
    
    /// Extra weight per source
    #[serde(default = "default_source_protection")]
    pub source_protection: f64,
    
    /// Extra weight for central triples
    #[serde(default = "default_centrality_protection")]
    pub centrality_protection: f64,
    
    /// Floor before eviction
    #[serde(default = "default_min_weight")]
    pub min_weight: f64,
}

impl Default for DecayConfig {
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

fn default_base_factor() -> f64 {
    0.95
}

fn default_access_boost() -> f64 {
    0.1
}

fn default_source_protection() -> f64 {
    0.05
}

fn default_centrality_protection() -> f64 {
    0.1
}

fn default_min_weight() -> f64 {
    0.01
}

/// Memory bounds configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundsConfig {
    /// Maximum triples
    #[serde(default = "default_max_triples")]
    pub max_triples: usize,
    
    /// Maximum nodes
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
    
    /// Target utilization (0.0-1.0)
    #[serde(default = "default_target_utilization")]
    pub target_utilization: f64,
}

impl Default for BoundsConfig {
    fn default() -> Self {
        Self {
            max_triples: 10_000,
            max_nodes: 5_000,
            target_utilization: 0.8,
        }
    }
}

fn default_max_triples() -> usize {
    10_000
}

fn default_max_nodes() -> usize {
    5_000
}

fn default_target_utilization() -> f64 {
    0.8
}

/// Inference loop configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Feedback recorder settings
    #[serde(default)]
    pub feedback: FeedbackConfig,
    
    /// Weight adjuster settings
    #[serde(default)]
    pub weight_adjuster: WeightAdjusterConfig,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            feedback: FeedbackConfig::default(),
            weight_adjuster: WeightAdjusterConfig::default(),
        }
    }
}

/// Feedback recorder configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackConfig {
    /// Maximum feedback events to retain
    #[serde(default = "default_max_events")]
    pub max_events: usize,
    
    /// Minimum feedback count before adjustment
    #[serde(default = "default_min_feedback_count")]
    pub min_feedback_count: usize,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            max_events: 10_000,
            min_feedback_count: 10,
        }
    }
}

fn default_max_events() -> usize {
    10_000
}

fn default_min_feedback_count() -> usize {
    10
}

/// Weight adjuster configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightAdjusterConfig {
    /// Learning rate for weight adjustments
    #[serde(default = "default_adjustment_learning_rate")]
    pub learning_rate: f64,
    
    /// Minimum weight value
    #[serde(default = "default_min_weight_value")]
    pub min_weight: f64,
    
    /// Maximum weight value
    #[serde(default = "default_max_weight_value")]
    pub max_weight: f64,
}

impl Default for WeightAdjusterConfig {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            min_weight: 0.0,
            max_weight: 1.0,
        }
    }
}

fn default_adjustment_learning_rate() -> f64 {
    0.01
}

fn default_min_weight_value() -> f64 {
    0.0
}

fn default_max_weight_value() -> f64 {
    1.0
}

/// Budget defaults configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Default time budget in milliseconds
    #[serde(default = "default_time_budget_ms")]
    pub time_budget_ms: u64,
    
    /// Default hop budget
    #[serde(default = "default_hop_budget")]
    pub hop_budget: u32,
    
    /// Default result budget
    #[serde(default = "default_result_budget")]
    pub result_budget: usize,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            time_budget_ms: 1000,
            hop_budget: 5,
            result_budget: 100,
        }
    }
}

fn default_time_budget_ms() -> u64 {
    1000
}

fn default_hop_budget() -> u32 {
    5
}

fn default_result_budget() -> usize {
    100
}

/// Query fusion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    /// Fusion scoring weights
    #[serde(default)]
    pub fusion: FusionConfig,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            fusion: FusionConfig::default(),
        }
    }
}

/// Fusion scoring configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionConfig {
    /// Weight for embedding similarity
    #[serde(default = "default_similarity_weight")]
    pub similarity_weight: f64,
    
    /// Weight for dynamic confidence
    #[serde(default = "default_confidence_weight")]
    pub confidence_weight: f64,
    
    /// Weight for recency
    #[serde(default = "default_recency_weight")]
    pub recency_weight: f64,
    
    /// Weight for graph distance
    #[serde(default = "default_graph_distance_weight")]
    pub graph_distance_weight: f64,
    
    /// Weight for source count
    #[serde(default = "default_source_count_weight")]
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

fn default_similarity_weight() -> f64 {
    0.35
}

fn default_confidence_weight() -> f64 {
    0.25
}

fn default_recency_weight() -> f64 {
    0.20
}

fn default_graph_distance_weight() -> f64 {
    0.10
}

fn default_source_count_weight() -> f64 {
    0.10
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server mode (http, mcp, or both)
    #[serde(default = "default_mode")]
    pub mode: String,
    
    /// Host to bind to (for HTTP mode)
    #[serde(default = "default_host")]
    pub host: String,
    
    /// Port to bind to (for HTTP mode)
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            mode: "http".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8421,
        }
    }
}

fn default_mode() -> String {
    "http".to_string()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8421
}

/// Unified Valence Engine configuration.
///
/// This encompasses all subsystem configurations and can be loaded from
/// TOML files or environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Storage backend configuration
    #[serde(default)]
    pub storage: StorageConfig,
    
    /// Embeddings configuration
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    
    /// Stigmergy configuration
    #[serde(default)]
    pub stigmergy: StigmergyConfig,
    
    /// Lifecycle configuration
    #[serde(default)]
    pub lifecycle: LifecycleConfig,
    
    /// Inference configuration
    #[serde(default)]
    pub inference: InferenceConfig,
    
    /// Budget defaults
    #[serde(default)]
    pub budget: BudgetConfig,
    
    /// Query configuration
    #[serde(default)]
    pub query: QueryConfig,
    
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            embeddings: EmbeddingsConfig::default(),
            stigmergy: StigmergyConfig::default(),
            lifecycle: LifecycleConfig::default(),
            inference: InferenceConfig::default(),
            budget: BudgetConfig::default(),
            query: QueryConfig::default(),
            server: ServerConfig::default(),
        }
    }
}

impl EngineConfig {
    /// Load configuration from a TOML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;
        
        let config: EngineConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path.as_ref()))?;
        
        Ok(config)
    }
    
    /// Load configuration from environment variables.
    ///
    /// Environment variables override config file values:
    /// - VALENCE_MODE -> server.mode
    /// - VALENCE_HOST -> server.host
    /// - VALENCE_PORT -> server.port
    /// - DATABASE_URL -> storage (postgres or tiered)
    pub fn from_env(mut self) -> Self {
        // Server config from env
        if let Ok(mode) = std::env::var("VALENCE_MODE") {
            self.server.mode = mode;
        }
        if let Ok(host) = std::env::var("VALENCE_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("VALENCE_PORT") {
            if let Ok(port_num) = port.parse() {
                self.server.port = port_num;
            }
        }
        
        // Database URL from env (only if not already postgres/tiered)
        #[cfg(feature = "postgres")]
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            match &mut self.storage {
                StorageConfig::Postgres { url } => {
                    *url = database_url;
                }
                StorageConfig::Tiered { database_url: db_url, .. } => {
                    *db_url = database_url;
                }
                StorageConfig::Memory => {
                    // Don't override memory config with env var
                }
            }
        }
        
        self
    }
    
    /// Load configuration with precedence: file -> env -> defaults.
    ///
    /// If config_path is provided, load from file and apply env overrides.
    /// Otherwise, use defaults with env overrides.
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let config = if let Some(path) = config_path {
            Self::from_file(path)?
        } else {
            Self::default()
        };
        
        Ok(config.from_env())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_default_config() {
        let config = EngineConfig::default();
        assert_eq!(config.server.port, 8421);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.budget.hop_budget, 5);
    }
    
    #[test]
    fn test_config_from_toml() -> Result<()> {
        let toml_content = r#"
[server]
mode = "both"
host = "0.0.0.0"
port = 9000

[budget]
time_budget_ms = 2000
hop_budget = 10

[embeddings.spectral]
dimensions = 128
"#;
        
        let mut file = NamedTempFile::new()?;
        file.write_all(toml_content.as_bytes())?;
        
        let config = EngineConfig::from_file(file.path())?;
        assert_eq!(config.server.mode, "both");
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.budget.time_budget_ms, 2000);
        assert_eq!(config.budget.hop_budget, 10);
        assert_eq!(config.embeddings.spectral.dimensions, 128);
        
        Ok(())
    }
    
    #[test]
    fn test_env_override() {
        std::env::set_var("VALENCE_MODE", "mcp");
        std::env::set_var("VALENCE_PORT", "7777");
        
        let config = EngineConfig::default().from_env();
        assert_eq!(config.server.mode, "mcp");
        assert_eq!(config.server.port, 7777);
        
        std::env::remove_var("VALENCE_MODE");
        std::env::remove_var("VALENCE_PORT");
    }
}
