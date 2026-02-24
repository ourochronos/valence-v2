use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use uuid::Uuid;
use base64::Engine;

use crate::{
    engine::ValenceEngine,
    embeddings::EmbeddingStore,
    graph::{GraphView, DynamicConfidence},
    models::{Source, Triple},
    storage::{MemoryStore, TriplePattern},
    vkb::SessionStore,
};

mod types;
mod resilience_endpoints;
pub use types::*;
use resilience_endpoints::{get_degradation_status, reset_degradation};

/// API server state - uses ValenceEngine
#[derive(Clone)]
pub struct ApiState {
    pub engine: ValenceEngine,
    pub start_time: std::time::Instant,
    pub store_type: String,
}

impl ApiState {
    pub fn new(engine: ValenceEngine, store_type: String) -> Self {
        Self { 
            engine,
            start_time: std::time::Instant::now(),
            store_type,
        }
    }

    /// Backward compatibility: create from a MemoryStore
    pub fn from_store(store: MemoryStore) -> Self {
        Self {
            engine: ValenceEngine::from_store(store),
            start_time: std::time::Instant::now(),
            store_type: "memory".to_string(),
        }
    }
}

/// Create the API router with all endpoints
pub fn create_router(engine: ValenceEngine) -> Router {
    create_router_with_store_type(engine, "memory".to_string())
}

/// Create the API router with store type specification
pub fn create_router_with_store_type(engine: ValenceEngine, store_type: String) -> Router {
    let state = ApiState::new(engine, store_type);

    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Triple operations
        .route("/triples", post(insert_triples))
        .route("/triples", get(query_triples))
        .route("/triples/{id}/sources", get(get_triple_sources))
        // Node operations
        .route("/nodes/{node}/neighbors", get(get_neighbors))
        // Search
        .route("/search", post(search))
        // Statistics
        .route("/stats", get(get_stats))
        // Maintenance
        .route("/maintenance/decay", post(trigger_decay))
        .route("/maintenance/evict", post(trigger_evict))
        .route("/maintenance/recompute-embeddings", post(recompute_embeddings))
        .route("/maintenance/recompute-node2vec", post(recompute_node2vec))
        .route("/maintenance/reinforce", post(trigger_stigmergy_reinforcement))
        .route("/maintenance/lifecycle", post(run_lifecycle_cycle))
        // Lifecycle stats
        .route("/stats/lifecycle", get(get_lifecycle_status))
        // Context assembly
        .route("/context", post(assemble_context))
        // Inference training loop / feedback
        .route("/inference/feedback", post(submit_feedback))
        .route("/inference/stats", get(get_feedback_stats))
        // Resilience / degradation
        .route("/resilience/status", get(get_degradation_status))
        .route("/resilience/reset", post(reset_degradation))
        // VKB endpoints
        .route("/sessions", post(create_session))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/end", post(end_session))
        .route("/sessions/room/{room_id}", get(find_session_by_room))
        .route("/sessions/{id}/exchanges", post(add_exchange))
        .route("/sessions/{id}/exchanges", get(list_exchanges))
        .route("/patterns", post(record_pattern))
        .route("/patterns/{id}/reinforce", post(reinforce_pattern))
        .route("/patterns", get(list_patterns))
        .route("/patterns/search", get(search_patterns))
        .route("/sessions/{id}/insights", post(extract_insight))
        .route("/sessions/{id}/insights", get(list_insights))
        // Trust endpoints
        .route("/trust", get(query_trust))
        // Knowledge management endpoints
        .route("/triples/{id}", get(get_triple_detail))
        .route("/triples/{id}/supersede", post(supersede_triple))
        .route("/triples/{id}/confidence", get(explain_confidence))
        .route("/triples/{id}/sign", post(sign_triple))
        .route("/triples/{id}/verify", get(verify_triple))
        .route("/nodes/search", get(search_nodes))
        // Combined query: connected AND similar
        .route("/query/combined", post(combined_query))
        .with_state(state)
}

/// GET /health — Enhanced health check endpoint
async fn health_check(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    // Check if store is accessible by counting triples
    let triple_count = state.engine.store.count_triples().await?;
    let node_count = state.engine.store.count_nodes().await?;
    
    // Calculate uptime
    let uptime_secs = state.start_time.elapsed().as_secs();
    let uptime_human = format_duration(uptime_secs);
    
    // Check module status
    let has_embeddings = state.engine.has_embeddings().await;
    let embeddings_lock = state.engine.embeddings.read().await;
    let embedding_count = embeddings_lock.len();
    drop(embeddings_lock);
    
    let has_feedback_recorder = state.engine.feedback_recorder.is_some();
    let has_weight_adjuster = state.engine.weight_adjuster.is_some();
    
    // Get lifecycle status
    let lifecycle_status = state.engine.lifecycle_status().await?;
    
    // Get current degradation level
    let degradation_level = state.engine.resilience.current_level().await;
    
    Ok(Json(serde_json::json!({
        "status": "healthy",
        "store_type": state.store_type,
        "uptime_seconds": uptime_secs,
        "uptime": uptime_human,
        "storage": {
            "triple_count": triple_count,
            "node_count": node_count,
            "max_triples": lifecycle_status.max_triples,
            "max_nodes": lifecycle_status.max_nodes,
            "utilization": lifecycle_status.utilization,
        },
        "modules": {
            "embeddings": {
                "enabled": has_embeddings,
                "count": embedding_count,
            },
            "stigmergy": {
                "enabled": true,
            },
            "lifecycle": {
                "enabled": true,
                "bounds_enforced": lifecycle_status.triples_exceeded || lifecycle_status.nodes_exceeded,
            },
            "inference": {
                "feedback_recorder": has_feedback_recorder,
                "weight_adjuster": has_weight_adjuster,
            },
            "resilience": {
                "enabled": true,
                "degradation_level": format!("{:?}", degradation_level),
            },
        },
    })))
}

/// Format duration in human-readable format (e.g., "2d 3h 45m 12s")
fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 || hours > 0 || days > 0 {
        parts.push(format!("{}m", minutes));
    }
    parts.push(format!("{}s", secs));
    
    parts.join(" ")
}

/// POST /triples — Insert one or more triples with optional source
async fn insert_triples(
    State(state): State<ApiState>,
    Json(req): Json<InsertTriplesRequest>,
) -> Result<Json<InsertTriplesResponse>, ApiError> {
    let mut triple_ids = Vec::new();

    // Insert each triple
    for triple_req in &req.triples {
        // Find or create subject and object nodes
        let subject_node = state
            .engine
            .store
            .find_or_create_node(&triple_req.subject)
            .await?;
        let object_node = state
            .engine
            .store
            .find_or_create_node(&triple_req.object)
            .await?;

        // Create and insert triple
        let triple = Triple::new(subject_node.id, &triple_req.predicate, object_node.id);
        let triple_id = state.engine.store.insert_triple(triple).await?;
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
        let source_id = state.engine.store.insert_source(source).await?;
        Some(source_id)
    } else {
        None
    };

    Ok(Json(InsertTriplesResponse {
        triple_ids: triple_ids.iter().map(|id| id.to_string()).collect(),
        source_id: source_id.map(|id| id.to_string()),
    }))
}

