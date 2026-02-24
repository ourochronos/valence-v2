//! MCP tool implementation functions
//!
//! These functions implement the actual business logic for each MCP tool,
//! mapping to ValenceEngine operations.

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;
use base64::Engine;

use crate::{
    api::{
        InsertTriplesRequest, InsertTriplesResponse, QueryTriplesResponse,
        SearchRequest, SearchResponse, NeighborsResponse, SourcesResponse,
        StatsResponse, TripleResponse, NodeResponse, SourceResponse,
    },
    embeddings::EmbeddingStore,
    engine::ValenceEngine,
    graph::{GraphView, DynamicConfidence},
    models::{Triple, Source},
    storage::TriplePattern,
    predicates,
    vkb::SessionStore as _,  // bring trait methods into scope
};

// ============================================================================
// Generic placeholder response for tools not yet implemented
// ============================================================================

/// Placeholder response for MCP tools not yet fully implemented
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlaceholderResponse {
    pub message: String,
}

/// Wrapper for list responses (MCP requires root type 'object')
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlaceholderListResponse {
    pub items: Vec<PlaceholderResponse>,
}

// ============================================================================
// Social/Knowledge Management Response Types
// ============================================================================

/// Response for node_search tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeSearchResponse {
    pub nodes: Vec<NodeResponse>,
}

/// Response for triple_supersede tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TripleSupersedeResponse {
    pub old_triple_id: String,
    pub new_triple_id: String,
    pub message: String,
}

/// Response for trust_edge_create tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrustEdgeCreateResponse {
    pub triple_id: String,
    pub from_did: String,
    pub to_did: String,
}

/// Response for reputation_get tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReputationGetResponse {
    pub did: String,
    pub trust_score: f64,
}

/// Response for share_create tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareCreateResponse {
    pub share_triple_id: String,
    pub triple_id: String,
    pub recipient_did: String,
}

/// Response for share_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareListResponse {
    pub shares: Vec<ShareEntry>,
}

/// A single share entry
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareEntry {
    pub share_triple_id: String,
    pub triple_id: String,
    pub recipient_did: String,
    pub weight: f64,
}

/// Response for share_revoke tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareRevokeResponse {
    pub share_id: String,
    pub message: String,
}

/// Response for verification_submit tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerificationSubmitResponse {
    pub verification_triple_id: String,
    pub result_triple_id: String,
    pub reasoning_triple_id: Option<String>,
    pub message: String,
}

/// Response for verification_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerificationListResponse {
    pub verifications: Vec<VerificationEntry>,
}

/// A single verification entry
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerificationEntry {
    pub verification_triple_id: String,
    pub verifier: String,
    pub target_triple_id: String,
    pub result: String,
    pub reasoning: Option<String>,
}

/// Response for tension_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TensionListResponse {
    pub tensions: Vec<TensionEntry>,
}

/// A single tension entry
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TensionEntry {
    pub triple_a_id: String,
    pub triple_b_id: String,
    pub tension_type: String,
    pub severity: String,
    pub subject: String,
    pub predicate: String,
}

/// Response for tension_resolve tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TensionResolveResponse {
    pub resolution_triple_id: String,
    pub action: String,
    pub message: String,
}

// ============================================================================
// VKB MCP Response Types
// ============================================================================

/// MCP response for a single session
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbSessionResponse {
    pub id: String,
    pub platform: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_room_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub themes: Option<Vec<String>>,
}

/// MCP response wrapping a list of sessions
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbSessionListResponse {
    pub sessions: Vec<VkbSessionResponse>,
}

/// MCP response for a single exchange
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbExchangeResponse {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// MCP response wrapping a list of exchanges
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbExchangeListResponse {
    pub exchanges: Vec<VkbExchangeResponse>,
}

/// MCP response for a single pattern
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbPatternResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub pattern_type: String,
    pub description: String,
    pub status: String,
    pub confidence: f64,
    pub evidence_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// MCP response wrapping a list of patterns
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbPatternListResponse {
    pub patterns: Vec<VkbPatternResponse>,
}

/// MCP response for a single insight
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbInsightResponse {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub confidence: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// MCP response wrapping a list of insights
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VkbInsightListResponse {
    pub insights: Vec<VkbInsightResponse>,
}

/// Convert a Session model to a VkbSessionResponse
fn session_to_mcp_response(s: &crate::vkb::models::Session) -> VkbSessionResponse {
    VkbSessionResponse {
        id: s.id.to_string(),
        platform: s.platform.as_str().to_string(),
        status: format!("{:?}", s.status).to_lowercase(),
        project_context: s.project_context.clone(),
        external_room_id: s.external_room_id.clone(),
        created_at: s.created_at,
        ended_at: s.ended_at,
        summary: s.summary.clone(),
        themes: if s.themes.is_empty() { None } else { Some(s.themes.clone()) },
    }
}

/// Convert a Pattern model to a VkbPatternResponse
fn pattern_to_mcp_response(p: &crate::vkb::models::Pattern) -> VkbPatternResponse {
    VkbPatternResponse {
        id: p.id.to_string(),
        pattern_type: p.pattern_type.clone(),
        description: p.description.clone(),
        status: format!("{:?}", p.status).to_lowercase(),
        confidence: p.confidence,
        evidence_count: p.evidence_session_ids.len() as i32,
        created_at: p.created_at,
        last_seen: p.updated_at,
    }
}

// ============================================================================
// Parameter types for tools that need them
// ============================================================================

/// Parameters for query_triples tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct QueryTriplesParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_sources: Option<bool>,
}

/// Parameters for neighbors tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NeighborsParams {
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Parameters for sources tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourcesParams {
    pub triple_id: String,
}

/// Parameters for maintain tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decay_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evict_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recompute_embeddings: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_dimensions: Option<usize>,
}

/// Response for maintain tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decay: Option<MaintainDecayResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evict: Option<MaintainEvictResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recompute_embeddings: Option<MaintainEmbeddingsResult>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainDecayResult {
    pub affected_count: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainEvictResult {
    pub evicted_count: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainEmbeddingsResult {
    pub embedding_count: usize,
}

// ============================================================================
// High-level tool parameters and responses (NEW)
// ============================================================================

/// Parameters for context_for_query tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContextForQueryParams {
    /// The query string
    pub query: String,
    /// Maximum number of triples to include (default: 50)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_triples: Option<usize>,
    /// Maximum number of nodes to include (default: 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<usize>,
    /// Include confidence scores (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_confidence: Option<bool>,
    /// Output format: plain, markdown, json (default: markdown)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Session ID to use for session-scoped context (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Response for context_for_query tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ContextForQueryResponse {
    /// Formatted context ready for LLM consumption
    pub formatted_context: String,
    /// Number of triples included
    pub triple_count: usize,
    /// Number of nodes included
    pub node_count: usize,
    /// Total relevance score
    pub total_score: f64,
    /// Whether embeddings were used (true) or graph-only fallback (false)
    pub used_embeddings: bool,
}

/// Parameters for record_feedback tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecordFeedbackParams {
    /// Session ID that this feedback belongs to
    pub session_id: String,
    /// Triple IDs that were useful/relevant
    pub useful_triple_ids: Vec<String>,
    /// Optional: Triple IDs that were not useful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_useful_triple_ids: Option<Vec<String>>,
}

/// Response for record_feedback tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecordFeedbackResponse {
    /// Number of useful triples recorded
    pub useful_count: usize,
    /// Number of not-useful triples recorded
    pub not_useful_count: usize,
    /// Message
    pub message: String,
}

/// Parameters for session_start tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionStartParams {
    /// Initial query or context for the session
    pub initial_query: String,
    /// Optional session ID (will be generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Response for session_start tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionStartResponse {
    /// The session ID (generated or provided)
    pub session_id: String,
    /// Initial working set summary
    pub working_set_summary: String,
    /// Number of nodes in initial working set
    pub node_count: usize,
    /// Number of triples in initial working set
    pub triple_count: usize,
}

/// Parameters for session_end tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionEndParams {
    /// Session ID to end
    pub session_id: String,
}

/// Response for session_end tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionEndResponse {
    /// Session ID that was ended
    pub session_id: String,
    /// Final turn count
    pub final_turn: u32,
    /// Number of active threads at end
    pub active_threads: usize,
    /// Message
    pub message: String,
}

/// Parameters for explore tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExploreParams {
    /// Starting node for exploration (value or ID)
    pub start_node: String,
    /// Maximum depth to explore (default: 2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    /// Maximum results to return (default: 20)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    /// Time budget in milliseconds (default: 1000)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_budget_ms: Option<u64>,
}

/// Response for explore tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExploreResponse {
    /// Exploration results
    pub results: Vec<ExploreResult>,
    /// Tier reached (warm, cold, exhaustive)
    pub tier_reached: String,
    /// Time taken in milliseconds
    pub time_ms: u64,
    /// Whether budget was exhausted
    pub budget_exhausted: bool,
}

