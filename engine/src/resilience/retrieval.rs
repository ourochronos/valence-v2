//! Resilient retrieval with automatic fallback strategies.
//!
//! Implements graceful degradation for knowledge retrieval:
//! - Full mode: embeddings + graph + confidence
//! - Cold mode: graph + confidence only
//! - Minimal mode: graph traversal + recency
//! - Offline mode: cached results

use std::sync::Arc;
use crate::{
    engine::ValenceEngine,
    models::{NodeId, TripleId},
    storage::{TripleStore, TriplePattern},
    error::ValenceError,
    embeddings::EmbeddingStore,
};
use super::degradation::DegradationLevel;
use super::fallback::ResilientResult;

/// Retrieval mode indicates which strategy was used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalMode {
    /// Full retrieval with embeddings
    Full,
    /// Cold mode: graph-based only
    Cold,
    /// Minimal mode: direct traversal
    Minimal,
    /// Offline: cached only
    Offline,
}

impl From<DegradationLevel> for RetrievalMode {
    fn from(level: DegradationLevel) -> Self {
        match level {
            DegradationLevel::Full => RetrievalMode::Full,
            DegradationLevel::Cold => RetrievalMode::Cold,
            DegradationLevel::Minimal => RetrievalMode::Minimal,
            DegradationLevel::Offline => RetrievalMode::Offline,
        }
    }
}

/// Result of a resilient retrieval operation
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    /// Retrieved triple IDs
    pub triple_ids: Vec<TripleId>,
    /// Mode used for retrieval
    pub mode: RetrievalMode,
    /// Optional warning message
    pub warning: Option<String>,
}

/// Resilient retrieval engine with automatic fallback
pub struct ResilientRetrieval {
    engine: Arc<ValenceEngine>,
}

impl ResilientRetrieval {
    /// Create a new resilient retrieval engine
    pub fn new(engine: Arc<ValenceEngine>) -> Self {
        Self { engine }
    }

    /// Retrieve neighbors of a node with automatic fallback
    pub async fn get_neighbors(
        &self,
        node_id: NodeId,
        max_results: usize,
    ) -> ResilientResult<Vec<TripleId>> {
        // Try full retrieval first
        match self.try_full_retrieval(node_id, max_results).await {
            Ok(result) => ResilientResult::ok(result),
            Err(_) => {
                // Fall back to cold mode (graph only)
                match self.try_cold_retrieval(node_id, max_results).await {
                    Ok(result) => ResilientResult::with_fallback(
                        result,
                        "Embeddings unavailable. Using graph-based retrieval.".to_string(),
                    ),
                    Err(_) => {
                        // Fall back to minimal mode
                        let result = self.try_minimal_retrieval(node_id, max_results).await;
                        ResilientResult::with_fallback(
                            result,
                            "Advanced features unavailable. Using basic graph traversal.".to_string(),
                        )
                    }
                }
            }
        }
    }

    /// Search for triples matching a query with automatic fallback
    pub async fn search(
        &self,
        query_value: &str,
        limit: usize,
    ) -> ResilientResult<RetrievalResult> {
        // Try to find the query node
        let query_node = match self.engine.store.find_node_by_value(query_value).await {
            Ok(Some(node)) => node,
            Ok(None) => {
                // Node not found
                return ResilientResult::ok(RetrievalResult {
                    triple_ids: Vec::new(),
                    mode: RetrievalMode::Full,
                    warning: Some(format!("Node '{}' not found", query_value)),
                });
            }
            Err(e) => {
                // Store error - offline mode
                return ResilientResult::with_fallback(
                    RetrievalResult {
                        triple_ids: Vec::new(),
                        mode: RetrievalMode::Offline,
                        warning: Some(format!("Store unavailable: {}", e)),
                    },
                    "Storage unavailable. No results can be retrieved.".to_string(),
                );
            }
        };

        // Check if we have embeddings
        let has_embeddings = self.engine.has_embeddings().await;

        if has_embeddings {
            // Full mode: try with embeddings
            match self.search_with_embeddings(query_node.id, limit).await {
                Ok(triple_ids) => {
                    ResilientResult::ok(RetrievalResult {
                        triple_ids,
                        mode: RetrievalMode::Full,
                        warning: None,
                    })
                }
                Err(_) => {
                    // Embedding search failed, fall back to cold mode
                    self.search_cold_mode(query_node.id, limit).await
                }
            }
        } else {
            // No embeddings available, use cold mode
            self.search_cold_mode(query_node.id, limit).await
        }
    }

    /// Try full retrieval with embeddings
    async fn try_full_retrieval(
        &self,
        node_id: NodeId,
        max_results: usize,
    ) -> Result<Vec<TripleId>, ValenceError> {
        // Check if embeddings are available
        if !self.engine.has_embeddings().await {
            return Err(ValenceError::Embedding(
                crate::error::EmbeddingError::NotFound(node_id.to_string()),
            ));
        }

        // Get all triples involving this node using neighbors
        let triples = self.engine.store.neighbors(node_id, 1).await?;
        
        // Extract IDs and limit results
        let limited: Vec<_> = triples.into_iter().map(|t| t.id).take(max_results).collect();
        Ok(limited)
    }

