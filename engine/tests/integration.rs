//! Integration tests for Valence Engine
//!
//! These tests start a real HTTP server and test the full API surface.

use reqwest::{Client, StatusCode};
use serde_json::json;
use std::net::TcpListener;
use tokio::task::JoinHandle;
use valence_engine::{api::create_router, engine::ValenceEngine};

/// Helper to find a random available port
fn get_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Helper to start the server on a random port and return the base URL and server handle
async fn start_test_server() -> (String, JoinHandle<()>) {
    let port = get_available_port();
    let engine = ValenceEngine::new();
    let app = create_router(engine);
    let addr = format!("127.0.0.1:{}", port);
    let base_url = format!("http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    (base_url, server_handle)
}

#[tokio::test]
async fn test_insert_triples() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert triples
    let response = client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows",
                    "object": "Bob"
                },
                {
                    "subject": "Alice",
                    "predicate": "lives_in",
                    "object": "NYC"
                }
            ],
            "source": {
                "type": "UserInput",
                "reference": "test-session"
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_ids"].as_array().unwrap().len(), 2);
    assert!(body["source_id"].is_string());
}

#[tokio::test]
async fn test_query_triples() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert test data
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows",
                    "object": "Bob"
                },
                {
                    "subject": "Alice",
                    "predicate": "lives_in",
                    "object": "NYC"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Query by subject
    let response = client
        .get(format!("{}/triples?subject=Alice", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    let triples = body["triples"].as_array().unwrap();
    assert_eq!(triples.len(), 2);
    assert_eq!(triples[0]["subject"]["value"], "Alice");

    // Query by predicate
    let response = client
        .get(format!("{}/triples?predicate=knows", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    let triples = body["triples"].as_array().unwrap();
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0]["predicate"], "knows");

    // Query by object
    let response = client
        .get(format!("{}/triples?object=NYC", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    let triples = body["triples"].as_array().unwrap();
    assert_eq!(triples.len(), 1);
    assert_eq!(triples[0]["object"]["value"], "NYC");
}

#[tokio::test]
async fn test_neighbors() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert a chain: Alice -> knows -> Bob -> knows -> Carol
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows",
                    "object": "Bob"
                },
                {
                    "subject": "Bob",
                    "predicate": "knows",
                    "object": "Carol"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Get neighbors of Alice with depth 1
    let response = client
        .get(format!("{}/nodes/Alice/neighbors?depth=1", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 1); // Only Alice -> Bob

    // Get neighbors of Alice with depth 2
    let response = client
        .get(format!("{}/nodes/Alice/neighbors?depth=2", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 2); // Alice -> Bob -> Carol
    assert_eq!(body["node_count"], 3); // Alice, Bob, Carol
}

#[tokio::test]
async fn test_stats() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Initial stats (empty)
    let response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 0);
    assert_eq!(body["node_count"], 0);

    // Insert some triples
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                },
                {
                    "subject": "B",
                    "predicate": "rel",
                    "object": "C"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Check updated stats
    let response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 2);
    assert_eq!(body["node_count"], 3); // A, B, C
    assert_eq!(body["avg_weight"], 1.0); // All start at 1.0
}

#[tokio::test]
async fn test_decay() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert a triple
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Decay by 50%
    let response = client
        .post(format!("{}/maintenance/decay", base_url))
        .json(&json!({
            "factor": 0.5,
            "min_weight": 0.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["affected_count"], 1);

    // Check that weight was updated
    let response = client
        .get(format!("{}/triples", base_url))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    let triples = body["triples"].as_array().unwrap();
    assert_eq!(triples[0]["weight"], 0.5);
}

#[tokio::test]
async fn test_evict() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert two triples
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                },
                {
                    "subject": "C",
                    "predicate": "rel",
                    "object": "D"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Decay first triple below threshold
    client
        .post(format!("{}/maintenance/decay", base_url))
        .json(&json!({
            "factor": 0.3,
            "min_weight": 0.0
        }))
        .send()
        .await
        .unwrap();

    // Evict below 0.5 (should remove both since both are at 0.3)
    let response = client
        .post(format!("{}/maintenance/evict", base_url))
        .json(&json!({
            "threshold": 0.5
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["evicted_count"], 2);

    // Verify stats show 0 triples
    let response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 0);
}

#[tokio::test]
async fn test_full_workflow() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // 1. Insert triples with source
    let insert_response = client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows",
                    "object": "Bob"
                }
            ],
            "source": {
                "type": "Conversation",
                "reference": "session-123"
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(insert_response.status(), StatusCode::OK);

    // 2. Query triples
    let response = client
        .get(format!("{}/triples?subject=Alice", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triples"].as_array().unwrap().len(), 1);

    // 3. Get neighbors
    let response = client
        .get(format!("{}/nodes/Alice/neighbors", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 1);

    // 4. Get stats
    let response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 1);
    assert_eq!(body["node_count"], 2);

    // 5. Decay
    let response = client
        .post(format!("{}/maintenance/decay", base_url))
        .json(&json!({
            "factor": 0.8,
            "min_weight": 0.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 6. Don't evict yet (weight is 0.8)
    let response = client
        .post(format!("{}/maintenance/evict", base_url))
        .json(&json!({
            "threshold": 0.5
        }))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["evicted_count"], 0);

    // 7. Verify triple still exists
    let response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 1);
}

// === EDGE CASE TESTS ===

#[tokio::test]
async fn test_post_triples_empty_body() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // POST with empty body should fail
    let response = client
        .post(format!("{}/triples", base_url))
        .send()
        .await
        .unwrap();

    // Should return error status (400 or 422)
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_post_triples_malformed_json() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // POST with malformed JSON
    let response = client
        .post(format!("{}/triples", base_url))
        .header("Content-Type", "application/json")
        .body("{invalid json}")
        .send()
        .await
        .unwrap();

    // Should return 400 Bad Request
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_get_triples_no_query_params() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert some test data
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows",
                    "object": "Bob"
                },
                {
                    "subject": "Bob",
                    "predicate": "knows",
                    "object": "Carol"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Query with no parameters (all wildcards) should return all triples
    let response = client
        .get(format!("{}/triples", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    let triples = body["triples"].as_array().unwrap();
    assert_eq!(triples.len(), 2); // Should return all triples
}

#[tokio::test]
async fn test_get_neighbors_nonexistent_node() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Try to get neighbors of non-existent node
    let response = client
        .get(format!("{}/nodes/999999/neighbors", base_url))
        .send()
        .await
        .unwrap();

    // Should return 404 or empty results
    // (implementation dependent - might return 200 with empty results or 404)
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
    
    if response.status() == StatusCode::OK {
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["triple_count"], 0);
    }
}

#[tokio::test]
async fn test_post_search_nonexistent_query_node() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert some data
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Search with non-existent node
    let response = client
        .post(format!("{}/search", base_url))
        .json(&json!({
            "query": "NonExistentNode",
            "k": 5
        }))
        .send()
        .await
        .unwrap();

    // Should return 200 with empty or error response
    assert!(response.status() == StatusCode::OK || response.status().is_client_error());
}

#[tokio::test]
async fn test_post_decay_invalid_factor() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Try decay with negative factor
    let response = client
        .post(format!("{}/maintenance/decay", base_url))
        .json(&json!({
            "factor": -0.5,
            "min_weight": 0.0
        }))
        .send()
        .await
        .unwrap();

    // Should succeed (no validation in current implementation)
    // or fail with 400 if validation is added
    assert!(response.status() == StatusCode::OK || response.status().is_client_error());
}

#[tokio::test]
async fn test_post_evict_negative_threshold() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert a triple
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Try evict with negative threshold
    let response = client
        .post(format!("{}/maintenance/evict", base_url))
        .json(&json!({
            "threshold": -1.0
        }))
        .send()
        .await
        .unwrap();

    // Should be rejected by input validation (negative threshold)
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_post_triples_missing_fields() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // POST with missing fields (no object)
    let response = client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows"
                    // Missing "object"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Should return 400 or 422 (validation error)
    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn test_get_neighbors_depth_zero() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert test data
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "Alice",
                    "predicate": "knows",
                    "object": "Bob"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Get neighbors with depth=0
    let response = client
        .get(format!("{}/nodes/Alice/neighbors?depth=0", base_url))
        .send()
        .await
        .unwrap();

    // Depth 0 is rejected by validation (must be >= 1)
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_stats_empty_database() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Get stats on empty database
    let response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_count"], 0);
    assert_eq!(body["node_count"], 0);
}