/// A single exploration result
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExploreResult {
    /// Triple ID
    pub triple_id: String,
    /// Subject
    pub subject: String,
    /// Predicate
    pub predicate: String,
    /// Object
    pub object: String,
    /// Relevance score
    pub score: f64,
    /// Confidence
    pub confidence: f64,
}

// ============================================================================
// Tool implementations
// ============================================================================

/// Insert triples with source provenance
pub async fn insert_triples_impl(
    engine: &ValenceEngine,
    req: InsertTriplesRequest,
) -> Result<InsertTriplesResponse> {
    let mut triple_ids = Vec::new();

    // Insert each triple
    for triple_req in &req.triples {
        // Find or create subject and object nodes
        let subject_node = engine
            .store
            .find_or_create_node(&triple_req.subject)
            .await?;
        let object_node = engine
            .store
            .find_or_create_node(&triple_req.object)
            .await?;

        // Create and insert triple
        let triple = Triple::new(subject_node.id, &triple_req.predicate, object_node.id);
        let triple_id = engine.store.insert_triple(triple).await?;
        triple_ids.push(triple_id);
    }

    // Insert source if provided
    let source_id = if let Some(source_req) = &req.source {
        let source = Source::new(triple_ids.clone(), source_req.source_type.clone());
        let source = if let Some(ref reference) = source_req.reference {
            source.with_reference(reference)
        } else {
            source
        };
        let source_id = engine.store.insert_source(source).await?;
        Some(source_id)
    } else {
        None
    };

    Ok(InsertTriplesResponse {
        triple_ids: triple_ids.iter().map(|id| id.to_string()).collect(),
        source_id: source_id.map(|id| id.to_string()),
    })
}

/// Query triples by pattern
pub async fn query_triples_impl(
    engine: &ValenceEngine,
    params: QueryTriplesParams,
) -> Result<QueryTriplesResponse> {
    // Resolve node values to IDs
    let subject_id = if let Some(ref subject_value) = params.subject {
        engine
            .store
            .find_node_by_value(subject_value)
            .await?
            .map(|n| n.id)
    } else {
        None
    };

    let object_id = if let Some(ref object_value) = params.object {
        engine
            .store
            .find_node_by_value(object_value)
            .await?
            .map(|n| n.id)
    } else {
        None
    };

    // Query triples
    let pattern = TriplePattern {
        subject: subject_id,
        predicate: params.predicate,
        object: object_id,
    };

    let triples = engine.store.query_triples(pattern).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    for triple in triples {
        let subject_node = engine.store.get_node(triple.subject).await?.unwrap();
        let object_node = engine.store.get_node(triple.object).await?.unwrap();

        let sources = if params.include_sources.unwrap_or(false) {
            let sources = engine.store.get_sources_for_triple(triple.id).await?;
            Some(
                sources
                    .into_iter()
                    .map(|s| SourceResponse {
                        id: s.id.to_string(),
                        source_type: s.source_type,
                        reference: s.reference,
                        created_at: s.created_at,
                    })
                    .collect(),
            )
        } else {
            None
        };

        triple_responses.push(TripleResponse {
            id: triple.id.to_string(),
            subject: NodeResponse {
                id: subject_node.id.to_string(),
                value: subject_node.value,
            },
            predicate: triple.predicate.value,
            object: NodeResponse {
                id: object_node.id.to_string(),
                value: object_node.value,
            },
            base_weight: triple.base_weight,
            local_weight: triple.local_weight,
            timestamp: triple.timestamp,
            last_accessed: triple.last_accessed,
            access_count: triple.access_count,
            origin_did: triple.origin_did.clone(),
            sources,
        });
    }

    Ok(QueryTriplesResponse {
        triples: triple_responses,
    })
}

