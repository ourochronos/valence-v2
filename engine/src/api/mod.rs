use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use uuid::Uuid;

use crate::{
    engine::ValenceEngine,
    embeddings::EmbeddingStore,
    graph::{GraphView, DynamicConfidence},
    models::{Source, Triple},
    storage::{MemoryStore, TriplePattern},
};

mod types;
pub use types::*;

/// API server state - uses ValenceEngine
#[derive(Clone)]
pub struct ApiState {
    pub engine: ValenceEngine,
}

impl ApiState {
    pub fn new(engine: ValenceEngine) -> Self {
        Self { engine }
    }

    /// Backward compatibility: create from a MemoryStore
    pub fn from_store(store: MemoryStore) -> Self {
        Self {
            engine: ValenceEngine::from_store(store),
        }
    }
}

/// Create the API router with all endpoints
pub fn create_router(engine: ValenceEngine) -> Router {
    let state = ApiState::new(engine);

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
        .route("/maintenance/reinforce", post(trigger_stigmergy_reinforcement))
        .with_state(state)
}

/// GET /health — Health check endpoint
async fn health_check(State(state): State<ApiState>) -> Result<Json<serde_json::Value>, ApiError> {
    // Check if store is accessible by counting triples
    let triple_count = state.engine.store.count_triples().await?;
    
    Ok(Json(serde_json::json!({
        "status": "healthy",
        "triple_count": triple_count,
        "has_embeddings": state.engine.has_embeddings().await,
    })))
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
            weight: triple.weight,
            created_at: triple.created_at,
            last_accessed: triple.last_accessed,
            access_count: triple.access_count,
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
            weight: triple.weight,
            created_at: triple.created_at,
            last_accessed: triple.last_accessed,
            access_count: triple.access_count,
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
        triples.iter().take(sample_size).map(|t| t.weight).sum::<f64>() / sample_size as f64
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
    // Validate k parameter (prevent pathological queries)
    if req.k == 0 {
        return Err(ApiError::BadRequest("k must be at least 1".to_string()));
    }
    if req.k > 1000 {
        return Err(ApiError::BadRequest("k cannot exceed 1000 (too expensive)".to_string()));
    }
    
    // Find the query node by value
    let query_node = state
        .engine
        .store
        .find_node_by_value(&req.query_node)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Query node not found: {}", req.query_node)))?;

    // Get the embedding for the query node
    let embeddings_store = state.engine.embeddings.read().await;
    let query_embedding = embeddings_store
        .get(query_node.id)
        .ok_or_else(|| ApiError::NotFound(format!("No embedding found for node: {}", req.query_node)))?;

    // Find k nearest neighbors
    let neighbors = embeddings_store.query_nearest(query_embedding, req.k)?;
    drop(embeddings_store); // Release lock before async operations

    // Build response
    let mut results = Vec::new();

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
            // We'll use the first triple we find (simplified approach)
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
                // No triples found for this node
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

    Ok(Json(SearchResponse { results }))
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

/// POST /maintenance/reinforce — Create edges based on co-access patterns (stigmergy)
async fn trigger_stigmergy_reinforcement(
    State(state): State<ApiState>,
) -> Result<Json<ReinforceResponse>, ApiError> {
    let edges_created = state.engine.run_stigmergy_reinforcement().await?;

    Ok(Json(ReinforceResponse { edges_created }))
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
}
