use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::SourceType;

/// Request to insert one or more triples
#[derive(Debug, Serialize, Deserialize)]
pub struct InsertTriplesRequest {
    pub triples: Vec<TripleInput>,
    pub source: Option<SourceInput>,
}

/// A triple to be inserted (string values, not IDs)
#[derive(Debug, Serialize, Deserialize)]
pub struct TripleInput {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

/// Source information for inserted triples
#[derive(Debug, Serialize, Deserialize)]
pub struct SourceInput {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub reference: Option<String>,
}

/// Response from inserting triples
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryTriplesResponse {
    pub triples: Vec<TripleResponse>,
}

/// A triple in the response
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeResponse {
    pub id: String,
    pub value: String,
}

/// A source in the response
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
pub struct NeighborsResponse {
    pub triples: Vec<TripleResponse>,
    pub node_count: usize,
    pub triple_count: usize,
}

/// Response from sources endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct SourcesResponse {
    pub sources: Vec<SourceResponse>,
}

/// Response from stats endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct StatsResponse {
    pub triple_count: u64,
    pub node_count: u64,
    pub avg_weight: f64,
}

/// Request to trigger decay
#[derive(Debug, Serialize, Deserialize)]
pub struct DecayRequest {
    pub factor: f64,
    pub min_weight: f64,
}

/// Response from decay endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct DecayResponse {
    pub affected_count: u64,
}

/// Request to evict low-weight triples
#[derive(Debug, Serialize, Deserialize)]
pub struct EvictRequest {
    pub threshold: f64,
}

/// Response from evict endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct EvictResponse {
    pub evicted_count: u64,
}

/// Request to search for similar nodes
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    /// Node value to search for
    pub query_node: String,
    /// Number of results to return
    #[serde(default = "default_k")]
    pub k: usize,
    /// Whether to include dynamic confidence scores
    #[serde(default)]
    pub include_confidence: bool,
}

fn default_k() -> usize {
    10
}

/// A single search result
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub node_id: String,
    pub value: String,
    pub similarity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// Response from search endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

/// Request to recompute embeddings
#[derive(Debug, Serialize, Deserialize)]
pub struct RecomputeEmbeddingsRequest {
    /// Number of dimensions for the embeddings
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

fn default_dimensions() -> usize {
    64
}

/// Response from recompute-embeddings endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct RecomputeEmbeddingsResponse {
    pub embedding_count: usize,
}