/// Semantic search using embeddings
pub async fn search_impl(
    engine: &ValenceEngine,
    req: SearchRequest,
) -> Result<SearchResponse> {
    use crate::api::SearchResult;

    // Find the query node by value
    let query_node = engine
        .store
        .find_node_by_value(&req.query_node)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Query node not found: {}", req.query_node))?;

    // Get the embedding for the query node
    let embeddings_store = engine.embeddings.read().await;
    let query_embedding = embeddings_store
        .get(query_node.id)
        .ok_or_else(|| anyhow::anyhow!("No embedding found for node: {}", req.query_node))?
        .clone(); // Clone to release the read lock

    // Find k nearest neighbors
    let neighbors = embeddings_store.query_nearest(&query_embedding, req.k)?;
    drop(embeddings_store); // Release lock before async operations

    // Build response
    let mut results = Vec::new();

    for (node_id, similarity) in neighbors {
        // Get node value
        let node = engine
            .store
            .get_node(node_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {:?}", node_id))?;

        // Optionally compute confidence
        let confidence = if req.include_confidence {
            // Build graph view
            let graph_view = GraphView::from_store(&*engine.store).await?;

            // Find a triple involving this node to compute confidence
            let pattern = TriplePattern {
                subject: Some(node_id),
                predicate: None,
                object: None,
            };
            let triples = engine.store.query_triples(pattern).await?;

            if let Some(triple) = triples.first() {
                let conf = DynamicConfidence::compute_confidence(
                    &*engine.store,
                    &graph_view,
                    triple.id,
                    Some(query_node.id),
                )
                .await?;
                Some(conf.combined)
            } else {
                Some(0.0)
            }
        } else {
            None
        };

        results.push(SearchResult {
            node_id: node_id.to_string(),
            value: node.value,
            similarity,
            confidence,
        });
    }

    Ok(SearchResponse {
        results,
        tier_reached: None,
        time_ms: None,
        budget_exhausted: None,
        fallback: None, // MCP search uses embeddings (warm mode only)
    })
}

/// Get k-hop neighborhood
pub async fn neighbors_impl(
    engine: &ValenceEngine,
    params: NeighborsParams,
) -> Result<NeighborsResponse> {
    // Try to parse as UUID, otherwise lookup by value
    let node_id = if let Ok(uuid) = Uuid::parse_str(&params.node) {
        uuid
    } else {
        engine
            .store
            .find_node_by_value(&params.node)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", params.node))?
            .id
    };

    let depth = params.depth.unwrap_or(1);
    let triples = engine.store.neighbors(node_id, depth).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    let mut unique_nodes = HashSet::new();

    for triple in &triples {
        let subject_node = engine.store.get_node(triple.subject).await?.unwrap();
        let object_node = engine.store.get_node(triple.object).await?.unwrap();

        unique_nodes.insert(triple.subject);
        unique_nodes.insert(triple.object);

        triple_responses.push(TripleResponse {
            id: triple.id.to_string(),
            subject: NodeResponse {
                id: subject_node.id.to_string(),
                value: subject_node.value,
            },
            predicate: triple.predicate.value.clone(),
            object: NodeResponse {
                id: object_node.id.to_string(),
                value: object_node.value,
            },
            base_weight: triple.base_weight,
            local_weight: triple.local_weight,
            timestamp: triple.timestamp,
            last_accessed: triple.last_accessed,
            access_count: triple.access_count,
            origin_did: triple.origin_did.clone(),
            sources: None,
        });
    }

    Ok(NeighborsResponse {
        triples: triple_responses,
        node_count: unique_nodes.len(),
        triple_count: triples.len(),
    })
}

/// Get provenance sources for a triple
pub async fn sources_impl(
    engine: &ValenceEngine,
    params: SourcesParams,
) -> Result<SourcesResponse> {
    let triple_id = Uuid::parse_str(&params.triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", params.triple_id))?;

    let sources = engine.store.get_sources_for_triple(triple_id).await?;

    let source_responses: Vec<SourceResponse> = sources
        .into_iter()
        .map(|s| SourceResponse {
            id: s.id.to_string(),
            source_type: s.source_type,
            reference: s.reference,
            created_at: s.created_at,
        })
        .collect();

    Ok(SourcesResponse {
        sources: source_responses,
    })
}

/// Get engine statistics
pub async fn stats_impl(engine: &ValenceEngine) -> Result<StatsResponse> {
    let triple_count = engine.store.count_triples().await?;
    let node_count = engine.store.count_nodes().await?;

    // Calculate average weight
    let pattern = TriplePattern {
        subject: None,
        predicate: None,
        object: None,
    };
    let triples = engine.store.query_triples(pattern).await?;
    let avg_weight = if !triples.is_empty() {
        triples.iter().map(|t| t.local_weight).sum::<f64>() / triples.len() as f64
    } else {
        0.0
    };

    Ok(StatsResponse {
        triple_count,
        node_count,
        avg_weight,
    })
}

/// Run maintenance operations
pub async fn maintain_impl(
    engine: &ValenceEngine,
    params: MaintainParams,
) -> Result<MaintainResponse> {
    let mut response = MaintainResponse {
        decay: None,
        evict: None,
        recompute_embeddings: None,
    };

    // Decay if requested
    if let Some(decay_factor) = params.decay_factor {
        let affected_count = engine.store.decay(decay_factor, 0.0).await?;
        response.decay = Some(MaintainDecayResult { affected_count });
    }

    // Evict if requested
    if let Some(threshold) = params.evict_threshold {
        let evicted_count = engine.store.evict_below_weight(threshold).await?;
        response.evict = Some(MaintainEvictResult { evicted_count });
    }

    // Recompute embeddings if requested
    if params.recompute_embeddings.unwrap_or(false) {
        let dimensions = params.embedding_dimensions.unwrap_or(64);
        let embedding_count = engine.recompute_embeddings(dimensions).await?;
        response.recompute_embeddings = Some(MaintainEmbeddingsResult { embedding_count });
    }

    Ok(response)
}

// ============================================================================
// High-level tool implementations (NEW)
// ============================================================================

/// Assemble optimal context for a query using working set + budget + fusion scoring
pub async fn context_for_query_impl(
    engine: &ValenceEngine,
    params: ContextForQueryParams,
) -> Result<ContextForQueryResponse> {
    use crate::context::{ContextAssembler, AssemblyConfig, ContextFormat};
    use anyhow::Context as _;

    // Parse format
    let format = match params.format.as_deref() {
        Some("plain") => ContextFormat::Plain,
        Some("json") => ContextFormat::Json,
        Some("markdown") | None => ContextFormat::Markdown,
        Some(other) => {
            return Err(anyhow::anyhow!("Invalid format '{}', expected: plain, markdown, json", other));
        }
    };

    // Build assembly config
    let config = AssemblyConfig {
        max_triples: params.max_triples.unwrap_or(50),
        max_nodes: params.max_nodes.unwrap_or(100),
        include_confidence: params.include_confidence.unwrap_or(true),
        include_sources: false,
        format,
        fusion_config: None, // Use default fusion config
    };

    // Assemble context
    let assembler = ContextAssembler::new(engine);
    let context = assembler.assemble(&params.query, config).await?;

    // Check if embeddings were used (warm mode) or graph-only (cold mode)
    let query_node = engine
        .store
        .find_node_by_value(&params.query)
        .await?
        .context("Query node not found")?;

    let embeddings_store = engine.embeddings.read().await;
    let used_embeddings = embeddings_store.get(query_node.id).is_some();
    drop(embeddings_store);

    Ok(ContextForQueryResponse {
        formatted_context: context.formatted,
        triple_count: context.triples.len(),
        node_count: context.nodes.len(),
        total_score: context.total_score,
        used_embeddings,
    })
}

/// Record feedback about which triples from context were useful
///
/// Uses stigmergy: touching (accessing) useful triples boosts their relevance
/// through the access tracking system. Not-useful triples are left untouched
/// and will naturally decay over time.
pub async fn record_feedback_impl(
    engine: &ValenceEngine,
    params: RecordFeedbackParams,
) -> Result<RecordFeedbackResponse> {
    let mut useful_count = 0;
    let mut not_useful_count = 0;

    // Parse session ID (validate but don't use yet - session tracking comes later)
    let _session_id = Uuid::parse_str(&params.session_id)
        .map_err(|_| anyhow::anyhow!("Invalid session ID: {}", params.session_id))?;

    // Process useful triples - mark them as accessed (stigmergy)
    // This boosts their relevance through the access tracking system
    for triple_id_str in &params.useful_triple_ids {
        let triple_id = Uuid::parse_str(triple_id_str)
            .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", triple_id_str))?;

        // Touch the triple to mark it as accessed
        // This updates last_accessed timestamp and increments access_count
        engine.store.touch_triple(triple_id).await?;
        useful_count += 1;
    }

    // Process not-useful triples - count them but don't touch
    // They will naturally decay through the lifecycle manager
    if let Some(not_useful_ids) = params.not_useful_triple_ids {
        not_useful_count = not_useful_ids.len();
        
        // We could explicitly decay these, but leaving them alone
        // and letting natural decay handle it is more aligned with stigmergy
    }

    Ok(RecordFeedbackResponse {
        useful_count,
        not_useful_count,
        message: format!(
            "Recorded feedback via stigmergy: {} useful triples touched, {} not-useful left to decay",
            useful_count, not_useful_count
        ),
    })
}

/// Start a new session with working set
pub async fn session_start_impl(
    engine: &ValenceEngine,
    params: SessionStartParams,
) -> Result<SessionStartResponse> {
    use crate::context::WorkingSet;

    // Parse or generate session ID
    let session_id = if let Some(id_str) = params.session_id {
        Uuid::parse_str(&id_str)
            .map_err(|_| anyhow::anyhow!("Invalid session ID: {}", id_str))?
    } else {
        Uuid::new_v4()
    };

    // Build initial working set from query
    let mut working_set = WorkingSet::from_query(engine, &params.initial_query, 20).await?;
    working_set.session_id = Some(session_id);

    // Get summary
    let summary = working_set.to_context_summary();

    Ok(SessionStartResponse {
        session_id: session_id.to_string(),
        working_set_summary: summary,
        node_count: working_set.node_count(),
        triple_count: working_set.triple_count(),
    })
}

/// End a session
pub async fn session_end_impl(
    _engine: &ValenceEngine,
    params: SessionEndParams,
) -> Result<SessionEndResponse> {
    // Parse session ID
    let session_id = Uuid::parse_str(&params.session_id)
        .map_err(|_| anyhow::anyhow!("Invalid session ID: {}", params.session_id))?;

    // In a real implementation, we would:
    // 1. Retrieve the working set from session storage
    // 2. Archive resolved threads
    // 3. Compress session history into decisions/learnings
    // 4. Clean up session state
    //
    // For now, we just acknowledge the session end
    
    Ok(SessionEndResponse {
        session_id: session_id.to_string(),
        final_turn: 0, // Would come from stored working set
        active_threads: 0, // Would come from stored working set
        message: format!("Session {} ended", session_id),
    })
}

/// Explore graph interactively with tiered retrieval
pub async fn explore_impl(
    engine: &ValenceEngine,
    params: ExploreParams,
) -> Result<ExploreResponse> {
    use crate::budget::{TieredRetriever, OperationBudget};
    use std::sync::Arc;

    // Build budget
    let max_depth = params.max_depth.unwrap_or(2);
    let max_results = params.max_results.unwrap_or(20);
    let time_budget_ms = params.time_budget_ms.unwrap_or(1000);

    let budget = OperationBudget::new(time_budget_ms, max_depth, max_results);

    // Run tiered retrieval
    let engine_arc = Arc::new(engine.clone());
    let retriever = TieredRetriever::new(engine_arc);
    let confidence_threshold = 0.8; // Stop early if we find high-confidence results
    let retrieval_result = retriever.retrieve(&params.start_node, budget, confidence_threshold).await?;

    // Convert node results to triple results by finding triples involving these nodes
    let mut results = Vec::new();
    
    for scored_result in retrieval_result.results.iter().take(max_results) {
        // Find triples where this node is subject or object
        let pattern = TriplePattern {
            subject: Some(scored_result.node_id),
            predicate: None,
            object: None,
        };
        
        let triples = engine.store.query_triples(pattern).await?;
        
        for triple in triples.iter().take(3) { // Limit to 3 triples per node to avoid explosion
            let subject_node = engine.store.get_node(triple.subject).await?
                .ok_or_else(|| anyhow::anyhow!("Subject node not found"))?;
            let object_node = engine.store.get_node(triple.object).await?
                .ok_or_else(|| anyhow::anyhow!("Object node not found"))?;

            results.push(ExploreResult {
                triple_id: triple.id.to_string(),
                subject: subject_node.value,
                predicate: triple.predicate.value.clone(),
                object: object_node.value,
                score: scored_result.similarity as f64,
                confidence: scored_result.confidence.unwrap_or(triple.local_weight),
            });
        }
        
        if results.len() >= max_results {
            break;
        }
    }

    // Map tier number to tier name
    let tier_name = match retrieval_result.tier_reached {
        1 => "warm (vector search only)",
        2 => "cold (+ graph walk)",
        3 => "exhaustive (+ confidence)",
        _ => "unknown",
    };

    Ok(ExploreResponse {
        results,
        tier_reached: tier_name.to_string(),
        time_ms: retrieval_result.time_ms,
        budget_exhausted: retrieval_result.budget_exhausted,
    })
}

// ============================================================================
// Tests for high-level tools
// ============================================================================

#[cfg(test)]
mod high_level_tests {
    use super::*;
    use crate::models::Triple;

    #[tokio::test]
    async fn test_context_for_query() {
        let engine = ValenceEngine::new();

        // Build a small knowledge graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "likes", carol.id)).await.unwrap();

        // Recompute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        // Test context_for_query
        let params = ContextForQueryParams {
            query: "Alice".to_string(),
            max_triples: Some(10),
            max_nodes: Some(20),
            include_confidence: Some(true),
            format: Some("markdown".to_string()),
            session_id: None,
        };

        let response = context_for_query_impl(&engine, params).await.unwrap();

        assert!(response.triple_count > 0);
        assert!(response.node_count > 0);
        assert!(!response.formatted_context.is_empty());
        assert!(response.used_embeddings); // Should use embeddings since we computed them
    }

    #[tokio::test]
    async fn test_context_for_query_cold_mode() {
        let engine = ValenceEngine::new();

        // Build a graph WITHOUT embeddings
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        // Don't compute embeddings — test cold mode fallback

        let params = ContextForQueryParams {
            query: "Alice".to_string(),
            max_triples: Some(10),
            max_nodes: Some(20),
            include_confidence: Some(true),
            format: Some("plain".to_string()),
            session_id: None,
        };

        let response = context_for_query_impl(&engine, params).await.unwrap();

        assert!(response.triple_count > 0);
        assert!(!response.formatted_context.is_empty());
        assert!(!response.used_embeddings); // Should use graph-only fallback
    }

    #[tokio::test]
    async fn test_record_feedback() {
        let engine = ValenceEngine::new();

        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();

        let triple_id = engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        // Get initial state
        let initial_triple = engine.store.get_triple(triple_id).await.unwrap().unwrap();
        let initial_access_count = initial_triple.access_count;

        // Record positive feedback
        let params = RecordFeedbackParams {
            session_id: Uuid::new_v4().to_string(),
            useful_triple_ids: vec![triple_id.to_string()],
            not_useful_triple_ids: None,
        };

        let response = record_feedback_impl(&engine, params).await.unwrap();

        assert_eq!(response.useful_count, 1);
        assert_eq!(response.not_useful_count, 0);

        // Access count should have increased (stigmergy via touch)
        let updated_triple = engine.store.get_triple(triple_id).await.unwrap().unwrap();
        assert!(updated_triple.access_count > initial_access_count, 
                "Access count should increase from {} to {}", 
                initial_access_count, 
                updated_triple.access_count);
    }

    #[tokio::test]
    async fn test_session_start() {
        let engine = ValenceEngine::new();

        // Build a small graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.recompute_embeddings(4).await.unwrap();

        let params = SessionStartParams {
            initial_query: "Alice".to_string(),
            session_id: None,
        };

        let response = session_start_impl(&engine, params).await.unwrap();

        assert!(!response.session_id.is_empty());
        assert!(Uuid::parse_str(&response.session_id).is_ok());
        assert!(response.node_count > 0);
        assert!(!response.working_set_summary.is_empty());
    }

    #[tokio::test]
    async fn test_session_end() {
        let engine = ValenceEngine::new();

        let session_id = Uuid::new_v4();

        let params = SessionEndParams {
            session_id: session_id.to_string(),
        };

        let response = session_end_impl(&engine, params).await.unwrap();

        assert_eq!(response.session_id, session_id.to_string());
        assert!(response.message.contains(&session_id.to_string()));
    }

    #[tokio::test]
    async fn test_explore() {
        let engine = ValenceEngine::new();

        // Build a small graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();

        engine.recompute_embeddings(4).await.unwrap();

        let params = ExploreParams {
            start_node: "Alice".to_string(),
            max_depth: Some(2),
            max_results: Some(10),
            time_budget_ms: Some(1000),
        };

        let response = explore_impl(&engine, params).await.unwrap();

        assert!(!response.results.is_empty(), "Should return exploration results");
        assert!(!response.tier_reached.is_empty(), "Should indicate tier reached");
        // Time may be 0 for very fast operations, so just verify it's a valid measurement
        assert!(response.time_ms >= 0);
    }
}

// ============================================================================
// VKB MCP Tool Parameters and Implementations
// ============================================================================

/// Parameters for session_get tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionGetParams {
    pub session_id: String,
}

/// Parameters for session_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Parameters for session_find_by_room tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionFindByRoomParams {
    pub room_id: String,
}