/// GET /triples?subject=X&predicate=Y&object=Z — Query triples by pattern
async fn query_triples(
    State(state): State<ApiState>,
    Query(params): Query<QueryTriplesParams>,
) -> Result<Json<QueryTriplesResponse>, ApiError> {
    // Resolve node values to IDs
    let subject_id = if let Some(ref subject_value) = params.subject {
        state
            .engine
            .store
            .find_node_by_value(subject_value)
            .await?
            .map(|n| n.id)
    } else {
        None
    };

    let object_id = if let Some(ref object_value) = params.object {
        state
            .engine
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
        predicate: params.predicate.clone(),
        object: object_id,
    };

    let triples = state.engine.store.query_triples(pattern).await?;

    // Record access for stigmergy (track which triples were retrieved together)
    if !triples.is_empty() {
        let triple_ids: Vec<_> = triples.iter().map(|t| t.id).collect();
        let context = format!("query_{}", uuid::Uuid::new_v4());
        state.engine.access_tracker
            .record_access(&triple_ids, &context)
            .await;
    }

    // Convert to response format
    let mut triple_responses = Vec::new();
    for triple in triples {
        let subject_node = state.engine.store.get_node(triple.subject).await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Subject node not found: {:?}", triple.subject)))?;
        let object_node = state.engine.store.get_node(triple.object).await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Object node not found: {:?}", triple.object)))?;

        let sources = if params.include_sources.unwrap_or(false) {
            let sources = state.engine.store.get_sources_for_triple(triple.id).await?;
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

    Ok(Json(QueryTriplesResponse {
        triples: triple_responses,
    }))
}

/// GET /nodes/:node/neighbors?depth=2 — Get k-hop neighborhood
async fn get_neighbors(
    State(state): State<ApiState>,
    Path(node): Path<String>,
    Query(params): Query<NeighborsParams>,
) -> Result<Json<NeighborsResponse>, ApiError> {
    // Try to parse as UUID, otherwise lookup by value
    let node_id = if let Ok(uuid) = Uuid::parse_str(&node) {
        uuid
    } else {
        state
            .engine
            .store
            .find_node_by_value(&node)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Node not found: {}", node)))?
            .id
    };

    let depth = params.depth.unwrap_or(1);
    
    // Validate depth parameter (prevent pathological queries)
    if depth == 0 {
        return Err(ApiError::BadRequest("Depth must be at least 1".to_string()));
    }
    if depth > 10 {
        return Err(ApiError::BadRequest("Depth cannot exceed 10 (too expensive)".to_string()));
    }
    let triples = state.engine.store.neighbors(node_id, depth).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    let mut unique_nodes = std::collections::HashSet::new();

    for triple in &triples {
        let subject_node = state.engine.store.get_node(triple.subject).await?.unwrap();
        let object_node = state.engine.store.get_node(triple.object).await?.unwrap();

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

    Ok(Json(NeighborsResponse {
        triples: triple_responses,
        node_count: unique_nodes.len(),
        triple_count: triples.len(),
    }))
}

/// GET /triples/:id/sources — Get provenance for a triple
async fn get_triple_sources(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<SourcesResponse>, ApiError> {
    let triple_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid triple ID: {}", id)))?;

    let sources = state.engine.store.get_sources_for_triple(triple_id).await?;

    let source_responses: Vec<SourceResponse> = sources
        .into_iter()
        .map(|s| SourceResponse {
            id: s.id.to_string(),
            source_type: s.source_type,
            reference: s.reference,
            created_at: s.created_at,
        })
        .collect();

    Ok(Json(SourcesResponse {
        sources: source_responses,
    }))
}

/// GET /stats — Get engine statistics
async fn get_stats(State(state): State<ApiState>) -> Result<Json<StatsResponse>, ApiError> {
    let triple_count = state.engine.store.count_triples().await?;
    let node_count = state.engine.store.count_nodes().await?;

    // Calculate average weight - sample up to 1000 triples to avoid OOM on large graphs
    let pattern = TriplePattern {
        subject: None,
        predicate: None,
        object: None,
    };
    let triples = state.engine.store.query_triples(pattern).await?;
    
    // For large graphs, sample instead of loading everything
    let sample_size = triples.len().min(1000);
    let avg_weight = if sample_size > 0 {
        triples.iter().take(sample_size).map(|t| t.local_weight).sum::<f64>() / sample_size as f64
    } else {
        0.0
    };

    Ok(Json(StatsResponse {
        triple_count,
        node_count,
        avg_weight,
    }))
}

/// POST /maintenance/decay — Trigger decay cycle
async fn trigger_decay(
    State(state): State<ApiState>,
    Json(req): Json<DecayRequest>,
) -> Result<Json<DecayResponse>, ApiError> {
    // Validate decay parameters
    if req.factor < 0.0 || req.factor > 1.0 {
        return Err(ApiError::BadRequest("Decay factor must be between 0.0 and 1.0".to_string()));
    }
    if req.min_weight < 0.0 {
        return Err(ApiError::BadRequest("Min weight cannot be negative".to_string()));
    }
    if req.min_weight > 1.0 {
        return Err(ApiError::BadRequest("Min weight cannot exceed 1.0".to_string()));
    }
    
    let affected_count = state.engine.store.decay(req.factor, req.min_weight).await?;

    Ok(Json(DecayResponse { affected_count }))
}

/// POST /maintenance/evict — Remove low-weight triples
async fn trigger_evict(
    State(state): State<ApiState>,
    Json(req): Json<EvictRequest>,
) -> Result<Json<EvictResponse>, ApiError> {
    // Validate threshold parameter
    if req.threshold < 0.0 {
        return Err(ApiError::BadRequest("Eviction threshold cannot be negative".to_string()));
    }
    
    let evicted_count = state.engine.store.evict_below_weight(req.threshold).await?;

    Ok(Json(EvictResponse { evicted_count }))
}

/// POST /search — Semantic search using embeddings
async fn search(
    State(state): State<ApiState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    use std::sync::Arc;
    use crate::budget::{TieredRetriever, OperationBudget};

    // Validate k parameter (prevent pathological queries)
    if req.k == 0 {
        return Err(ApiError::BadRequest("k must be at least 1".to_string()));
    }
    if req.k > 1000 {
        return Err(ApiError::BadRequest("k cannot exceed 1000 (too expensive)".to_string()));
    }
    
    // Use tiered retrieval if requested
    if req.use_tiered {
        let budget_ms = req.budget_ms.unwrap_or(100);
        let confidence_threshold = req.confidence_threshold.unwrap_or(0.8);
        
        // Validate budget parameters
        if budget_ms == 0 {
            return Err(ApiError::BadRequest("budget_ms must be at least 1".to_string()));
        }
        if budget_ms > 10000 {
            return Err(ApiError::BadRequest("budget_ms cannot exceed 10000 (10 seconds)".to_string()));
        }
        if confidence_threshold < 0.0 || confidence_threshold > 1.0 {
            return Err(ApiError::BadRequest("confidence_threshold must be between 0.0 and 1.0".to_string()));
        }

        // Create budget: time, hops, results
        let budget = OperationBudget::new(budget_ms, 5, req.k);
        
        // Create tiered retriever
        let retriever = TieredRetriever::new(Arc::new(state.engine.clone()));
        
        // Run tiered retrieval
        let retrieval_result = retriever.retrieve(&req.query_node, budget, confidence_threshold).await?;
        
        // Convert to API response
        let results: Vec<SearchResult> = retrieval_result.results
            .into_iter()
            .map(|r| SearchResult {
                node_id: r.node_id.to_string(),
                value: r.value,
                similarity: r.similarity,
                confidence: r.confidence,
            })
            .collect();
        
        return Ok(Json(SearchResponse {
            results,
            tier_reached: Some(retrieval_result.tier_reached),
            time_ms: Some(retrieval_result.time_ms),
            budget_exhausted: Some(retrieval_result.budget_exhausted),
            fallback: Some(false), // Tiered search requires embeddings
        }));
    }

    // Standard (non-tiered) search with graceful degradation
    // Find the query node by value
    let query_node = state
        .engine
        .store
        .find_node_by_value(&req.query_node)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Query node not found: {}", req.query_node)))?;

    // Try to get embedding for the query node
    let embeddings_store = state.engine.embeddings.read().await;
    let query_embedding = embeddings_store.get(query_node.id);

    let mut results = Vec::new();
    let mut fallback_mode = false;

    if let Some(query_emb) = query_embedding {
        // Warm mode: use embedding similarity
        let neighbors = embeddings_store.query_nearest(query_emb, req.k)?;
        drop(embeddings_store); // Release lock before async operations

        for (node_id, similarity) in neighbors {
            // Get node value
            let node = state
                .engine
                .store
                .get_node(node_id)
                .await?
                .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Node not found: {:?}", node_id)))?;

            // Optionally compute confidence
            let confidence = if req.include_confidence {
                // Build graph view
                let graph_view = GraphView::from_store(&*state.engine.store).await?;

                // Find a triple involving this node to compute confidence
                let pattern = TriplePattern {
                    subject: Some(node_id),
                    predicate: None,
                    object: None,
                };
                let triples = state.engine.store.query_triples(pattern).await?;

                if let Some(triple) = triples.first() {
                    let conf = DynamicConfidence::compute_confidence(
                        &*state.engine.store,
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
    } else {
        // Cold mode: fall back to graph-based search
        drop(embeddings_store);
        
        tracing::warn!(
            "No embedding found for query node '{}', falling back to graph-based search",
            req.query_node
        );
        
        fallback_mode = true;

        // Use graph traversal to find related nodes
        let depth = 2; // 2-hop neighborhood
        let neighbor_triples = state.engine.store.neighbors(query_node.id, depth).await?;

        // Extract unique nodes from triples
        let mut node_set = std::collections::HashSet::new();
        node_set.insert(query_node.id); // Include query node itself
        
        for triple in &neighbor_triples {
            node_set.insert(triple.subject);
            node_set.insert(triple.object);
        }

        // Convert to results (limit to k)
        let mut node_list: Vec<_> = node_set.into_iter().collect();
        node_list.truncate(req.k);

        for node_id in node_list {
            let node = state
                .engine
                .store
                .get_node(node_id)
                .await?
                .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Node not found: {:?}", node_id)))?;

            results.push(SearchResult {
                node_id: node_id.to_string(),
                value: node.value,
                similarity: 0.0, // No similarity in graph-based mode
                confidence: None, // Skip confidence computation in fallback
            });
        }
    }

    Ok(Json(SearchResponse {
        results,
        tier_reached: None,
        time_ms: None,
        budget_exhausted: None,
        fallback: Some(fallback_mode),
    }))
}

/// POST /maintenance/recompute-embeddings — Recompute embeddings from current graph
async fn recompute_embeddings(
    State(state): State<ApiState>,
    Json(req): Json<RecomputeEmbeddingsRequest>,
) -> Result<Json<RecomputeEmbeddingsResponse>, ApiError> {
    // Validate dimensions parameter
    if req.dimensions == 0 {
        return Err(ApiError::BadRequest("Dimensions must be at least 1".to_string()));
    }
    if req.dimensions > 512 {
        return Err(ApiError::BadRequest("Dimensions cannot exceed 512 (too expensive)".to_string()));
    }
    
    let embedding_count = state.engine.recompute_embeddings(req.dimensions).await?;

    Ok(Json(RecomputeEmbeddingsResponse { embedding_count }))
}

/// POST /maintenance/recompute-node2vec — Recompute Node2Vec embeddings from current graph
/// POST /maintenance/recompute-node2vec — Recompute Node2Vec embeddings from current graph
async fn recompute_node2vec(
    State(state): State<ApiState>,
    Json(req): Json<RecomputeNode2VecRequest>,
) -> Result<Json<RecomputeNode2VecResponse>, ApiError> {
    use crate::embeddings::node2vec;
    
    // Validate parameters
    if req.dimensions == 0 {
        return Err(ApiError::BadRequest("Dimensions must be at least 1".to_string()));
    }
    if req.dimensions > 512 {
        return Err(ApiError::BadRequest("Dimensions cannot exceed 512 (too expensive)".to_string()));
    }
    if req.walk_length == 0 {
        return Err(ApiError::BadRequest("Walk length must be at least 1".to_string()));
    }
    if req.walks_per_node == 0 {
        return Err(ApiError::BadRequest("Walks per node must be at least 1".to_string()));
    }
    if req.p <= 0.0 {
        return Err(ApiError::BadRequest("Parameter p must be positive".to_string()));
    }
    if req.q <= 0.0 {
        return Err(ApiError::BadRequest("Parameter q must be positive".to_string()));
    }
    if req.window == 0 {
        return Err(ApiError::BadRequest("Window size must be at least 1".to_string()));
    }
    if req.epochs == 0 {
        return Err(ApiError::BadRequest("Epochs must be at least 1".to_string()));
    }
    if req.learning_rate <= 0.0 {
        return Err(ApiError::BadRequest("Learning rate must be positive".to_string()));
    }
    
    // Build config from request
    let config = node2vec::Node2VecConfig {
        dimensions: req.dimensions,
        walk_length: req.walk_length,
        walks_per_node: req.walks_per_node,
        p: req.p,
        q: req.q,
        window: req.window,
        epochs: req.epochs,
        learning_rate: req.learning_rate,
    };
    
    let embedding_count = state.engine.recompute_node2vec(config).await?;

    Ok(Json(RecomputeNode2VecResponse { embedding_count }))
}

/// POST /maintenance/reinforce — Create edges based on co-access patterns (stigmergy)
async fn trigger_stigmergy_reinforcement(
    State(state): State<ApiState>,
) -> Result<Json<ReinforceResponse>, ApiError> {
    let edges_created = state.engine.run_stigmergy_reinforcement().await?;

    Ok(Json(ReinforceResponse { edges_created }))
}

/// POST /context — Assemble structured context for a query
async fn assemble_context(
    State(state): State<ApiState>,
    Json(req): Json<ContextRequest>,
) -> Result<Json<ContextResponse>, ApiError> {
    use crate::context::{ContextAssembler, AssemblyConfig, ContextFormat};

    // Validate parameters
    if req.max_triples == 0 {
        return Err(ApiError::BadRequest("max_triples must be at least 1".to_string()));
    }
    if req.max_triples > 1000 {
        return Err(ApiError::BadRequest("max_triples cannot exceed 1000 (too expensive)".to_string()));
    }

    // Parse format
    let format = match req.format.to_lowercase().as_str() {
        "plain" => ContextFormat::Plain,
        "markdown" | "md" => ContextFormat::Markdown,
        "json" => ContextFormat::Json,
        _ => return Err(ApiError::BadRequest(format!("Invalid format: {}. Must be one of: plain, markdown, json", req.format))),
    };

    // Build config
    let config = AssemblyConfig {
        max_triples: req.max_triples,
        max_nodes: req.max_triples * 2, // Reasonable node limit
        include_confidence: true,
        include_sources: false,
        format,
        fusion_config: req.fusion_config,
    };

    // Assemble context
    let assembler = ContextAssembler::new(&state.engine);
    let context = assembler.assemble(&req.query, config).await?;

    Ok(Json(ContextResponse {
        context: context.formatted,
        triple_count: context.triples.len(),
        node_count: context.nodes.len(),
        total_relevance: context.total_score,
    }))
}

/// POST /maintenance/lifecycle — Run a full lifecycle cycle (decay + bounds enforcement)
async fn run_lifecycle_cycle(
    State(state): State<ApiState>,
    Json(_req): Json<LifecycleRequest>,
) -> Result<Json<LifecycleResponse>, ApiError> {
    // Run the full lifecycle cycle
    let (decay_result, enforce_result) = state.engine.run_lifecycle_cycle().await?;
    
    Ok(Json(LifecycleResponse {
        decay: DecayCycleResponse {
            triples_decayed: decay_result.triples_decayed,
            triples_evicted: decay_result.triples_evicted,
            total_weight_before: decay_result.total_weight_before,
            total_weight_after: decay_result.total_weight_after,
        },
        bounds: EnforceResponse {
            triples_evicted: enforce_result.triples_evicted,
            nodes_removed: enforce_result.nodes_removed,
            final_triple_count: enforce_result.final_triple_count,
            final_node_count: enforce_result.final_node_count,
            target_reached: enforce_result.target_reached,
        },
    }))
}

/// GET /stats/lifecycle — Get lifecycle status (bounds and utilization)
async fn get_lifecycle_status(
    State(state): State<ApiState>,
) -> Result<Json<LifecycleStatusResponse>, ApiError> {
    let status = state.engine.lifecycle_status().await?;
    
    Ok(Json(LifecycleStatusResponse {
        current_triples: status.current_triples,
        current_nodes: status.current_nodes,
        max_triples: status.max_triples,
        max_nodes: status.max_nodes,
        triples_exceeded: status.triples_exceeded,
        nodes_exceeded: status.nodes_exceeded,
        utilization: status.utilization,
    }))
}

/// POST /query/combined — Connected AND similar: the killer query
async fn combined_query(
    State(state): State<ApiState>,
    Json(params): Json<crate::query::combined::CombinedQueryParams>,
) -> Result<Json<crate::query::combined::CombinedQueryResponse>, ApiError> {
    let response = state.engine.combined_query(params).await?;
    Ok(Json(response))
}

/// API error types
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err)
    }
}

impl From<crate::ValenceError> for ApiError {
    fn from(err: crate::ValenceError) -> Self {
        ApiError::Internal(anyhow::anyhow!(err))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        let body = Json(serde_json::json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

// ========== Inference Training Loop Endpoints ==========

/// POST /inference/feedback — Submit usage feedback for an assembled context
async fn submit_feedback(
    State(state): State<ApiState>,
    Json(req): Json<SubmitFeedbackRequest>,
) -> Result<Json<SubmitFeedbackResponse>, ApiError> {
    use crate::inference::{UsageFeedback, FeedbackSignal, WeightAdjuster};
    use crate::inference::feedback::TripleFeedback;
    use uuid::Uuid;
    use std::str::FromStr;

    // Convert API types to internal types
    let triple_feedbacks: Result<Vec<TripleFeedback>, ApiError> = req
        .triples
        .iter()
        .map(|tf| {
            let triple_id = Uuid::from_str(&tf.triple_id)
                .map_err(|e| ApiError::BadRequest(format!("Invalid triple ID: {}", e)))?;
            
            let signal = match tf.signal {
                FeedbackSignalType::Cited => FeedbackSignal::Cited,
                FeedbackSignalType::Relevant => FeedbackSignal::Relevant,
                FeedbackSignalType::Ignored => FeedbackSignal::Ignored,
                FeedbackSignalType::Misleading => FeedbackSignal::Misleading,
            };

            Ok(TripleFeedback { triple_id, signal })
        })
        .collect();

    let triple_feedbacks = triple_feedbacks?;

    // Create usage feedback
    let feedback = if let Some(quality) = req.context_quality {
        UsageFeedback::with_quality(req.context_id, triple_feedbacks, quality)
    } else {
        UsageFeedback::new(req.context_id, triple_feedbacks)
    };

    let feedback_id = feedback.id;

    // Record feedback in the engine's feedback recorder
    if let Some(recorder) = state.engine.feedback_recorder() {
        recorder.record(feedback.clone()).await;
    }

    // Apply feedback via weight adjuster
    let adjuster = state.engine.weight_adjuster().ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("Weight adjuster not initialized"))
    })?;

    let summary = adjuster.apply_feedback(&feedback).await?;

    Ok(Json(SubmitFeedbackResponse {
        feedback_id: feedback_id.to_string(),
        adjusted_count: summary.success_count(),
        error_count: summary.error_count(),
        avg_weight_change: summary.average_weight_change(),
        stigmergy_updated: summary.stigmergy_updated,
    }))
}

/// GET /inference/stats — Get feedback statistics for a triple
async fn get_feedback_stats(
    State(state): State<ApiState>,
    Query(params): Query<FeedbackStatsParams>,
) -> Result<Json<FeedbackStatsResponse>, ApiError> {
    use uuid::Uuid;
    use std::str::FromStr;

    let triple_id = Uuid::from_str(&params.triple_id)
        .map_err(|e| ApiError::BadRequest(format!("Invalid triple ID: {}", e)))?;

    let recorder = state.engine.feedback_recorder().ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("Feedback recorder not initialized"))
    })?;

    let stats = recorder.get_triple_stats(triple_id).await;

    // Convert HashMap<FeedbackSignal, usize> to HashMap<String, usize>
    let signal_counts: std::collections::HashMap<String, usize> = stats
        .iter()
        .map(|(signal, count)| {
            let signal_str = match signal {
                crate::inference::FeedbackSignal::Cited => "cited",
                crate::inference::FeedbackSignal::Relevant => "relevant",
                crate::inference::FeedbackSignal::Ignored => "ignored",
                crate::inference::FeedbackSignal::Misleading => "misleading",
            };
            (signal_str.to_string(), *count)
        })
        .collect();

    let total_feedback_count: usize = signal_counts.values().sum();

    Ok(Json(FeedbackStatsResponse {
        triple_id: params.triple_id,
        signal_counts,
        total_feedback_count,
    }))
}

// ========== VKB Endpoints ==========

/// Helper to get the session store or return an error.
fn get_session_store(state: &ApiState) -> Result<&std::sync::Arc<tokio::sync::RwLock<crate::vkb::memory::MemorySessionStore>>, ApiError> {
    state.engine.session_store.as_ref()
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("VKB session store not configured")))
}

