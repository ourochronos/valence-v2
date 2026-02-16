use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::SourceType;
use crate::query::FusionConfig;

/// Request to insert one or more triples
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "mcp", derive(JsonSchema))]
pub struct InsertTriplesRequest {
    pub triples: Vec<TripleInput>,
    pub source: Option<SourceInput>,
}

/// A triple to be inserted (string values, not IDs)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TripleInput {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

/// Source information for inserted triples
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourceInput {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub reference: Option<String>,
}

/// Response from inserting triples
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InsertTriplesResponse {
    pub triple_ids: Vec<String>,
    pub source_id: Option<String>,
}

/// Query parameters for searching triples
#[derive(Debug, Deserialize)]
pub struct QueryTriplesParams {
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<String>,
    pub limit: Option<usize>,
    pub include_sources: Option<bool>,
}

/// Response from querying triples
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct QueryTriplesResponse {
    pub triples: Vec<TripleResponse>,
}

/// A triple in the response
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TripleResponse {
    pub id: String,
    pub subject: NodeResponse,
    pub predicate: String,
    pub object: NodeResponse,
    pub weight: f64,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    pub sources: Option<Vec<SourceResponse>>,
}

/// A node in the response
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeResponse {
    pub id: String,
    pub value: String,
}

/// A source in the response
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourceResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub reference: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for neighbors endpoint
#[derive(Debug, Deserialize)]
pub struct NeighborsParams {
    pub depth: Option<u32>,
    pub limit: Option<usize>,
}

/// Response from neighbors endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NeighborsResponse {
    pub triples: Vec<TripleResponse>,
    pub node_count: usize,
    pub triple_count: usize,
}

/// Response from sources endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourcesResponse {
    pub sources: Vec<SourceResponse>,
}

/// Response from stats endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StatsResponse {
    pub triple_count: u64,
    pub node_count: u64,
    pub avg_weight: f64,
}

/// Request to trigger decay
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecayRequest {
    pub factor: f64,
    pub min_weight: f64,
}

/// Response from decay endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecayResponse {
    pub affected_count: u64,
}

/// Request to evict low-weight triples
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvictRequest {
    pub threshold: f64,
}

/// Response from evict endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvictResponse {
    pub evicted_count: u64,
}

/// Request to search for similar nodes
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Node value to search for
    pub query_node: String,
    /// Number of results to return
    #[serde(default = "default_k")]
    pub k: usize,
    /// Whether to include dynamic confidence scores
    #[serde(default)]
    pub include_confidence: bool,
    /// Optional: use tiered retrieval with budget constraints
    #[serde(default)]
    pub use_tiered: bool,
    /// Optional: budget in milliseconds (for tiered retrieval)
    pub budget_ms: Option<u64>,
    /// Optional: confidence threshold (0.0-1.0) for early stopping
    pub confidence_threshold: Option<f64>,
    /// Optional: fusion scoring configuration (uses default if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub fusion_config: Option<FusionConfig>,
}

fn default_k() -> usize {
    10
}

/// A single search result
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub node_id: String,
    pub value: String,
    pub similarity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// Response from search endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    /// Optional: tier reached in tiered retrieval (1-3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier_reached: Option<u8>,
    /// Optional: time taken in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<u64>,
    /// Optional: whether budget was exhausted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_exhausted: Option<bool>,
    /// Optional: whether fallback mode was used (no embeddings available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<bool>,
}

/// Request to recompute embeddings
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecomputeEmbeddingsRequest {
    /// Number of dimensions for the embeddings
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

fn default_dimensions() -> usize {
    64
}

/// Response from recompute-embeddings endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecomputeEmbeddingsResponse {
    pub embedding_count: usize,
}

/// Response from stigmergy reinforcement endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReinforceResponse {
    /// Number of co-retrieval edges created
    pub edges_created: u64,
}

/// Request to assemble context for a query
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContextRequest {
    /// Query string to build context around
    pub query: String,
    /// Maximum number of triples to include
    #[serde(default = "default_max_triples")]
    pub max_triples: usize,
    /// Output format
    #[serde(default = "default_format")]
    pub format: String,
    /// Optional: fusion scoring configuration (uses default if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub fusion_config: Option<FusionConfig>,
}