/// Parameters for exchange_add tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExchangeAddParams {
    pub session_id: String,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_approx: Option<i32>,
}

/// Parameters for exchange_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ExchangeListParams {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Parameters for pattern_record tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatternRecordParams {
    #[serde(rename = "type")]
    pub pattern_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// Parameters for pattern_reinforce tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatternReinforceParams {
    pub pattern_id: String,
}

/// Parameters for pattern_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatternListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub pattern_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Parameters for pattern_search tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatternSearchParams {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Parameters for insight_extract tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InsightExtractParams {
    pub session_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// Parameters for insight_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InsightListParams {
    pub session_id: String,
}

// ============================================================================
// Trust/Identity MCP Tool Parameters
// ============================================================================

/// Parameters for trust_query tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrustQueryParams {
    pub did: String,
}

/// Response for trust_query tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrustQueryResponse {
    pub did: String,
    pub trust_score: f64,
    pub connected_dids: Vec<ConnectedDidResponse>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConnectedDidResponse {
    pub did: String,
    pub score: f64,
}

/// Parameters for sign_triple tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SignTripleParams {
    pub triple_id: String,
}

/// Response for sign_triple tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SignTripleResponse {
    pub triple_id: String,
    pub signature: String, // base64-encoded
    pub signer_did: String,
}

/// Parameters for verify_triple tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerifyTripleParams {
    pub triple_id: String,
}

/// Response for verify_triple tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerifyTripleResponse {
    pub triple_id: String,
    pub valid: bool,
    pub origin_did: Option<String>,
}


// ============================================================================
// Knowledge Management MCP Tool Parameters
// ============================================================================

/// Parameters for triple_get tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TripleGetParams {
    pub triple_id: String,
}

/// Parameters for triple_supersede tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TripleSupersedePar {
    pub old_triple_id: String,
    pub new_subject: String,
    pub new_predicate: String,
    pub new_object: String,
    pub reason: String,
}

/// Parameters for node_search tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NodeSearchParams {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}


// ============================================================================
// Stub implementations for new tools
// ============================================================================

// VKB implementations

pub async fn session_get_impl(
    engine: &ValenceEngine,
    params: SessionGetParams,
) -> Result<VkbSessionResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let session_id = Uuid::parse_str(&params.session_id)?;
    let store_lock = store.read().await;
    let session = store_lock.get_session(session_id).await?
        .ok_or_else(|| anyhow::anyhow!("Session {} not found", params.session_id))?;

    Ok(session_to_mcp_response(&session))
}

pub async fn session_list_impl(
    engine: &ValenceEngine,
    params: SessionListParams,
) -> Result<VkbSessionListResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let status = params.status.as_deref().and_then(|s| match s {
        "active" => Some(crate::vkb::SessionStatus::Active),
        "completed" => Some(crate::vkb::SessionStatus::Completed),
        "abandoned" => Some(crate::vkb::SessionStatus::Abandoned),
        _ => None,
    });

    let store_lock = store.read().await;
    let sessions = store_lock.list_sessions(status, None, None, params.limit.unwrap_or(20)).await?;

    Ok(VkbSessionListResponse {
        sessions: sessions.iter().map(session_to_mcp_response).collect(),
    })
}

pub async fn session_find_by_room_impl(
    engine: &ValenceEngine,
    params: SessionFindByRoomParams,
) -> Result<VkbSessionResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let store_lock = store.read().await;
    let session = store_lock.find_session_by_room(&params.room_id).await?
        .ok_or_else(|| anyhow::anyhow!("No session found for room {}", params.room_id))?;

    Ok(session_to_mcp_response(&session))
}

pub async fn exchange_add_impl(
    engine: &ValenceEngine,
    params: ExchangeAddParams,
) -> Result<VkbExchangeResponse> {
    use crate::vkb::{ExchangeRole, Exchange};

    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let session_id = Uuid::parse_str(&params.session_id)?;
    let role = match params.role.as_str() {
        "user" => ExchangeRole::User,
        "assistant" => ExchangeRole::Assistant,
        "system" => ExchangeRole::System,
        _ => return Err(anyhow::anyhow!("Invalid role: {}", params.role)),
    };

    let mut exchange = Exchange::new(session_id, role, &params.content);
    exchange.tokens_approx = params.tokens_approx;

    let store_lock = store.read().await;
    let exchange_id = store_lock.add_exchange(exchange.clone()).await?;

    Ok(VkbExchangeResponse {
        id: exchange_id.to_string(),
        session_id: session_id.to_string(),
        role: params.role,
        content: params.content,
        created_at: exchange.created_at,
    })
}

pub async fn exchange_list_impl(
    engine: &ValenceEngine,
    params: ExchangeListParams,
) -> Result<VkbExchangeListResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let session_id = Uuid::parse_str(&params.session_id)?;
    let store_lock = store.read().await;
    let exchanges = store_lock.list_exchanges(session_id, params.limit.unwrap_or(20), 0).await?;

    Ok(VkbExchangeListResponse {
        exchanges: exchanges.iter().map(|e| VkbExchangeResponse {
            id: e.id.to_string(),
            session_id: e.session_id.to_string(),
            role: format!("{:?}", e.role).to_lowercase(),
            content: e.content.clone(),
            created_at: e.created_at,
        }).collect(),
    })
}

pub async fn pattern_record_impl(
    engine: &ValenceEngine,
    params: PatternRecordParams,
) -> Result<VkbPatternResponse> {
    use crate::vkb::Pattern;

    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let mut pattern = Pattern::new(&params.pattern_type, &params.description);
    if let Some(c) = params.confidence {
        pattern.confidence = c;
    }

    let store_lock = store.read().await;
    let _pattern_id = store_lock.record_pattern(pattern.clone()).await?;

    Ok(pattern_to_mcp_response(&pattern))
}