/// Convert a Session model to a SessionResponse.
fn session_to_response(s: &crate::vkb::models::Session) -> types::SessionResponse {
    types::SessionResponse {
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

/// POST /sessions — Create a new session
async fn create_session(
    State(state): State<ApiState>,
    Json(req): Json<types::SessionStartRequest>,
) -> Result<Json<types::SessionStartResponse>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;

    let mut session = crate::vkb::models::Session::new(req.platform);
    session.project_context = req.project_context;
    session.external_room_id = req.external_room_id;
    if let Some(metadata) = req.metadata {
        session.metadata = metadata;
    }

    let id = SessionStore::create_session(&*store, session.clone()).await?;

    Ok(Json(types::SessionStartResponse {
        id: id.to_string(),
        status: "active".to_string(),
        created_at: session.created_at,
    }))
}

/// GET /sessions — List sessions
async fn list_sessions(
    State(state): State<ApiState>,
    Query(params): Query<types::SessionListParams>,
) -> Result<Json<Vec<types::SessionResponse>>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;

    let sessions = SessionStore::list_sessions(
        &*store,
        params.status,
        None,
        None,
        params.limit.unwrap_or(20),
    ).await?;

    Ok(Json(sessions.iter().map(session_to_response).collect()))
}

/// GET /sessions/:id — Get a session by ID
async fn get_session(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<types::SessionResponse>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let uuid = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    let session = SessionStore::get_session(&*store, uuid).await?
        .ok_or_else(|| ApiError::NotFound(format!("Session {} not found", id)))?;

    Ok(Json(session_to_response(&session)))
}

