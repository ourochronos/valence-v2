//! MCP tool implementation functions
//!
//! These functions implement the actual business logic for each MCP tool,
//! mapping to ValenceEngine operations.

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

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
};

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
            weight: triple.weight,
            created_at: triple.created_at,
            last_accessed: triple.last_accessed,
            access_count: triple.access_count,
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
            weight: triple.weight,
            created_at: triple.created_at,
            last_accessed: triple.last_accessed,
            access_count: triple.access_count,
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
        triples.iter().map(|t| t.weight).sum::<f64>() / triples.len() as f64
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
                confidence: scored_result.confidence.unwrap_or(triple.weight),
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