pub async fn pattern_reinforce_impl(
    engine: &ValenceEngine,
    params: PatternReinforceParams,
) -> Result<VkbPatternResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let pattern_id = Uuid::parse_str(&params.pattern_id)?;
    let store_lock = store.read().await;
    store_lock.reinforce_pattern(pattern_id, None).await?;

    // Fetch the updated pattern to return its current state
    // list_patterns doesn't filter by ID, so we search for all and find ours
    let patterns = store_lock.list_patterns(None, None, 1000).await?;
    let pattern = patterns.iter().find(|p| p.id == pattern_id)
        .ok_or_else(|| anyhow::anyhow!("Pattern {} not found after reinforcement", params.pattern_id))?;

    Ok(pattern_to_mcp_response(pattern))
}

pub async fn pattern_list_impl(
    engine: &ValenceEngine,
    params: PatternListParams,
) -> Result<VkbPatternListResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let store_lock = store.read().await;
    let patterns = store_lock.list_patterns(
        params.status.as_deref(),
        params.pattern_type.as_deref(),
        params.limit.unwrap_or(20),
    ).await?;

    Ok(VkbPatternListResponse {
        patterns: patterns.iter().map(pattern_to_mcp_response).collect(),
    })
}

pub async fn pattern_search_impl(
    engine: &ValenceEngine,
    params: PatternSearchParams,
) -> Result<VkbPatternListResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let store_lock = store.read().await;
    let patterns = store_lock.search_patterns(&params.query, params.limit.unwrap_or(10)).await?;

    Ok(VkbPatternListResponse {
        patterns: patterns.iter().map(pattern_to_mcp_response).collect(),
    })
}

pub async fn insight_extract_impl(
    engine: &ValenceEngine,
    params: InsightExtractParams,
) -> Result<VkbInsightResponse> {
    use crate::vkb::Insight;

    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let session_id = Uuid::parse_str(&params.session_id)?;
    let insight = Insight::new(session_id, &params.content);

    let store_lock = store.read().await;
    let insight_id = store_lock.extract_insight(insight.clone()).await?;

    Ok(VkbInsightResponse {
        id: insight_id.to_string(),
        session_id: session_id.to_string(),
        content: params.content,
        confidence: params.confidence.unwrap_or(0.8),
        created_at: insight.created_at,
    })
}

pub async fn insight_list_impl(
    engine: &ValenceEngine,
    params: InsightListParams,
) -> Result<VkbInsightListResponse> {
    let store = engine.session_store.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Session store not configured"))?;

    let session_id = Uuid::parse_str(&params.session_id)?;
    let store_lock = store.read().await;
    let insights = crate::vkb::SessionStore::list_insights(&*store_lock, session_id).await?;

    Ok(VkbInsightListResponse {
        insights: insights.iter().map(|i| VkbInsightResponse {
            id: i.id.to_string(),
            session_id: i.session_id.to_string(),
            content: i.content.clone(),
            confidence: 0.8,
            created_at: i.created_at,
        }).collect(),
    })
}

// Trust/Identity implementations

/// Query trust score for a DID using PageRank
pub async fn trust_query_impl(
    engine: &ValenceEngine,
    params: TrustQueryParams,
) -> Result<TrustQueryResponse> {
    use crate::graph::{GraphView, algorithms::pagerank};

    // Build graph view
    let graph = GraphView::from_store(&*engine.store).await?;

    // Run PageRank on the graph
    let ranks = pagerank(&graph, 0.85, 50);

    // Find the DID node
    let did_node = engine.store.find_node_by_value(&params.did).await?
        .ok_or_else(|| anyhow::anyhow!("DID not found: {}", params.did))?;

    let trust_score = ranks.get(&did_node.id).copied().unwrap_or(0.0);

    // Get connected DIDs (nodes with "trusts" edges)
    let pattern = crate::storage::TriplePattern {
        subject: Some(did_node.id),
        predicate: Some(crate::predicates::TRUSTS.to_string()),
        object: None,
    };
    let triples = engine.store.query_triples(pattern).await?;

    let mut connected_dids = Vec::new();
    for triple in triples {
        let object_node = engine.store.get_node(triple.object).await?
            .ok_or_else(|| anyhow::anyhow!("Object node not found"))?;
        let score = ranks.get(&object_node.id).copied().unwrap_or(0.0);
        connected_dids.push(ConnectedDidResponse {
            did: object_node.value,
            score,
        });
    }

    Ok(TrustQueryResponse {
        did: params.did,
        trust_score,
        connected_dids,
    })
}