/// POST /sessions/:id/end — End a session
async fn end_session(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<types::SessionEndRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let uuid = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    let status = req.status.unwrap_or(crate::vkb::models::SessionStatus::Completed);
    SessionStore::end_session(&*store, uuid, status, req.summary, req.themes).await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "status": format!("{:?}", status).to_lowercase(),
    })))
}

/// GET /sessions/room/:room_id — Find session by room ID
async fn find_session_by_room(
    State(state): State<ApiState>,
    Path(room_id): Path<String>,
) -> Result<Json<types::SessionResponse>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;

    let session = SessionStore::find_session_by_room(&*store, &room_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("No session found for room {}", room_id)))?;

    Ok(Json(session_to_response(&session)))
}

/// POST /sessions/:id/exchanges — Add an exchange to a session
async fn add_exchange(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<types::ExchangeAddRequest>,
) -> Result<Json<types::ExchangeResponse>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let session_id = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    let role = match req.role.to_lowercase().as_str() {
        "user" => crate::vkb::models::ExchangeRole::User,
        "assistant" => crate::vkb::models::ExchangeRole::Assistant,
        "system" => crate::vkb::models::ExchangeRole::System,
        _ => return Err(ApiError::BadRequest(format!("Invalid role: {}", req.role))),
    };

    let mut exchange = crate::vkb::models::Exchange::new(session_id, role, &req.content);
    exchange.tokens_approx = req.tokens_approx;

    let exchange_id = SessionStore::add_exchange(&*store, exchange.clone()).await?;

    Ok(Json(types::ExchangeResponse {
        id: exchange_id.to_string(),
        session_id: session_id.to_string(),
        role: req.role,
        content: req.content,
        created_at: exchange.created_at,
    }))
}

