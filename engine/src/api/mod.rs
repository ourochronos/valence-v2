use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{Source, SourceType, Triple},
    storage::{MemoryStore, TriplePattern, TripleStore},
};

mod types;
pub use types::*;

/// API server state - uses concrete MemoryStore for simplicity
#[derive(Clone)]
pub struct ApiState {
    pub store: Arc<MemoryStore>,
}

impl ApiState {
    pub fn new(store: MemoryStore) -> Self {
        Self {
            store: Arc::new(store),
        }
    }
}

/// Create the API router with all endpoints
pub fn create_router(store: MemoryStore) -> Router {
    let state = ApiState::new(store);

    Router::new()
        // Triple operations
        .route("/triples", post(insert_triples))
        .route("/triples", get(query_triples))
        .route("/triples/{id}/sources", get(get_triple_sources))
        // Node operations
        .route("/nodes/{node}/neighbors", get(get_neighbors))
        // Statistics
        .route("/stats", get(get_stats))
        // Maintenance
        .route("/maintenance/decay", post(trigger_decay))
        .route("/maintenance/evict", post(trigger_evict))
        .with_state(state)
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
            .store
            .find_or_create_node(&triple_req.subject)
            .await?;
        let object_node = state
            .store
            .find_or_create_node(&triple_req.object)
            .await?;

        // Create and insert triple
        let triple = Triple::new(subject_node.id, &triple_req.predicate, object_node.id);
        let triple_id = state.store.insert_triple(triple).await?;
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
        let source_id = state.store.insert_source(source).await?;
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
            .store
            .find_node_by_value(subject_value)
            .await?
            .map(|n| n.id)
    } else {
        None
    };

    let object_id = if let Some(ref object_value) = params.object {
        state
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

    let triples = state.store.query_triples(pattern).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    for triple in triples {
        let subject_node = state.store.get_node(triple.subject).await?.unwrap();
        let object_node = state.store.get_node(triple.object).await?.unwrap();

        let sources = if params.include_sources.unwrap_or(false) {
            let sources = state.store.get_sources_for_triple(triple.id).await?;
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
            .store
            .find_node_by_value(&node)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Node not found: {}", node)))?
            .id
    };

    let depth = params.depth.unwrap_or(1);
    let triples = state.store.neighbors(node_id, depth).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    let mut unique_nodes = std::collections::HashSet::new();

    for triple in &triples {
        let subject_node = state.store.get_node(triple.subject).await?.unwrap();
        let object_node = state.store.get_node(triple.object).await?.unwrap();

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

    let sources = state.store.get_sources_for_triple(triple_id).await?;

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
    let triple_count = state.store.count_triples().await?;
    let node_count = state.store.count_nodes().await?;

    // Calculate average weight
    let pattern = TriplePattern {
        subject: None,
        predicate: None,
        object: None,
    };
    let triples = state.store.query_triples(pattern).await?;
    let avg_weight = if !triples.is_empty() {
        triples.iter().map(|t| t.weight).sum::<f64>() / triples.len() as f64
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
    let affected_count = state.store.decay(req.factor, req.min_weight).await?;

    Ok(Json(DecayResponse { affected_count }))
}

/// POST /maintenance/evict — Remove low-weight triples
async fn trigger_evict(
    State(state): State<ApiState>,
    Json(req): Json<EvictRequest>,
) -> Result<Json<EvictResponse>, ApiError> {
    let evicted_count = state.store.evict_below_weight(req.threshold).await?;

    Ok(Json(EvictResponse { evicted_count }))
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
    use crate::storage::MemoryStore;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn test_insert_and_query_triples() {
        let store = MemoryStore::new();
        let app = create_router(store);

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
        let store = MemoryStore::new();
        let app = create_router(store);

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
        let store = MemoryStore::new();
        let app = create_router(store);

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
        let store = MemoryStore::new();
        let app = create_router(store);

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
        let store = MemoryStore::new();
        let app = create_router(store.clone());

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
                    .uri(&format!("/triples/{}/sources", triple_id))
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
}