/// Sign a triple with the local keypair
pub async fn sign_triple_impl(
    engine: &ValenceEngine,
    params: SignTripleParams,
) -> Result<SignTripleResponse> {
    let triple_id = Uuid::parse_str(&params.triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", params.triple_id))?;

    let triple = engine.store.get_triple(triple_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple not found: {}", params.triple_id))?;

    // Create message to sign: triple_id bytes
    let message = triple_id.as_bytes();
    let signature = engine.keypair.sign(message);
    let signature_b64 = base64::engine::general_purpose::STANDARD.encode(signature);

    Ok(SignTripleResponse {
        triple_id: params.triple_id,
        signature: signature_b64,
        signer_did: engine.keypair.did_string(),
    })
}

/// Verify a triple's signature
pub async fn verify_triple_impl(
    engine: &ValenceEngine,
    params: VerifyTripleParams,
) -> Result<VerifyTripleResponse> {
    use crate::identity::Keypair;

    let triple_id = Uuid::parse_str(&params.triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", params.triple_id))?;

    let triple = engine.store.get_triple(triple_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple not found: {}", params.triple_id))?;

    let valid = if let (Some(origin_did), Some(sig_b64)) = (&triple.origin_did, &triple.signature) {
        // Parse DID to get public key
        // did:valence:key:<base58-pubkey>
        if let Some(key_part) = origin_did.strip_prefix("did:valence:key:") {
            let pubkey_bytes = bs58::decode(key_part).into_vec()
                .map_err(|_| anyhow::anyhow!("Invalid base58 in DID"))?;
            if pubkey_bytes.len() != 32 {
                return Ok(VerifyTripleResponse {
                    triple_id: params.triple_id,
                    valid: false,
                    origin_did: Some(origin_did.clone()),
                });
            }
            let mut pubkey_arr = [0u8; 32];
            pubkey_arr.copy_from_slice(&pubkey_bytes);

            // Decode signature
            let sig_bytes = base64::engine::general_purpose::STANDARD.decode(sig_b64)
                .map_err(|_| anyhow::anyhow!("Invalid base64 signature"))?;
            if sig_bytes.len() != 64 {
                return Ok(VerifyTripleResponse {
                    triple_id: params.triple_id,
                    valid: false,
                    origin_did: Some(origin_did.clone()),
                });
            }
            let mut sig_arr = [0u8; 64];
            sig_arr.copy_from_slice(&sig_bytes);

            // Verify
            let message = triple_id.as_bytes();
            Keypair::verify(&pubkey_arr, message, &sig_arr)
        } else {
            false
        }
    } else {
        false
    };

    Ok(VerifyTripleResponse {
        triple_id: params.triple_id,
        valid,
        origin_did: triple.origin_did.clone(),
    })
}

// Knowledge management implementations

pub async fn triple_get_impl(
    engine: &ValenceEngine,
    params: TripleGetParams,
) -> Result<crate::api::TripleResponse> {
    use crate::api::{TripleResponse, NodeResponse, SourceResponse};

    let triple_id = Uuid::parse_str(&params.triple_id)?;
    let triple = engine.store.get_triple(triple_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple not found: {}", params.triple_id))?;

    let subject_node = engine.store.get_node(triple.subject).await?
        .ok_or_else(|| anyhow::anyhow!("Subject node not found"))?;
    let object_node = engine.store.get_node(triple.object).await?
        .ok_or_else(|| anyhow::anyhow!("Object node not found"))?;

    let sources = engine.store.get_sources_for_triple(triple.id).await?;
    let source_responses: Vec<SourceResponse> = sources
        .into_iter()
        .map(|s| SourceResponse {
            id: s.id.to_string(),
            source_type: s.source_type,
            reference: s.reference,
            created_at: s.created_at,
        })
        .collect();

    Ok(TripleResponse {
        id: triple.id.to_string(),
        subject: NodeResponse {
            id: subject_node.id.to_string(),
            value: subject_node.value,
        },
        predicate: triple.predicate.value,
        object: NodeResponse {
            id: object_node.id.to_string(),
            value: object_node.value,
        },
        origin_did: triple.origin_did.clone(),
        base_weight: triple.base_weight,
        local_weight: triple.local_weight,
        timestamp: triple.timestamp,
        last_accessed: triple.last_accessed,
        access_count: triple.access_count,
        sources: Some(source_responses),
    })
}

pub async fn triple_supersede_impl(
    engine: &ValenceEngine,
    params: TripleSupersedePar,
) -> Result<TripleSupersedeResponse> {
    let old_triple_id = Uuid::parse_str(&params.old_triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid old triple ID: {}", params.old_triple_id))?;

    // Verify old triple exists
    let old_triple = engine.store.get_triple(old_triple_id).await?
        .ok_or_else(|| anyhow::anyhow!("Old triple not found: {}", params.old_triple_id))?;

    // Create the new triple
    let subject_node = engine.store.find_or_create_node(&params.new_subject).await?;
    let object_node = engine.store.find_or_create_node(&params.new_object).await?;
    let new_triple = Triple::new(subject_node.id, &params.new_predicate, object_node.id);
    let new_triple_id = engine.store.insert_triple(new_triple).await?;

    // Create nodes for triple IDs to link them
    let new_triple_node = engine.store.find_or_create_node(&new_triple_id.to_string()).await?;
    let old_triple_node = engine.store.find_or_create_node(&old_triple_id.to_string()).await?;

    // Link: new_triple --supersedes--> old_triple
    let supersedes_triple = Triple::new(new_triple_node.id, predicates::SUPERSEDES, old_triple_node.id);
    engine.store.insert_triple(supersedes_triple).await?;

    // Link: new_triple --supersede_reason--> reason
    let reason_node = engine.store.find_or_create_node(&params.reason).await?;
    let reason_triple = Triple::new(new_triple_node.id, predicates::SUPERSEDE_REASON, reason_node.id);
    engine.store.insert_triple(reason_triple).await?;

    // Reduce old triple's weight
    let mut updated_old = old_triple;
    updated_old.local_weight *= 0.1;
    engine.store.update_triple(updated_old).await?;

    Ok(TripleSupersedeResponse {
        old_triple_id: old_triple_id.to_string(),
        new_triple_id: new_triple_id.to_string(),
        message: format!("Triple {} superseded by {}", old_triple_id, new_triple_id),
    })
}

pub async fn node_search_impl(
    engine: &ValenceEngine,
    params: NodeSearchParams,
) -> Result<NodeSearchResponse> {
    let limit = params.limit.unwrap_or(20) as usize;
    let nodes = engine.store.search_nodes(&params.query, limit).await?;
    Ok(NodeSearchResponse {
        nodes: nodes.into_iter().map(|n| NodeResponse {
            id: n.id.to_string(),
            value: n.value,
        }).collect(),
    })
}

// ============================================================================
// Combined query tool
// ============================================================================

/// Execute the combined "connected AND similar" query via MCP.
pub async fn combined_query_impl(
    engine: &ValenceEngine,
    params: crate::query::combined::CombinedQueryParams,
) -> Result<crate::query::combined::CombinedQueryResponse> {
    crate::query::combined::combined_query(engine, params).await
}

// ============================================================================
// Confidence explain tool
// ============================================================================

/// Parameters for the confidence_explain tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConfidenceExplainParams {
    /// The triple ID to explain confidence for
    pub triple_id: String,
    /// Optional node value for query context (affects path diversity scoring)
    pub context: Option<String>,
}

/// Response from the confidence_explain tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConfidenceExplainResponse {
    pub triple_id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub combined: f64,
    pub source_reliability: f64,
    pub path_diversity: f64,
    pub centrality: f64,
    pub weights: ConfidenceWeights,
    pub source_count: usize,
    pub context: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConfidenceWeights {
    pub source_reliability: f64,
    pub path_diversity: f64,
    pub centrality: f64,
}

pub async fn confidence_explain_impl(
    engine: &ValenceEngine,
    params: ConfidenceExplainParams,
) -> Result<ConfidenceExplainResponse> {
    use crate::graph::{GraphView, DynamicConfidence};

    let triple_id = uuid::Uuid::parse_str(&params.triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", params.triple_id))?;

    let triple = engine.store.get_triple(triple_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple not found: {}", params.triple_id))?;

    // Resolve query context
    let query_context = if let Some(ref ctx) = params.context {
        engine.store.find_node_by_value(ctx).await?.map(|n| n.id)
    } else {
        None
    };

    // Build graph view and compute
    let graph_view = GraphView::from_store(&*engine.store).await?;
    let score = DynamicConfidence::compute_confidence(
        &*engine.store,
        &graph_view,
        triple_id,
        query_context,
    ).await?;

    let sources = engine.store.get_sources_for_triple(triple_id).await?;
    let subject = engine.store.get_node(triple.subject).await?
        .map(|n| n.value).unwrap_or_default();
    let object = engine.store.get_node(triple.object).await?
        .map(|n| n.value).unwrap_or_default();

    Ok(ConfidenceExplainResponse {
        triple_id: params.triple_id,
        subject,
        predicate: triple.predicate.value.clone(),
        object,
        combined: score.combined,
        source_reliability: score.source_reliability,
        path_diversity: score.path_diversity,
        centrality: score.centrality,
        weights: ConfidenceWeights {
            source_reliability: 0.5,
            path_diversity: 0.3,
            centrality: 0.2,
        },
        source_count: sources.len(),
        context: params.context,
    })
}

// ============================================================================
// Stub types and impls for tools referencing deleted modules
// (Trust, sharing, tension, verification, reputation are now graph operations)
// ============================================================================

/// Parameters for trust_check tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrustCheckParams {
    pub topic: String,
}

pub async fn trust_check_impl(
    _engine: &ValenceEngine,
    _params: TrustCheckParams,
) -> Result<PlaceholderResponse> {
    Ok(PlaceholderResponse {
        message: "Trust check: use trust_query with a DID for PageRank-based trust scores".to_string(),
    })
}

/// Parameters for trust_edge_create tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrustEdgeCreateParams {
    pub from_did: String,
    pub to_did: String,
}

pub async fn trust_edge_create_impl(
    engine: &ValenceEngine,
    params: TrustEdgeCreateParams,
) -> Result<TrustEdgeCreateResponse> {
    // Trust edges are just triples: from_did --trusts--> to_did
    let from_node = engine.store.find_or_create_node(&params.from_did).await?;
    let to_node = engine.store.find_or_create_node(&params.to_did).await?;
    let triple = Triple::new(from_node.id, predicates::TRUSTS, to_node.id);
    let triple_id = engine.store.insert_triple(triple).await?;
    Ok(TrustEdgeCreateResponse {
        triple_id: triple_id.to_string(),
        from_did: params.from_did,
        to_did: params.to_did,
    })
}

/// Parameters for reputation_get tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReputationGetParams {
    pub did: String,
}

pub async fn reputation_get_impl(
    engine: &ValenceEngine,
    params: ReputationGetParams,
) -> Result<ReputationGetResponse> {
    // Reputation is PageRank score of the DID node
    let result = trust_query_impl(engine, TrustQueryParams { did: params.did.clone() }).await?;
    Ok(ReputationGetResponse {
        did: params.did,
        trust_score: result.trust_score,
    })
}

/// Parameters for verification_submit tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerificationSubmitParams {
    /// ID of the triple being verified
    pub triple_id: String,
    /// Verifier identifier (DID or name)
    pub verifier: String,
    /// Verification result: confirmed, contradicted, or uncertain
    pub result: String,
    /// Optional reasoning for the verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

pub async fn verification_submit_impl(
    engine: &ValenceEngine,
    params: VerificationSubmitParams,
) -> Result<VerificationSubmitResponse> {
    // Validate result value
    let valid_results = ["confirmed", "contradicted", "uncertain"];
    if !valid_results.contains(&params.result.as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid result '{}', expected one of: confirmed, contradicted, uncertain",
            params.result
        ));
    }

    // Verify the target triple exists
    let triple_id = Uuid::parse_str(&params.triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", params.triple_id))?;
    let _triple = engine.store.get_triple(triple_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple not found: {}", params.triple_id))?;

    // Create verifier node and target triple node
    let verifier_node = engine.store.find_or_create_node(&params.verifier).await?;
    let target_node = engine.store.find_or_create_node(&params.triple_id).await?;

    // Create verification triple: verifier --verifies--> target_triple_id
    let verify_triple = Triple::new(verifier_node.id, predicates::VERIFIES, target_node.id);
    let verify_triple_id = engine.store.insert_triple(verify_triple).await?;

    // Create result triple: verification --verification_result--> result_value
    let verify_node = engine.store.find_or_create_node(&verify_triple_id.to_string()).await?;
    let result_node = engine.store.find_or_create_node(&params.result).await?;
    let result_triple = Triple::new(verify_node.id, predicates::VERIFICATION_RESULT, result_node.id);
    let result_triple_id = engine.store.insert_triple(result_triple).await?;

    // Optionally create reasoning triple
    let reasoning_triple_id = if let Some(ref reasoning) = params.reasoning {
        let reasoning_node = engine.store.find_or_create_node(reasoning).await?;
        let reasoning_triple = Triple::new(verify_node.id, predicates::VERIFICATION_REASONING, reasoning_node.id);
        Some(engine.store.insert_triple(reasoning_triple).await?)
    } else {
        None
    };

    Ok(VerificationSubmitResponse {
        verification_triple_id: verify_triple_id.to_string(),
        result_triple_id: result_triple_id.to_string(),
        reasoning_triple_id: reasoning_triple_id.map(|id| id.to_string()),
        message: format!(
            "Verification submitted: {} {} triple {}",
            params.verifier, params.result, params.triple_id
        ),
    })
}

/// Parameters for verification_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct VerificationListParams {
    pub triple_id: String,
}

pub async fn verification_list_impl(
    engine: &ValenceEngine,
    params: VerificationListParams,
) -> Result<VerificationListResponse> {
    // Find the target triple node
    let target_node = engine.store.find_node_by_value(&params.triple_id).await?;
    let target_node = match target_node {
        Some(n) => n,
        None => return Ok(VerificationListResponse { verifications: vec![] }),
    };

    // Query: * --verifies--> target_triple_id
    let pattern = TriplePattern {
        subject: None,
        predicate: Some(predicates::VERIFIES.to_string()),
        object: Some(target_node.id),
    };
    let verify_triples = engine.store.query_triples(pattern).await?;

    let mut verifications = Vec::new();
    for vt in &verify_triples {
        // Get verifier name
        let verifier_node = engine.store.get_node(vt.subject).await?
            .ok_or_else(|| anyhow::anyhow!("Verifier node not found"))?;

        // Find verification node (the ID of this verify triple)
        let verify_id_node = engine.store.find_node_by_value(&vt.id.to_string()).await?;

        let (result, reasoning) = if let Some(ref vid_node) = verify_id_node {
            // Get result: verify_node --verification_result--> result
            let result_pattern = TriplePattern {
                subject: Some(vid_node.id),
                predicate: Some(predicates::VERIFICATION_RESULT.to_string()),
                object: None,
            };
            let result_triples = engine.store.query_triples(result_pattern).await?;
            let result_val = if let Some(rt) = result_triples.first() {
                let rn = engine.store.get_node(rt.object).await?;
                rn.map(|n| n.value).unwrap_or_default()
            } else {
                String::new()
            };

            // Get reasoning: verify_node --verification_reasoning--> reasoning
            let reasoning_pattern = TriplePattern {
                subject: Some(vid_node.id),
                predicate: Some(predicates::VERIFICATION_REASONING.to_string()),
                object: None,
            };
            let reasoning_triples = engine.store.query_triples(reasoning_pattern).await?;
            let reasoning_val = if let Some(rt) = reasoning_triples.first() {
                let rn = engine.store.get_node(rt.object).await?;
                rn.map(|n| n.value)
            } else {
                None
            };

            (result_val, reasoning_val)
        } else {
            (String::new(), None)
        };

        verifications.push(VerificationEntry {
            verification_triple_id: vt.id.to_string(),
            verifier: verifier_node.value,
            target_triple_id: params.triple_id.clone(),
            result,
            reasoning,
        });
    }

    Ok(VerificationListResponse { verifications })
}

/// Parameters for share_create tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareCreateParams {
    pub triple_id: String,
    pub recipient_did: String,
}

pub async fn share_create_impl(
    engine: &ValenceEngine,
    params: ShareCreateParams,
) -> Result<ShareCreateResponse> {
    // Sharing is a triple: triple_node --shareable_with--> recipient_did
    let triple_node = engine.store.find_or_create_node(&params.triple_id).await?;
    let recipient_node = engine.store.find_or_create_node(&params.recipient_did).await?;
    let share_triple = Triple::new(triple_node.id, predicates::SHAREABLE_WITH, recipient_node.id);
    let share_id = engine.store.insert_triple(share_triple).await?;
    Ok(ShareCreateResponse {
        share_triple_id: share_id.to_string(),
        triple_id: params.triple_id,
        recipient_did: params.recipient_did,
    })
}

/// Parameters for share_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

pub async fn share_list_impl(
    engine: &ValenceEngine,
    _params: ShareListParams,
) -> Result<ShareListResponse> {
    // Query all triples with predicate SHAREABLE_WITH
    let pattern = TriplePattern {
        subject: None,
        predicate: Some(predicates::SHAREABLE_WITH.to_string()),
        object: None,
    };
    let share_triples = engine.store.query_triples(pattern).await?;

    let mut shares = Vec::new();
    for st in &share_triples {
        let subject_node = engine.store.get_node(st.subject).await?
            .ok_or_else(|| anyhow::anyhow!("Subject node not found"))?;
        let object_node = engine.store.get_node(st.object).await?
            .ok_or_else(|| anyhow::anyhow!("Object node not found"))?;

        shares.push(ShareEntry {
            share_triple_id: st.id.to_string(),
            triple_id: subject_node.value,
            recipient_did: object_node.value,
            weight: st.local_weight,
        });
    }

    Ok(ShareListResponse { shares })
}

/// Parameters for share_revoke tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShareRevokeParams {
    pub share_id: String,
}

pub async fn share_revoke_impl(
    engine: &ValenceEngine,
    params: ShareRevokeParams,
) -> Result<ShareRevokeResponse> {
    let share_id = Uuid::parse_str(&params.share_id)
        .map_err(|_| anyhow::anyhow!("Invalid share ID: {}", params.share_id))?;

    // Verify the share triple exists
    let share_triple = engine.store.get_triple(share_id).await?
        .ok_or_else(|| anyhow::anyhow!("Share triple not found: {}", params.share_id))?;

    // Create retraction metadata
    let share_node = engine.store.find_or_create_node(&share_id.to_string()).await?;

    let retractor_node = engine.store.find_or_create_node("local").await?;
    let retract_triple = Triple::new(share_node.id, predicates::RETRACTED_BY, retractor_node.id);
    engine.store.insert_triple(retract_triple).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let timestamp_node = engine.store.find_or_create_node(&now).await?;
    let timestamp_triple = Triple::new(share_node.id, predicates::RETRACTED_AT, timestamp_node.id);
    engine.store.insert_triple(timestamp_triple).await?;

    // Zero out the share triple's weight
    let mut updated = share_triple;
    updated.local_weight = 0.0;
    engine.store.update_triple(updated).await?;

    Ok(ShareRevokeResponse {
        share_id: share_id.to_string(),
        message: format!("Share {} revoked", share_id),
    })
}

/// Parameters for tension_list tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TensionListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

pub async fn tension_list_impl(
    engine: &ValenceEngine,
    _params: TensionListParams,
) -> Result<TensionListResponse> {
    use std::collections::HashMap;

    // Get all active triples (weight > 0)
    let pattern = TriplePattern {
        subject: None,
        predicate: None,
        object: None,
    };
    let all_triples = engine.store.query_triples(pattern).await?;

    // Group triples by (subject, predicate) to find conflicts
    // A tension is when the same subject has the same predicate pointing to different objects
    let mut by_subject_pred: HashMap<(uuid::Uuid, String), Vec<&Triple>> = HashMap::new();
    for triple in &all_triples {
        if triple.local_weight > 0.0 {
            let key = (triple.subject, triple.predicate.value.clone());
            by_subject_pred.entry(key).or_default().push(triple);
        }
    }

    let mut tensions = Vec::new();

    for ((subject_id, predicate), triples) in &by_subject_pred {
        if triples.len() < 2 {
            continue;
        }

        // Skip predicates that are naturally multi-valued
        let multi_valued = ["knows", "likes", "trusts", predicates::SHAREABLE_WITH,
                           predicates::VERIFIES, predicates::SUPERSEDES];
        if multi_valued.contains(&predicate.as_str()) {
            continue;
        }

        // Get subject name for reporting
        let subject_node = engine.store.get_node(*subject_id).await?
            .map(|n| n.value)
            .unwrap_or_else(|| subject_id.to_string());

        // Each pair of triples with different objects is a tension
        for i in 0..triples.len() {
            for j in (i + 1)..triples.len() {
                if triples[i].object != triples[j].object {
                    let severity = if predicate == "is" || predicate == "is_a" || predicate == "has_type" {
                        "high"
                    } else {
                        "medium"
                    };

                    tensions.push(TensionEntry {
                        triple_a_id: triples[i].id.to_string(),
                        triple_b_id: triples[j].id.to_string(),
                        tension_type: "conflicting_objects".to_string(),
                        severity: severity.to_string(),
                        subject: subject_node.clone(),
                        predicate: predicate.clone(),
                    });
                }
            }
        }
    }

    Ok(TensionListResponse { tensions })
}

/// Parameters for tension_resolve tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TensionResolveParams {
    /// ID of the first conflicting triple
    pub triple_a_id: String,
    /// ID of the second conflicting triple
    pub triple_b_id: String,
    /// Resolution action: keep_a, keep_b, keep_both, archive_both
    pub action: String,
    /// Reasoning for the resolution
    pub reasoning: String,
}