/// GET /sessions/:id/exchanges — List exchanges for a session
async fn list_exchanges(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(params): Query<types::ExchangeListParams>,
) -> Result<Json<Vec<types::ExchangeResponse>>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let session_id = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    let exchanges = SessionStore::list_exchanges(
        &*store,
        session_id,
        params.limit.unwrap_or(20),
        params.offset.unwrap_or(0),
    ).await?;

    Ok(Json(exchanges.iter().map(|e| types::ExchangeResponse {
        id: e.id.to_string(),
        session_id: e.session_id.to_string(),
        role: format!("{:?}", e.role).to_lowercase(),
        content: e.content.clone(),
        created_at: e.created_at,
    }).collect()))
}

/// POST /patterns — Record a new pattern
async fn record_pattern(
    State(state): State<ApiState>,
    Json(req): Json<types::PatternRecordRequest>,
) -> Result<Json<types::PatternRecordResponse>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;

    let mut pattern = crate::vkb::models::Pattern::new(&req.pattern_type, &req.description);
    if let Some(conf) = req.confidence {
        pattern.confidence = conf;
    }
    if let Some(evidence) = req.evidence {
        pattern.evidence_session_ids = evidence.iter()
            .filter_map(|s| s.parse::<Uuid>().ok())
            .collect();
    }

    let id = SessionStore::record_pattern(&*store, pattern).await?;

    Ok(Json(types::PatternRecordResponse {
        id: id.to_string(),
    }))
}

/// POST /patterns/:id/reinforce — Reinforce a pattern
async fn reinforce_pattern(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let pattern_id = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    SessionStore::reinforce_pattern(&*store, pattern_id, None).await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "reinforced": true,
    })))
}

/// GET /patterns — List patterns
async fn list_patterns(
    State(state): State<ApiState>,
    Query(params): Query<types::PatternListParams>,
) -> Result<Json<Vec<types::PatternResponse>>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;

    let patterns = SessionStore::list_patterns(
        &*store,
        params.status.as_deref(),
        params.pattern_type.as_deref(),
        params.limit.unwrap_or(20),
    ).await?;

    Ok(Json(patterns.iter().map(pattern_to_response).collect()))
}