#[tokio::test]
async fn test_post_triples_empty_triples_array() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // POST with empty triples array
    let response = client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": []
        }))
        .send()
        .await
        .unwrap();

    // Should succeed but insert nothing
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["triple_ids"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_decay_factor_greater_than_one() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert a triple
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Decay with factor > 1.0 should be rejected by validation
    let response = client
        .post(format!("{}/maintenance/decay", base_url))
        .json(&json!({
            "factor": 2.0,
            "min_weight": 0.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_evict_threshold_above_one() {
    let (base_url, _server) = start_test_server().await;
    let client = Client::new();

    // Insert some triples
    client
        .post(format!("{}/triples", base_url))
        .json(&json!({
            "triples": [
                {
                    "subject": "A",
                    "predicate": "rel",
                    "object": "B"
                },
                {
                    "subject": "C",
                    "predicate": "rel",
                    "object": "D"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    // Evict with threshold > 1.0 (should evict everything since initial weight is 1.0)
    let response = client
        .post(format!("{}/maintenance/evict", base_url))
        .json(&json!({
            "threshold": 1.5
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["evicted_count"], 2);

    // Verify all triples are gone
    let stats_response = client
        .get(format!("{}/stats", base_url))
        .send()
        .await
        .unwrap();

    let stats_body: serde_json::Value = stats_response.json().await.unwrap();
    assert_eq!(stats_body["triple_count"], 0);
}