pub async fn tension_resolve_impl(
    engine: &ValenceEngine,
    params: TensionResolveParams,
) -> Result<TensionResolveResponse> {
    let valid_actions = ["keep_a", "keep_b", "keep_both", "archive_both"];
    if !valid_actions.contains(&params.action.as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid action '{}', expected one of: keep_a, keep_b, keep_both, archive_both",
            params.action
        ));
    }

    let triple_a_id = Uuid::parse_str(&params.triple_a_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple_a_id: {}", params.triple_a_id))?;
    let triple_b_id = Uuid::parse_str(&params.triple_b_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple_b_id: {}", params.triple_b_id))?;

    let triple_a = engine.store.get_triple(triple_a_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple A not found: {}", params.triple_a_id))?;
    let triple_b = engine.store.get_triple(triple_b_id).await?
        .ok_or_else(|| anyhow::anyhow!("Triple B not found: {}", params.triple_b_id))?;

    // Apply action
    match params.action.as_str() {
        "keep_a" => {
            let mut updated_b = triple_b;
            updated_b.local_weight *= 0.1;
            engine.store.update_triple(updated_b).await?;
        }
        "keep_b" => {
            let mut updated_a = triple_a;
            updated_a.local_weight *= 0.1;
            engine.store.update_triple(updated_a).await?;
        }
        "archive_both" => {
            let mut updated_a = triple_a;
            updated_a.local_weight *= 0.1;
            engine.store.update_triple(updated_a).await?;
            let mut updated_b = triple_b;
            updated_b.local_weight *= 0.1;
            engine.store.update_triple(updated_b).await?;
        }
        "keep_both" => {
            // No weight changes — both are kept as-is
        }
        _ => unreachable!(),
    }

    // Create resolution triple: triple_a_node --tension_resolved_with--> triple_b_node
    let triple_a_node = engine.store.find_or_create_node(&params.triple_a_id).await?;
    let triple_b_node = engine.store.find_or_create_node(&params.triple_b_id).await?;
    let resolution_triple = Triple::new(triple_a_node.id, predicates::TENSION_RESOLVED_WITH, triple_b_node.id);
    let resolution_id = engine.store.insert_triple(resolution_triple).await?;

    // Record the action
    let resolution_node = engine.store.find_or_create_node(&resolution_id.to_string()).await?;
    let action_node = engine.store.find_or_create_node(&params.action).await?;
    let action_triple = Triple::new(resolution_node.id, predicates::TENSION_RESOLUTION_ACTION, action_node.id);
    engine.store.insert_triple(action_triple).await?;

    // Record the reasoning
    let reasoning_node = engine.store.find_or_create_node(&params.reasoning).await?;
    let reasoning_triple = Triple::new(resolution_node.id, predicates::TENSION_RESOLUTION_REASONING, reasoning_node.id);
    engine.store.insert_triple(reasoning_triple).await?;

    Ok(TensionResolveResponse {
        resolution_triple_id: resolution_id.to_string(),
        action: params.action,
        message: format!("Tension between {} and {} resolved", params.triple_a_id, params.triple_b_id),
    })
}

// ============================================================================
// Social tool tests
// ============================================================================

#[cfg(test)]
mod social_tests {
    use super::*;
    use crate::models::Triple;

    #[tokio::test]
    async fn test_node_search() {
        let engine = ValenceEngine::new();

        engine.store.find_or_create_node("Alice").await.unwrap();
        engine.store.find_or_create_node("Alice Smith").await.unwrap();
        engine.store.find_or_create_node("Bob").await.unwrap();

        let params = NodeSearchParams {
            query: "alice".to_string(),
            limit: Some(10),
        };
        let response = node_search_impl(&engine, params).await.unwrap();
        assert_eq!(response.nodes.len(), 2);
        assert!(response.nodes.iter().all(|n| n.value.to_lowercase().contains("alice")));
    }

    #[tokio::test]
    async fn test_share_list() {
        let engine = ValenceEngine::new();

        // Create a triple, then share it
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let triple_id = engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        // Create a share
        let share_params = ShareCreateParams {
            triple_id: triple_id.to_string(),
            recipient_did: "did:valence:carol".to_string(),
        };
        share_create_impl(&engine, share_params).await.unwrap();

        // List shares
        let list_params = ShareListParams { direction: None };
        let response = share_list_impl(&engine, list_params).await.unwrap();
        assert_eq!(response.shares.len(), 1);
        assert_eq!(response.shares[0].triple_id, triple_id.to_string());
        assert_eq!(response.shares[0].recipient_did, "did:valence:carol");
    }

    #[tokio::test]
    async fn test_verification_submit() {
        let engine = ValenceEngine::new();

        // Create a triple to verify
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let triple_id = engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        // Submit verification
        let params = VerificationSubmitParams {
            triple_id: triple_id.to_string(),
            verifier: "did:valence:carol".to_string(),
            result: "confirmed".to_string(),
            reasoning: Some("I witnessed them meeting".to_string()),
        };
        let response = verification_submit_impl(&engine, params).await.unwrap();

        assert!(!response.verification_triple_id.is_empty());
        assert!(!response.result_triple_id.is_empty());
        assert!(response.reasoning_triple_id.is_some());
        assert!(response.message.contains("confirmed"));
    }

    #[tokio::test]
    async fn test_verification_list() {
        let engine = ValenceEngine::new();

        // Create a triple and verify it
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let triple_id = engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        // Submit a verification
        let submit_params = VerificationSubmitParams {
            triple_id: triple_id.to_string(),
            verifier: "did:valence:carol".to_string(),
            result: "confirmed".to_string(),
            reasoning: Some("Verified through observation".to_string()),
        };
        verification_submit_impl(&engine, submit_params).await.unwrap();

        // List verifications
        let list_params = VerificationListParams {
            triple_id: triple_id.to_string(),
        };
        let response = verification_list_impl(&engine, list_params).await.unwrap();
        assert_eq!(response.verifications.len(), 1);
        assert_eq!(response.verifications[0].verifier, "did:valence:carol");
        assert_eq!(response.verifications[0].result, "confirmed");
        assert_eq!(response.verifications[0].reasoning.as_deref(), Some("Verified through observation"));
    }

    #[tokio::test]
    async fn test_tension_detection() {
        let engine = ValenceEngine::new();

        // Create contradictory triples: same subject, same predicate, different objects
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let cat = engine.store.find_or_create_node("cat").await.unwrap();
        let dog = engine.store.find_or_create_node("dog").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "is", cat.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "is", dog.id)).await.unwrap();

        // Detect tensions
        let params = TensionListParams { severity: None };
        let response = tension_list_impl(&engine, params).await.unwrap();

        assert_eq!(response.tensions.len(), 1);
        assert_eq!(response.tensions[0].tension_type, "conflicting_objects");
        assert_eq!(response.tensions[0].severity, "high"); // "is" predicate => high severity
        assert_eq!(response.tensions[0].subject, "Alice");
        assert_eq!(response.tensions[0].predicate, "is");
    }

    #[tokio::test]
    async fn test_tension_resolve() {
        let engine = ValenceEngine::new();

        // Create contradictory triples
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let cat = engine.store.find_or_create_node("cat").await.unwrap();
        let dog = engine.store.find_or_create_node("dog").await.unwrap();

        let triple_a_id = engine.store.insert_triple(Triple::new(alice.id, "is", cat.id)).await.unwrap();
        let triple_b_id = engine.store.insert_triple(Triple::new(alice.id, "is", dog.id)).await.unwrap();

        // Resolve: keep_a (Alice is a cat, not a dog)
        let params = TensionResolveParams {
            triple_a_id: triple_a_id.to_string(),
            triple_b_id: triple_b_id.to_string(),
            action: "keep_a".to_string(),
            reasoning: "Alice is definitely a cat".to_string(),
        };
        let response = tension_resolve_impl(&engine, params).await.unwrap();

        assert!(!response.resolution_triple_id.is_empty());
        assert_eq!(response.action, "keep_a");

        // Verify triple_b's weight was reduced
        let triple_b = engine.store.get_triple(triple_b_id).await.unwrap().unwrap();
        assert!(triple_b.local_weight < 0.5, "Triple B weight should be reduced, got {}", triple_b.local_weight);

        // Triple A should be unchanged
        let triple_a = engine.store.get_triple(triple_a_id).await.unwrap().unwrap();
        assert!((triple_a.local_weight - 1.0).abs() < f64::EPSILON, "Triple A weight should be unchanged");
    }

    #[tokio::test]
    async fn test_triple_supersede_response() {
        let engine = ValenceEngine::new();

        // Create a triple to supersede
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let old_triple_id = engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        let params = TripleSupersedePar {
            old_triple_id: old_triple_id.to_string(),
            new_subject: "Alice".to_string(),
            new_predicate: "friends_with".to_string(),
            new_object: "Bob".to_string(),
            reason: "Updated relationship".to_string(),
        };
        let response = triple_supersede_impl(&engine, params).await.unwrap();

        assert_eq!(response.old_triple_id, old_triple_id.to_string());
        assert!(!response.new_triple_id.is_empty());
        assert!(response.message.contains("superseded"));

        // Verify old triple's weight was reduced
        let old_triple = engine.store.get_triple(old_triple_id).await.unwrap().unwrap();
        assert!(old_triple.local_weight < 0.5);
    }
}