/// GET /patterns/search — Search patterns
async fn search_patterns(
    State(state): State<ApiState>,
    Query(params): Query<types::PatternSearchParams>,
) -> Result<Json<Vec<types::PatternResponse>>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;

    let patterns = SessionStore::search_patterns(
        &*store,
        &params.q,
        params.limit.unwrap_or(20),
    ).await?;

    Ok(Json(patterns.iter().map(pattern_to_response).collect()))
}

/// Convert a Pattern model to a PatternResponse.
fn pattern_to_response(p: &crate::vkb::models::Pattern) -> types::PatternResponse {
    types::PatternResponse {
        id: p.id.to_string(),
        pattern_type: p.pattern_type.clone(),
        description: p.description.clone(),
        status: format!("{:?}", p.status).to_lowercase(),
        confidence: p.confidence,
        evidence_count: p.evidence_session_ids.len() as i32,
        reinforcement_count: 0, // Pattern model doesn't track this separately
        created_at: p.created_at,
        last_seen: p.updated_at,
    }
}

/// POST /sessions/:id/insights — Extract an insight from a session
async fn extract_insight(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<types::InsightExtractRequest>,
) -> Result<Json<types::InsightResponse>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let session_id = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    let mut insight = crate::vkb::models::Insight::new(session_id, &req.content);
    if let Some(domain_path) = req.domain_path {
        insight.domain_path = domain_path;
    }

    let insight_id = SessionStore::extract_insight(&*store, insight.clone()).await?;

    Ok(Json(types::InsightResponse {
        id: insight_id.to_string(),
        session_id: session_id.to_string(),
        content: req.content,
        confidence: req.confidence.unwrap_or(0.8),
        created_at: insight.created_at,
    }))
}