fn default_max_triples() -> usize {
    50
}

fn default_format() -> String {
    "markdown".to_string()
}

/// Response from context assembly endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContextResponse {
    /// Formatted context string
    pub context: String,
    /// Number of triples included
    pub triple_count: usize,
    /// Number of nodes included
    pub node_count: usize,
    /// Total relevance score
    pub total_relevance: f64,
}

/// Request to recompute Node2Vec embeddings
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecomputeNode2VecRequest {
    /// Number of dimensions for the embeddings
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
    
    /// Length of each random walk
    #[serde(default = "default_walk_length")]
    pub walk_length: usize,
    
    /// Number of walks to start from each node
    #[serde(default = "default_walks_per_node")]
    pub walks_per_node: usize,
    
    /// Return parameter (controls likelihood of returning to previous node)
    #[serde(default = "default_p")]
    pub p: f64,
    
    /// In-out parameter (controls breadth vs depth)
    #[serde(default = "default_q")]
    pub q: f64,
    
    /// Context window size for skip-gram
    #[serde(default = "default_window")]
    pub window: usize,
    
    /// Number of training epochs
    #[serde(default = "default_epochs")]
    pub epochs: usize,
    
    /// Learning rate
    #[serde(default = "default_learning_rate")]
    pub learning_rate: f64,
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

/// Response from recompute-node2vec endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecomputeNode2VecResponse {
    pub embedding_count: usize,
}

/// Request to run a full lifecycle cycle
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleRequest {
    /// Optional: override the default decay policy
    pub policy: Option<DecayPolicyInput>,
    /// Optional: override the default memory bounds
    pub bounds: Option<MemoryBoundsInput>,
}

/// Decay policy configuration
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecayPolicyInput {
    /// Base decay factor per cycle (0.0-1.0, default 0.95)
    #[serde(default = "default_base_factor")]
    pub base_factor: f64,
    
    /// Weight boost on access (default 0.1)
    #[serde(default = "default_access_boost")]
    pub access_boost: f64,
    
    /// Extra weight per source (default 0.05)
    #[serde(default = "default_source_protection")]
    pub source_protection: f64,
    
    /// Extra weight for central triples (default 0.1)
    #[serde(default = "default_centrality_protection")]
    pub centrality_protection: f64,
    
    /// Floor before eviction (default 0.01)
    #[serde(default = "default_min_weight")]
    pub min_weight: f64,
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

/// Memory bounds configuration
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryBoundsInput {
    /// Hard cap on triple count
    #[serde(default = "default_max_triples_memory")]
    pub max_triples: usize,
    
    /// Hard cap on node count
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
    
    /// Target utilization (0.0-1.0, default 0.8)
    #[serde(default = "default_target_utilization")]
    pub target_utilization: f64,
}

fn default_max_triples_memory() -> usize {
    10_000
}

fn default_max_nodes() -> usize {
    5_000
}

fn default_target_utilization() -> f64 {
    0.8
}

/// Response from lifecycle endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleResponse {
    /// Decay cycle result
    pub decay: DecayCycleResponse,
    /// Bounds enforcement result
    pub bounds: EnforceResponse,
}

/// Decay cycle result
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecayCycleResponse {
    /// Number of triples that had decay applied
    pub triples_decayed: u64,
    
    /// Number of triples evicted (below min_weight)
    pub triples_evicted: u64,
    
    /// Total weight before decay
    pub total_weight_before: f64,
    
    /// Total weight after decay
    pub total_weight_after: f64,
}

/// Bounds enforcement result
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EnforceResponse {
    /// Number of triples evicted
    pub triples_evicted: u64,
    
    /// Number of nodes removed
    pub nodes_removed: u64,
    
    /// Final triple count
    pub final_triple_count: u64,
    
    /// Final node count
    pub final_node_count: u64,
    
    /// Whether target was reached
    pub target_reached: bool,
}

/// Response from lifecycle status endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleStatusResponse {
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