    /// Try cold mode retrieval (graph-based, no embeddings)
    async fn try_cold_retrieval(
        &self,
        node_id: NodeId,
        max_results: usize,
    ) -> Result<Vec<TripleId>, ValenceError> {
        // Get triples for node using graph structure only
        let triples = self.engine.store.neighbors(node_id, 1).await?;
        
        // Sort by weight (confidence proxy) and take top N
        let mut sorted = triples;
        sorted.sort_by(|a, b| b.local_weight.partial_cmp(&a.local_weight).unwrap_or(std::cmp::Ordering::Equal));
        
        // Extract IDs and limit
        let limited: Vec<_> = sorted.into_iter().map(|t| t.id).take(max_results).collect();
        Ok(limited)
    }

    /// Try minimal retrieval (basic graph traversal)
    async fn try_minimal_retrieval(&self, node_id: NodeId, max_results: usize) -> Vec<TripleId> {
        // Simplified: just get triples and return first N
        match self.engine.store.neighbors(node_id, 1).await {
            Ok(triples) => triples.into_iter().map(|t| t.id).take(max_results).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Search using embeddings (full mode)
    async fn search_with_embeddings(
        &self,
        query_node_id: NodeId,
        limit: usize,
    ) -> Result<Vec<TripleId>, ValenceError> {
        // Get query node embedding
        let embeddings = self.engine.embeddings.read().await;
        let _query_embedding = embeddings
            .get(query_node_id)
            .ok_or_else(|| {
                ValenceError::Embedding(crate::error::EmbeddingError::NotFound(
                    query_node_id.to_string(),
                ))
            })?;

        // For now, just return triples involving the query node
        // (Full semantic search would compare embeddings)
        drop(embeddings); // Release lock
        let triples = self.engine.store.neighbors(query_node_id, 1).await?;
        Ok(triples.into_iter().map(|t| t.id).take(limit).collect())
    }

    /// Search in cold mode (no embeddings)
    async fn search_cold_mode(&self, query_node_id: NodeId, limit: usize) -> ResilientResult<RetrievalResult> {
        match self.engine.store.neighbors(query_node_id, 1).await {
            Ok(triples) => {
                let limited: Vec<_> = triples.into_iter().map(|t| t.id).take(limit).collect();
                ResilientResult::with_fallback(
                    RetrievalResult {
                        triple_ids: limited,
                        mode: RetrievalMode::Cold,
                        warning: Some("Embeddings unavailable. Using graph-based retrieval only.".to_string()),
                    },
                    "Operating in cold mode (no embeddings).".to_string(),
                )
            }
            Err(e) => {
                // Store error - minimal mode
                ResilientResult::with_fallback(
                    RetrievalResult {
                        triple_ids: Vec::new(),
                        mode: RetrievalMode::Minimal,
                        warning: Some(format!("Store error: {}", e)),
                    },
                    "Store unavailable. Cannot retrieve results.".to_string(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;

    #[tokio::test]
    async fn test_retrieval_mode_from_degradation_level() {
        assert_eq!(RetrievalMode::from(DegradationLevel::Full), RetrievalMode::Full);
        assert_eq!(RetrievalMode::from(DegradationLevel::Cold), RetrievalMode::Cold);
        assert_eq!(RetrievalMode::from(DegradationLevel::Minimal), RetrievalMode::Minimal);
        assert_eq!(RetrievalMode::from(DegradationLevel::Offline), RetrievalMode::Offline);
    }

    #[tokio::test]
    async fn test_resilient_search_with_missing_node() {
        let engine = ValenceEngine::new();
        let retrieval = ResilientRetrieval::new(Arc::new(engine));

        let result = retrieval.search("nonexistent", 10).await;
        assert_eq!(result.value.triple_ids.len(), 0);
        assert!(result.value.warning.is_some());
    }

    #[tokio::test]
    async fn test_resilient_search_cold_mode() {
        let engine = ValenceEngine::new();

        // Add some data
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        let retrieval = ResilientRetrieval::new(Arc::new(engine));

        // Search without embeddings (cold mode)
        let result = retrieval.search("Alice", 10).await;
        assert!(!result.value.triple_ids.is_empty());
        assert_eq!(result.value.mode, RetrievalMode::Cold);
        assert!(result.used_fallback);
    }

    #[tokio::test]
    async fn test_get_neighbors_fallback() {
        let engine = ValenceEngine::new();

        // Add some data
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let charlie = engine.store.find_or_create_node("Charlie").await.unwrap();
        
        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "likes", charlie.id)).await.unwrap();

        let retrieval = ResilientRetrieval::new(Arc::new(engine));

        // Get neighbors (will fall back since no embeddings)
        let result = retrieval.get_neighbors(alice.id, 10).await;
        assert_eq!(result.value.len(), 2);
        assert!(result.used_fallback);
    }
}