/// GET /sessions/:id/insights — List insights for a session
async fn list_insights(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<types::InsightResponse>>, ApiError> {
    let store = get_session_store(&state)?;
    let store = store.read().await;
    let session_id = id.parse::<Uuid>().map_err(|e| ApiError::BadRequest(format!("Invalid UUID: {}", e)))?;

    let insights = SessionStore::list_insights(&*store, session_id).await?;

    Ok(Json(insights.iter().map(|i| types::InsightResponse {
        id: i.id.to_string(),
        session_id: i.session_id.to_string(),
        content: i.content.clone(),
        confidence: 0.8, // Insight model doesn't have confidence field
        created_at: i.created_at,
    }).collect()))
}

// ========== Trust Endpoints ==========

/// GET /trust?did=X — Query trust score using PageRank
async fn query_trust(
    State(state): State<ApiState>,
    Query(params): Query<types::TrustQueryParams>,
) -> Result<Json<types::TrustQueryResponse>, ApiError> {
    use crate::graph::{GraphView, algorithms::pagerank};

    // Build graph view
    let graph = GraphView::from_store(&*state.engine.store).await?;

    // Run PageRank on the graph
    let ranks = pagerank(&graph, 0.85, 50);

    // Find the DID node
    let did_node = state.engine.store.find_node_by_value(&params.did).await?
        .ok_or_else(|| ApiError::NotFound(format!("DID not found: {}", params.did)))?;

    let trust_score = ranks.get(&did_node.id).copied().unwrap_or(0.0);

    // Get connected DIDs (nodes with "trusts" edges)
    let pattern = TriplePattern {
        subject: Some(did_node.id),
        predicate: Some(crate::predicates::TRUSTS.to_string()),
        object: None,
    };
    let triples = state.engine.store.query_triples(pattern).await?;

    let mut connected_dids = Vec::new();
    for triple in triples {
        let object_node = state.engine.store.get_node(triple.object).await?
            .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Object node not found")))?;
        let score = ranks.get(&object_node.id).copied().unwrap_or(0.0);
        connected_dids.push(types::TrustedEntity {
            did: object_node.value,
            trust_score: score,
        });
    }

    Ok(Json(types::TrustQueryResponse {
        did: params.did,
        trust_score,
        connected_dids,
    }))
}

// ========== Knowledge Management Endpoints ==========

/// GET /triples/:id — Get a single triple with full details
async fn get_triple_detail(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<TripleResponse>, ApiError> {
    let triple_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid triple ID: {}", id)))?;

    let triple = state.engine.store.get_triple(triple_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Triple not found: {}", id)))?;

    let subject_node = state.engine.store.get_node(triple.subject).await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Subject node not found: {:?}", triple.subject)))?;
    let object_node = state.engine.store.get_node(triple.object).await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("Object node not found: {:?}", triple.object)))?;

    let sources = state.engine.store.get_sources_for_triple(triple.id).await?;
    let source_responses: Vec<SourceResponse> = sources
        .into_iter()
        .map(|s| SourceResponse {
            id: s.id.to_string(),
            source_type: s.source_type,
            reference: s.reference,
            created_at: s.created_at,
        })
        .collect();

    Ok(Json(TripleResponse {
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
    }))
}

/// GET /triples/:id/confidence — Explain dynamic confidence score for a triple
async fn explain_confidence(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(params): Query<types::ConfidenceExplainParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let triple_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid triple ID: {}", id)))?;

    // Verify triple exists
    let triple = state.engine.store.get_triple(triple_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Triple not found: {}", id)))?;

    // Resolve optional query context node
    let query_context = if let Some(ref ctx) = params.context {
        state.engine.store.find_node_by_value(ctx).await?.map(|n| n.id)
    } else {
        None
    };

    // Build graph view and compute confidence
    let graph_view = GraphView::from_store(&*state.engine.store).await?;
    let score = DynamicConfidence::compute_confidence(
        &*state.engine.store,
        &graph_view,
        triple_id,
        query_context,
    ).await?;

    // Get source count for the breakdown
    let sources = state.engine.store.get_sources_for_triple(triple_id).await?;

    // Get subject/object node values
    let subject_node = state.engine.store.get_node(triple.subject).await?;
    let object_node = state.engine.store.get_node(triple.object).await?;

    Ok(Json(serde_json::json!({
        "triple_id": id,
        "subject": subject_node.map(|n| n.value),
        "predicate": triple.predicate.value,
        "object": object_node.map(|n| n.value),
        "confidence": {
            "combined": score.combined,
            "source_reliability": score.source_reliability,
            "path_diversity": score.path_diversity,
            "centrality": score.centrality,
        },
        "weights": {
            "source_reliability": 0.5,
            "path_diversity": 0.3,
            "centrality": 0.2,
        },
        "details": {
            "source_count": sources.len(),
            "query_context": params.context,
        }
    })))
}

/// POST /triples/:id/supersede — Supersede a triple with a new one
async fn supersede_triple(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<types::SupersedeTripleRequest>,
) -> Result<Json<InsertTriplesResponse>, ApiError> {
    let old_triple_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid triple ID: {}", id)))?;

    // Verify old triple exists
    state.engine.store.get_triple(old_triple_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Triple not found: {}", id)))?;

    // Create new triple
    let subject_node = state.engine.store.find_or_create_node(&req.new_subject).await?;
    let object_node = state.engine.store.find_or_create_node(&req.new_object).await?;
    let new_triple = Triple::new(subject_node.id, &req.new_predicate, object_node.id);
    let new_triple_id = state.engine.store.insert_triple(new_triple).await?;

    // Create a "supersedes" edge from new to old
    let supersedes_node = state.engine.store.find_or_create_node(&old_triple_id.to_string()).await?;
    let new_triple_node = state.engine.store.find_or_create_node(&new_triple_id.to_string()).await?;
    let supersedes_triple = Triple::new(new_triple_node.id, "supersedes", supersedes_node.id);
    state.engine.store.insert_triple(supersedes_triple).await?;

    // Insert source if provided
    let source_id = if let Some(source_req) = &req.source {
        let source = Source::new(vec![new_triple_id], source_req.source_type.clone());
        let source = if let Some(ref reference) = source_req.reference {
            source.with_reference(reference)
        } else {
            source
        };
        let source_id = state.engine.store.insert_source(source).await?;
        Some(source_id)
    } else {
        None
    };

    Ok(Json(InsertTriplesResponse {
        triple_ids: vec![new_triple_id.to_string()],
        source_id: source_id.map(|id| id.to_string()),
    }))
}

/// GET /nodes/search — Search nodes by value substring (case-insensitive)
async fn search_nodes(
    State(state): State<ApiState>,
    Query(params): Query<types::NodeSearchParams>,
) -> Result<Json<Vec<NodeResponse>>, ApiError> {
    let limit = params.limit.unwrap_or(20) as usize;
    let nodes = state.engine.store.search_nodes(&params.q, limit).await?;
    let results: Vec<NodeResponse> = nodes
        .into_iter()
        .map(|n| NodeResponse {
            id: n.id.to_string(),
            value: n.value,
        })
        .collect();
    Ok(Json(results))
}

/// POST /triples/:id/sign — Sign a triple with the local keypair
async fn sign_triple(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<types::SignTripleResponse>, ApiError> {
    let triple_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid triple ID: {}", id)))?;

    let triple = state.engine.store.get_triple(triple_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Triple not found: {}", id)))?;

    // Create message to sign: triple_id bytes
    let message = triple_id.as_bytes();
    let signature = state.engine.keypair.sign(message);
    let signature_b64 = base64::engine::general_purpose::STANDARD.encode(signature);

    Ok(Json(types::SignTripleResponse {
        triple_id: id,
        signature: signature_b64,
        signer_did: state.engine.keypair.did_string(),
    }))
}

/// GET /triples/:id/verify — Verify a triple's signature
async fn verify_triple(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<types::VerifyTripleResponse>, ApiError> {
    use crate::identity::Keypair;

    let triple_id = Uuid::parse_str(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid triple ID: {}", id)))?;

    let triple = state.engine.store.get_triple(triple_id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Triple not found: {}", id)))?;

    let valid = if let (Some(origin_did), Some(sig_b64)) = (&triple.origin_did, &triple.signature) {
        // Parse DID to get public key
        // did:valence:key:<base58-pubkey>
        if let Some(key_part) = origin_did.strip_prefix("did:valence:key:") {
            let pubkey_bytes = bs58::decode(key_part).into_vec()
                .map_err(|_| ApiError::Internal(anyhow::anyhow!("Invalid base58 in DID")))?;
            if pubkey_bytes.len() != 32 {
                return Ok(Json(types::VerifyTripleResponse {
                    triple_id: id,
                    valid: false,
                    origin_did: Some(origin_did.clone()),
                }));
            }
            let mut pubkey_arr = [0u8; 32];
            pubkey_arr.copy_from_slice(&pubkey_bytes);

            // Decode signature
            let sig_bytes = base64::engine::general_purpose::STANDARD.decode(sig_b64)
                .map_err(|_| ApiError::Internal(anyhow::anyhow!("Invalid base64 signature")))?;
            if sig_bytes.len() != 64 {
                return Ok(Json(types::VerifyTripleResponse {
                    triple_id: id,
                    valid: false,
                    origin_did: Some(origin_did.clone()),
                }));
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

    Ok(Json(types::VerifyTripleResponse {
        triple_id: id,
        valid,
        origin_did: triple.origin_did.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SourceType;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn test_insert_and_query_triples() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine);

        // Insert triples
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "lives_in".to_string(),
                    object: "NYC".to_string(),
                },
            ],
            source: Some(SourceInput {
                source_type: SourceType::UserInput,
                reference: Some("test-session".to_string()),
            }),
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Query by subject
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/triples?subject=Alice")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let query_response: QueryTriplesResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(query_response.triples.len(), 2);
        assert_eq!(query_response.triples[0].subject.value, "Alice");
    }

    #[tokio::test]
    async fn test_neighbors() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine);

        // Insert a chain: Alice -> knows -> Bob -> knows -> Carol
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Bob".to_string(),
                    predicate: "knows".to_string(),
                    object: "Carol".to_string(),
                },
            ],
            source: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Get neighbors of Alice with depth 1
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/nodes/Alice/neighbors?depth=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let neighbors: NeighborsResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(neighbors.triple_count, 1); // Only Alice -> Bob

        // Get neighbors of Alice with depth 2
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/nodes/Alice/neighbors?depth=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let neighbors: NeighborsResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(neighbors.triple_count, 2); // Alice -> Bob -> Carol
    }

    #[tokio::test]
    async fn test_stats() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine);

        // Insert some triples
        let insert_req = InsertTriplesRequest {
            triples: vec![TripleInput {
                subject: "A".to_string(),
                predicate: "rel".to_string(),
                object: "B".to_string(),
            }],
            source: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Get stats
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let stats: StatsResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(stats.triple_count, 1);
        assert_eq!(stats.node_count, 2); // A and B
        assert_eq!(stats.avg_weight, 1.0);
    }

    #[tokio::test]
    async fn test_decay_and_evict() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine);

        // Insert a triple
        let insert_req = InsertTriplesRequest {
            triples: vec![TripleInput {
                subject: "A".to_string(),
                predicate: "rel".to_string(),
                object: "B".to_string(),
            }],
            source: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Decay
        let decay_req = DecayRequest {
            factor: 0.5,
            min_weight: 0.0,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/decay")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&decay_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let decay_resp: DecayResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(decay_resp.affected_count, 1);

        // Evict below 0.6 (should remove the triple since it's at 0.5)
        let evict_req = EvictRequest { threshold: 0.6 };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/evict")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&evict_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let evict_resp: EvictResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(evict_resp.evicted_count, 1);

        // Verify stats show 0 triples
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let stats: StatsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(stats.triple_count, 0);
    }

    #[tokio::test]
    async fn test_sources() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine.clone());

        // Insert with source
        let insert_req = InsertTriplesRequest {
            triples: vec![TripleInput {
                subject: "A".to_string(),
                predicate: "rel".to_string(),
                object: "B".to_string(),
            }],
            source: Some(SourceInput {
                source_type: SourceType::Conversation,
                reference: Some("session-123".to_string()),
            }),
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let insert_resp: InsertTriplesResponse = serde_json::from_slice(&body).unwrap();
        let triple_id = &insert_resp.triple_ids[0];

        // Get sources for triple
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/triples/{}/sources", triple_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let sources_resp: SourcesResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(sources_resp.sources.len(), 1);
        assert_eq!(sources_resp.sources[0].source_type, SourceType::Conversation);
        assert_eq!(
            sources_resp.sources[0].reference.as_deref(),
            Some("session-123")
        );
    }

    #[tokio::test]
    async fn test_recompute_embeddings() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine.clone());

        // Insert some triples to build a graph
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Bob".to_string(),
                    predicate: "knows".to_string(),
                    object: "Carol".to_string(),
                },
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "works_with".to_string(),
                    object: "Carol".to_string(),
                },
            ],
            source: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Recompute embeddings
        let recompute_req = RecomputeEmbeddingsRequest { dimensions: 2 };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/recompute-embeddings")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&recompute_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let recompute_resp: RecomputeEmbeddingsResponse = serde_json::from_slice(&body).unwrap();

        // Should have embeddings for 3 nodes
        assert_eq!(recompute_resp.embedding_count, 3);
    }

    #[tokio::test]
    async fn test_recompute_node2vec() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine.clone());

        // Insert triples
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Bob".to_string(),
                    predicate: "knows".to_string(),
                    object: "Carol".to_string(),
                },
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "works_with".to_string(),
                    object: "Carol".to_string(),
                },
            ],
            source: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Recompute Node2Vec embeddings with custom config
        let recompute_req = RecomputeNode2VecRequest {
            dimensions: 8,
            walk_length: 10,
            walks_per_node: 5,
            p: 1.0,
            q: 1.0,
            window: 3,
            epochs: 3,
            learning_rate: 0.025,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/recompute-node2vec")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&recompute_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let recompute_resp: RecomputeNode2VecResponse = serde_json::from_slice(&body).unwrap();

        // Should have embeddings for 3 nodes
        assert_eq!(recompute_resp.embedding_count, 3);
    }

    #[tokio::test]
    async fn test_search() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine.clone());

        // Insert triples
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Bob".to_string(),
                    predicate: "knows".to_string(),
                    object: "Carol".to_string(),
                },
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "works_with".to_string(),
                    object: "Carol".to_string(),
                },
                TripleInput {
                    subject: "Dave".to_string(),
                    predicate: "knows".to_string(),
                    object: "Eve".to_string(),
                },
            ],
            source: None,
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Recompute embeddings first
        let recompute_req = RecomputeEmbeddingsRequest { dimensions: 4 };
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/recompute-embeddings")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&recompute_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Search for nodes similar to Alice
        let search_req = SearchRequest {
            query_node: "Alice".to_string(),
            k: 3,
            include_confidence: false,
            use_tiered: false,
            budget_ms: None,
            confidence_threshold: None,
            fusion_config: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&search_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let search_resp: SearchResponse = serde_json::from_slice(&body).unwrap();

        // Should return up to 3 results
        assert!(search_resp.results.len() <= 3);
        assert!(!search_resp.results.is_empty());

        // First result should be Alice itself (highest similarity)
        assert_eq!(search_resp.results[0].value, "Alice");
        assert!(search_resp.results[0].similarity > 0.99); // Near 1.0

        // Results should be sorted by similarity descending
        for i in 1..search_resp.results.len() {
            assert!(search_resp.results[i - 1].similarity >= search_resp.results[i].similarity);
        }

        // Standard search should not include tiered metadata
        assert!(search_resp.tier_reached.is_none());
        assert!(search_resp.time_ms.is_none());
        assert!(search_resp.budget_exhausted.is_none());
    }

    #[tokio::test]
    async fn test_search_with_confidence() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine.clone());

        // Insert triples with sources
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Bob".to_string(),
                    predicate: "knows".to_string(),
                    object: "Carol".to_string(),
                },
            ],
            source: Some(SourceInput {
                source_type: SourceType::Conversation,
                reference: Some("test".to_string()),
            }),
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Recompute embeddings
        let recompute_req = RecomputeEmbeddingsRequest { dimensions: 2 };
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/recompute-embeddings")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&recompute_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Search with confidence
        let search_req = SearchRequest {
            query_node: "Alice".to_string(),
            k: 2,
            include_confidence: true,
            use_tiered: false,
            budget_ms: None,
            confidence_threshold: None,
            fusion_config: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&search_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let search_resp: SearchResponse = serde_json::from_slice(&body).unwrap();

        // Should have confidence scores
        for result in &search_resp.results {
            assert!(result.confidence.is_some());
        }
    }

    #[tokio::test]
    async fn test_tiered_search() {
        let engine = crate::engine::ValenceEngine::new();
        let app = create_router(engine.clone());

        // Insert triples
        let insert_req = InsertTriplesRequest {
            triples: vec![
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "knows".to_string(),
                    object: "Bob".to_string(),
                },
                TripleInput {
                    subject: "Bob".to_string(),
                    predicate: "knows".to_string(),
                    object: "Carol".to_string(),
                },
                TripleInput {
                    subject: "Alice".to_string(),
                    predicate: "works_with".to_string(),
                    object: "Carol".to_string(),
                },
            ],
            source: Some(SourceInput {
                source_type: SourceType::Conversation,
                reference: Some("test".to_string()),
            }),
        };

        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/triples")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&insert_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Recompute embeddings
        let recompute_req = RecomputeEmbeddingsRequest { dimensions: 4 };
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/maintenance/recompute-embeddings")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&recompute_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Tiered search with budget
        let search_req = SearchRequest {
            query_node: "Alice".to_string(),
            k: 3,
            include_confidence: false,
            use_tiered: true,
            budget_ms: Some(1000),
            confidence_threshold: Some(0.8),
            fusion_config: None,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&search_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let search_resp: SearchResponse = serde_json::from_slice(&body).unwrap();

        // Should have results
        assert!(!search_resp.results.is_empty());

        // Should include tiered metadata
        assert!(search_resp.tier_reached.is_some());
        assert!(search_resp.time_ms.is_some());
        assert!(search_resp.budget_exhausted.is_some());

        let tier = search_resp.tier_reached.unwrap();
        assert!(tier >= 1 && tier <= 3);

        // Should complete within budget
        assert!(search_resp.time_ms.unwrap() <= 1000);
    }
}
