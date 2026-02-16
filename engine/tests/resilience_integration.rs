//! Integration tests for graceful degradation.

use valence_engine::{
    ValenceEngine, DegradationLevel, Triple,
    resilience::{ResilientRetrieval, RetrievalMode},
};
use std::sync::Arc;

#[tokio::test]
async fn test_full_mode_with_embeddings() {
    let engine = ValenceEngine::new();

    // Add some data
    let alice = engine.store.find_or_create_node("Alice").await.unwrap();
    let bob = engine.store.find_or_create_node("Bob").await.unwrap();
    let charlie = engine.store.find_or_create_node("Charlie").await.unwrap();

    engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
    engine.store.insert_triple(Triple::new(bob.id, "knows", charlie.id)).await.unwrap();

    // Compute embeddings
    engine.recompute_embeddings(8).await.unwrap();

    // Check we're in full mode
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Full);

    // Search should work
    let retrieval = ResilientRetrieval::new(Arc::new(engine.clone()));
    let result = retrieval.search("Alice", 10).await;
    
    assert!(!result.value.triple_ids.is_empty());
    assert_eq!(result.value.mode, RetrievalMode::Full);
    assert!(!result.used_fallback);
}

#[tokio::test]
async fn test_cold_mode_without_embeddings() {
    let engine = ValenceEngine::new();

    // Add some data (but don't compute embeddings)
    let alice = engine.store.find_or_create_node("Alice").await.unwrap();
    let bob = engine.store.find_or_create_node("Bob").await.unwrap();
    engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

    // No embeddings computed
    assert!(!engine.has_embeddings().await);

    // Search should fall back to cold mode
    let retrieval = ResilientRetrieval::new(Arc::new(engine.clone()));
    let result = retrieval.search("Alice", 10).await;
    
    assert!(!result.value.triple_ids.is_empty());
    assert_eq!(result.value.mode, RetrievalMode::Cold);
    assert!(result.used_fallback);
    assert!(result.value.warning.is_some());
}

#[tokio::test]
async fn test_embedding_failure_degrades_gracefully() {
    let engine = ValenceEngine::new();

    // Manually simulate an embedding failure
    engine.resilience.record_failure("embeddings", "test failure").await;
    
    // Check degradation was recorded
    assert!(engine.resilience.is_degraded("embeddings").await);
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Cold);
    
    // Get warnings
    let warnings = engine.resilience.get_warnings().await;
    assert!(!warnings.is_empty());
    
    let emb_warning = warnings.iter().find(|w| w.component == "embeddings");
    assert!(emb_warning.is_some());
}

#[tokio::test]
async fn test_resilience_recovery_after_success() {
    let engine = ValenceEngine::new();

    // Simulate a failure
    engine.resilience.record_failure("embeddings", "test failure").await;
    
    // Should be degraded
    assert!(engine.resilience.is_degraded("embeddings").await);
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Cold);

    // Simulate 3 consecutive successes to trigger recovery
    engine.resilience.record_success("embeddings").await;
    engine.resilience.record_success("embeddings").await;
    engine.resilience.record_success("embeddings").await;

    // Should be recovered
    assert!(!engine.resilience.is_degraded("embeddings").await);
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Full);
}

#[tokio::test]
async fn test_get_neighbors_with_fallback() {
    let engine = ValenceEngine::new();

    // Add a small graph
    let alice = engine.store.find_or_create_node("Alice").await.unwrap();
    let bob = engine.store.find_or_create_node("Bob").await.unwrap();
    let charlie = engine.store.find_or_create_node("Charlie").await.unwrap();
    
    engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
    engine.store.insert_triple(Triple::new(alice.id, "likes", charlie.id)).await.unwrap();
    engine.store.insert_triple(Triple::new(bob.id, "knows", charlie.id)).await.unwrap();

    // No embeddings, so should fall back
    let retrieval = ResilientRetrieval::new(Arc::new(engine.clone()));
    let result = retrieval.get_neighbors(alice.id, 10).await;
    
    // Should get Alice's outgoing edges
    assert_eq!(result.value.len(), 2);
    assert!(result.used_fallback);
}

#[tokio::test]
async fn test_manual_degradation_level_override() {
    let engine = ValenceEngine::new();

    // Start in full mode
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Full);

    // Force cold mode
    engine.resilience.set_level(DegradationLevel::Cold).await;
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Cold);

    // Check capabilities reflect cold mode
    let level = engine.resilience.current_level().await;
    assert!(!level.has_embeddings());
    assert!(level.has_graph());
    assert!(level.has_confidence());
    assert!(level.has_store());
}

#[tokio::test]
async fn test_degradation_warnings_content() {
    let engine = ValenceEngine::new();

    // Simulate embedding failure
    engine.resilience.record_failure("embeddings", "test error message").await;

    // Get warnings
    let warnings = engine.resilience.get_warnings().await;
    assert_eq!(warnings.len(), 1);

    let warning = &warnings[0];
    assert_eq!(warning.component, "embeddings");
    assert!(warning.message.contains("graph-based"));
    assert_eq!(warning.last_error, Some("test error message".to_string()));
}

#[tokio::test]
async fn test_multiple_component_degradation() {
    let engine = ValenceEngine::new();

    // Initially full mode
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Full);

    // Degrade embeddings
    engine.resilience.record_failure("embeddings", "compute failed").await;
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Cold);

    // Degrade graph too
    engine.resilience.record_failure("graph", "algorithm failed").await;
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Minimal);

    // Degrade storage
    engine.resilience.record_failure("storage", "connection lost").await;
    assert_eq!(engine.resilience.current_level().await, DegradationLevel::Offline);

    // Should have 3 warnings
    let warnings = engine.resilience.get_warnings().await;
    assert_eq!(warnings.len(), 3);
}

#[tokio::test]
async fn test_search_with_nonexistent_node() {
    let engine = ValenceEngine::new();
    
    let retrieval = ResilientRetrieval::new(Arc::new(engine));
    let result = retrieval.search("NonexistentNode", 10).await;
    
    // Should return empty results with a warning
    assert!(result.value.triple_ids.is_empty());
    assert!(result.value.warning.is_some());
    assert!(result.value.warning.unwrap().contains("not found"));
}
