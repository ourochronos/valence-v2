//! Cold Engine Tests
//!
//! Verify that the engine works without embeddings (cold mode) — the engine
//! should gracefully degrade to graph-based operations when embeddings are not
//! available, never panicking or returning 500 errors.

use valence_engine::{
    api::{create_router, ContextRequest, DecayRequest, EvictRequest, InsertTriplesRequest, SearchRequest, TripleInput},
    engine::ValenceEngine,
    models::Triple,
};
use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

/// Helper to convert response body to bytes
async fn body_to_bytes(body: Body) -> Bytes {
    axum::body::to_bytes(body, usize::MAX).await.unwrap()
}

#[tokio::test]
async fn test_cold_engine_insert_triples() {
    // Cold engine should allow inserting triples without any embeddings
    let engine = ValenceEngine::new();
    
    // Insert directly via store (no embeddings)
    let alice = engine.store.find_or_create_node("Alice").await.unwrap();
    let bob = engine.store.find_or_create_node("Bob").await.unwrap();
    let triple = Triple::new(alice.id, "knows", bob.id);
    let triple_id = engine.store.insert_triple(triple).await.unwrap();
    
    assert!(triple_id != uuid::Uuid::nil());
    
    // Verify the triple exists
    let retrieved = engine.store.get_triple(triple_id).await.unwrap();
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_cold_engine_query_triples() {
    // Cold engine should allow querying triples without embeddings
    let engine = ValenceEngine::new();
    let app = create_router(engine.clone());
    
    // Insert triples via API
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
    
    // Query triples without embeddings
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
    
    let body = body_to_bytes(response.into_body()).await;
    let query_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(query_response["triples"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_cold_engine_neighbors() {
    // Cold engine should support graph traversal without embeddings
    let engine = ValenceEngine::new();
    let app = create_router(engine.clone());
    
    // Build a chain
    let insert_req = InsertTriplesRequest {
        triples: vec![
            TripleInput {
                subject: "A".to_string(),
                predicate: "next".to_string(),
                object: "B".to_string(),
            },
            TripleInput {
                subject: "B".to_string(),
                predicate: "next".to_string(),
                object: "C".to_string(),
            },
            TripleInput {
                subject: "C".to_string(),
                predicate: "next".to_string(),
                object: "D".to_string(),
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
    
    // Get neighbors without embeddings
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/nodes/A/neighbors?depth=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = body_to_bytes(response.into_body()).await;
    let neighbors: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should traverse the graph successfully
    assert!(neighbors["triple_count"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn test_cold_engine_decay() {
    // Cold engine should support decay without embeddings
    let engine = ValenceEngine::new();
    let app = create_router(engine.clone());
    
    // Insert triples
    let insert_req = InsertTriplesRequest {
        triples: vec![
            TripleInput {
                subject: "X".to_string(),
                predicate: "rel".to_string(),
                object: "Y".to_string(),
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
    
    // Trigger decay without embeddings
    let decay_req = DecayRequest {
        factor: 0.8,
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
    
    let body = body_to_bytes(response.into_body()).await;
    let decay_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(decay_response["affected_count"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_cold_engine_evict() {
    // Cold engine should support eviction without embeddings
    let engine = ValenceEngine::new();
    let app = create_router(engine.clone());
    
    // Insert triple
    let insert_req = InsertTriplesRequest {
        triples: vec![
            TripleInput {
                subject: "X".to_string(),
                predicate: "rel".to_string(),
                object: "Y".to_string(),
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
    
    // Decay first
    let decay_req = DecayRequest {
        factor: 0.3,
        min_weight: 0.0,
    };
    
    app.clone()
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
    
    // Evict without embeddings
    let evict_req = EvictRequest { threshold: 0.5 };
    
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
    
    let body = body_to_bytes(response.into_body()).await;
    let evict_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(evict_response["evicted_count"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_cold_engine_stats() {
    // Cold engine should provide stats without embeddings
    let engine = ValenceEngine::new();
    let app = create_router(engine.clone());
    
    // Insert some data
    let insert_req = InsertTriplesRequest {
        triples: vec![
            TripleInput {
                subject: "A".to_string(),
                predicate: "rel".to_string(),
                object: "B".to_string(),
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
    
    // Get stats without embeddings
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = body_to_bytes(response.into_body()).await;
    let stats: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(stats["triple_count"].as_u64().unwrap(), 1);
    assert_eq!(stats["node_count"].as_u64().unwrap(), 2);
    assert_eq!(stats["avg_weight"].as_f64().unwrap(), 1.0);
}

#[tokio::test]
async fn test_cold_engine_search_fallback() {
    // Search should fall back to graph-based retrieval when embeddings don't exist
    let engine = ValenceEngine::new();
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
                predicate: "likes".to_string(),
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
    
    // Search WITHOUT computing embeddings (cold mode)
    let search_req = SearchRequest {
        query_node: "Alice".to_string(),
        k: 5,
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
    
    // Should return 200 with degraded results, NOT 500
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = body_to_bytes(response.into_body()).await;
    let search_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should have results via fallback
    let results = search_response["results"].as_array().unwrap();
    assert!(!results.is_empty(), "Cold search should return graph-based results");
    
    // Should have fallback flag
    assert_eq!(search_response["fallback"].as_bool(), Some(true));
    
    // Results should have similarity=0.0 in fallback mode
    for result in results {
        assert_eq!(result["similarity"].as_f64().unwrap(), 0.0);
    }
}

#[tokio::test]
async fn test_cold_engine_context_fallback() {
    // Context assembly should fall back to graph-based assembly when embeddings don't exist
    let engine = ValenceEngine::new();
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
                predicate: "works_at".to_string(),
                object: "Acme".to_string(),
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
    
    // Assemble context WITHOUT embeddings (cold mode)
    let context_req = ContextRequest {
        query: "Alice".to_string(),
        max_triples: 10,
        format: "markdown".to_string(),
        fusion_config: None,
    };
    
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/context")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&context_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    // Should return 200 with graph-based context, NOT 500
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = body_to_bytes(response.into_body()).await;
    let context_response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should have assembled context
    assert!(context_response["triple_count"].as_u64().unwrap() > 0);
    assert!(!context_response["context"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_graceful_degradation_workflow() {
    // Test the full graceful degradation workflow:
    // 1. Engine starts (no embeddings)
    // 2. Queries work in cold mode (graph-based)
    // 3. Embeddings computed
    // 4. Queries get better (warm mode)
    
    let engine = ValenceEngine::new();
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
                predicate: "likes".to_string(),
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
    
    // Step 1 & 2: Search in COLD mode (no embeddings yet)
    let search_req = SearchRequest {
        query_node: "Alice".to_string(),
        k: 3,
        include_confidence: false,
        use_tiered: false,
        budget_ms: None,
        confidence_threshold: None,
        fusion_config: None,
    };
    
    let cold_response = app
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
    
    assert_eq!(cold_response.status(), StatusCode::OK);
    
    let cold_body = body_to_bytes(cold_response.into_body()).await;
    let cold_search: serde_json::Value = serde_json::from_slice(&cold_body).unwrap();
    
    // Verify cold mode characteristics
    assert_eq!(cold_search["fallback"].as_bool(), Some(true));
    assert!(!cold_search["results"].as_array().unwrap().is_empty());
    
    // Step 3: Compute embeddings (transition to warm mode)
    let recompute_req = serde_json::json!({ "dimensions": 4 });
    
    let _embed_response = app
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
    
    // Step 4: Search in WARM mode (with embeddings)
    let warm_response = app
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
    
    assert_eq!(warm_response.status(), StatusCode::OK);
    
    let warm_body = body_to_bytes(warm_response.into_body()).await;
    let warm_search: serde_json::Value = serde_json::from_slice(&warm_body).unwrap();
    
    // Verify warm mode characteristics
    assert_ne!(warm_search["fallback"].as_bool(), Some(true)); // No fallback in warm mode
    assert!(!warm_search["results"].as_array().unwrap().is_empty());
    
    // Warm mode results should have actual similarity scores
    let warm_results = warm_search["results"].as_array().unwrap();
    assert!(warm_results[0]["similarity"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_no_panics_on_missing_embeddings() {
    // Ensure no operation panics when embeddings are missing
    let engine = ValenceEngine::new();
    let app = create_router(engine.clone());
    
    // Insert data
    let insert_req = InsertTriplesRequest {
        triples: vec![
            TripleInput {
                subject: "Node1".to_string(),
                predicate: "connects_to".to_string(),
                object: "Node2".to_string(),
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
    
    // Try all endpoints without embeddings — none should panic or return 500
    
    // 1. Query triples
    let resp = app.clone().oneshot(
        Request::builder()
            .uri("/triples")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    
    // 2. Neighbors
    let resp = app.clone().oneshot(
        Request::builder()
            .uri("/nodes/Node1/neighbors?depth=1")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    
    // 3. Stats
    let resp = app.clone().oneshot(
        Request::builder()
            .uri("/stats")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    
    // 4. Decay
    let decay_req = DecayRequest { factor: 0.9, min_weight: 0.1 };
    let resp = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/maintenance/decay")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&decay_req).unwrap()))
            .unwrap(),
    ).await.unwrap();
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    
    // 5. Evict
    let evict_req = EvictRequest { threshold: 0.5 };
    let resp = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/maintenance/evict")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&evict_req).unwrap()))
            .unwrap(),
    ).await.unwrap();
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    
    // 6. Search (should use fallback)
    let search_req = SearchRequest {
        query_node: "Node1".to_string(),
        k: 3,
        include_confidence: false,
        use_tiered: false,
        budget_ms: None,
        confidence_threshold: None,
        fusion_config: None,
    };
    let resp = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&search_req).unwrap()))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK); // Should succeed with fallback
    
    // 7. Context (should use fallback)
    let context_req = ContextRequest {
        query: "Node1".to_string(),
        max_triples: 10,
        format: "json".to_string(),
        fusion_config: None,
    };
    let resp = app.clone().oneshot(
        Request::builder()
            .method("POST")
            .uri("/context")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&context_req).unwrap()))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK); // Should succeed with fallback
}
