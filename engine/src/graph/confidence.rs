//! Dynamic confidence scoring based on graph topology.

use std::collections::HashMap;
use anyhow::Result;

use crate::models::{NodeId, TripleId};
use crate::storage::TripleStore;
use super::view::GraphView;
use super::algorithms::{pagerank, count_distinct_paths};

/// Confidence score for a triple, computed from topology.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfidenceScore {
    /// Source reliability (0.0 - 1.0)
    pub source_reliability: f64,
    /// Path diversity (0.0 - 1.0)
    pub path_diversity: f64,
    /// Centrality score (0.0 - 1.0)
    pub centrality: f64,
    /// Combined confidence (0.0 - 1.0)
    pub combined: f64,
}

impl ConfidenceScore {
    /// Create a new confidence score
    pub fn new(source_reliability: f64, path_diversity: f64, centrality: f64) -> Self {
        // Weighted combination: source reliability is most important,
        // followed by path diversity, then centrality
        let combined = 0.5 * source_reliability + 0.3 * path_diversity + 0.2 * centrality;
        
        Self {
            source_reliability,
            path_diversity,
            centrality,
            combined,
        }
    }
}

/// Compute dynamic confidence scores from graph topology.
pub struct DynamicConfidence;

impl DynamicConfidence {
    /// Compute source reliability for a triple.
    ///
    /// Measures:
    /// - Number of distinct sources
    /// - Average connectivity of source nodes
    pub async fn source_reliability(
        store: &(impl TripleStore + ?Sized),
        _graph: &GraphView,
        triple_id: TripleId,
    ) -> Result<f64> {
        let sources = store.get_sources_for_triple(triple_id).await?;
        
        if sources.is_empty() {
            return Ok(0.0);
        }

        // Count sources
        let source_count = sources.len() as f64;
        
        // Normalize by max expected sources (e.g., 10)
        let source_score = (source_count / 10.0).min(1.0);
        
        Ok(source_score)
    }

    /// Compute path diversity between query node and triple nodes.
    ///
    /// Measures how many distinct paths exist between nodes in the query context
    /// and the nodes involved in this triple.
    pub fn path_diversity(
        graph: &GraphView,
        query_node: NodeId,
        triple_subject: NodeId,
        triple_object: NodeId,
    ) -> f64 {
        const MAX_DEPTH: u32 = 5;
        const MAX_PATHS: usize = 10;

        // Count paths to subject
        let paths_to_subject = count_distinct_paths(graph, query_node, triple_subject, MAX_DEPTH);
        
        // Count paths to object
        let paths_to_object = count_distinct_paths(graph, query_node, triple_object, MAX_DEPTH);
        
        // Total paths
        let total_paths = paths_to_subject + paths_to_object;
        
        // Normalize
        total_paths.min(MAX_PATHS) as f64 / MAX_PATHS as f64
    }

    /// Compute centrality score for a node within the graph.
    ///
    /// Uses PageRank as a measure of node importance.
    pub fn centrality_score(
        _graph: &GraphView,
        node_id: NodeId,
        pagerank_scores: &HashMap<NodeId, f64>,
    ) -> f64 {
        pagerank_scores.get(&node_id).copied().unwrap_or(0.0)
    }

    /// Compute overall confidence for a triple at query time.
    ///
    /// Combines:
    /// - Source reliability (how many sources, how well-connected they are)
    /// - Path diversity (how many paths from query context to triple)
    /// - Centrality (PageRank scores of nodes in the triple)
    pub async fn compute_confidence(
        store: &(impl TripleStore + ?Sized),
        graph: &GraphView,
        triple_id: TripleId,
        query_context: Option<NodeId>,
    ) -> Result<ConfidenceScore> {
        // Get the triple
        let Some(triple) = store.get_triple(triple_id).await? else {
            return Ok(ConfidenceScore::new(0.0, 0.0, 0.0));
        };

        // Compute source reliability
        let source_reliability = Self::source_reliability(store, graph, triple_id).await?;

        // Compute PageRank scores
        let pagerank_scores = pagerank(graph, 0.85, 20);

        // Compute centrality for subject and object
        let subject_centrality = Self::centrality_score(graph, triple.subject, &pagerank_scores);
        let object_centrality = Self::centrality_score(graph, triple.object, &pagerank_scores);
        let avg_centrality = (subject_centrality + object_centrality) / 2.0;

        // Compute path diversity if we have query context
        let path_diversity = if let Some(query_node) = query_context {
            Self::path_diversity(graph, query_node, triple.subject, triple.object)
        } else {
            // No query context — use a default based on local connectivity
            let subject_neighbors = graph.neighbors(triple.subject).len();
            let object_neighbors = graph.neighbors(triple.object).len();
            let total_neighbors = subject_neighbors + object_neighbors;
            total_neighbors.min(20) as f64 / 20.0
        };

        Ok(ConfidenceScore::new(
            source_reliability,
            path_diversity,
            avg_centrality,
        ))
    }

    /// Compute confidence scores for multiple triples in batch.
    pub async fn compute_batch_confidence(
        store: &(impl TripleStore + ?Sized),
        graph: &GraphView,
        triple_ids: &[TripleId],
        query_context: Option<NodeId>,
    ) -> Result<HashMap<TripleId, ConfidenceScore>> {
        let mut scores = HashMap::new();

        for &triple_id in triple_ids {
            let score = Self::compute_confidence(store, graph, triple_id, query_context).await?;
            scores.insert(triple_id, score);
        }

        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Triple, Source, SourceType};
    use crate::storage::{MemoryStore, TripleStore};

    #[tokio::test]
    async fn test_source_reliability() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "knows", b.id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        // Add some sources
        let source1 = Source::new(vec![triple_id], SourceType::Observation);
        let source2 = Source::new(vec![triple_id], SourceType::UserInput);
        
        store.insert_source(source1).await.unwrap();
        store.insert_source(source2).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        let reliability = DynamicConfidence::source_reliability(&store, &graph, triple_id)
            .await
            .unwrap();
        
        // Should be > 0 since we have sources
        assert!(reliability > 0.0);
    }

    #[tokio::test]
    async fn test_confidence_computation() {
        let store = MemoryStore::new();
        
        // Build a small knowledge graph
        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let charlie = store.find_or_create_node("Charlie").await.unwrap();
        let diana = store.find_or_create_node("Diana").await.unwrap();
        
        // Well-connected triple: Alice knows Bob
        let t1 = Triple::new(alice.id, "knows", bob.id);
        let t1_id = store.insert_triple(t1).await.unwrap();
        
        // Add multiple sources for t1
        for _ in 0..3 {
            let source = Source::new(vec![t1_id], SourceType::Conversation);
            store.insert_source(source).await.unwrap();
        }
        
        // Create more connections around Alice and Bob
        store.insert_triple(Triple::new(alice.id, "works_with", charlie.id)).await.unwrap();
        store.insert_triple(Triple::new(bob.id, "lives_near", charlie.id)).await.unwrap();
        store.insert_triple(Triple::new(alice.id, "mentor_of", diana.id)).await.unwrap();
        
        // Peripheral triple: Diana knows some isolated node
        let eve = store.find_or_create_node("Eve").await.unwrap();
        let t2 = Triple::new(diana.id, "knows", eve.id);
        let t2_id = store.insert_triple(t2).await.unwrap();
        
        // Only one source for t2
        let source = Source::new(vec![t2_id], SourceType::Inference);
        store.insert_source(source).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        
        // Compute confidence for well-connected triple
        let conf1 = DynamicConfidence::compute_confidence(&store, &graph, t1_id, Some(alice.id))
            .await
            .unwrap();
        
        // Compute confidence for peripheral triple
        let conf2 = DynamicConfidence::compute_confidence(&store, &graph, t2_id, Some(alice.id))
            .await
            .unwrap();
        
        // Well-connected triple should have higher confidence
        assert!(conf1.combined > conf2.combined, 
            "Well-connected triple should have higher confidence: {} > {}", 
            conf1.combined, conf2.combined);
        
        // Source reliability should be higher for t1 (more sources)
        assert!(conf1.source_reliability > conf2.source_reliability);
    }

    #[tokio::test]
    async fn test_batch_confidence() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();
        
        let t1 = Triple::new(a.id, "links", b.id);
        let t2 = Triple::new(b.id, "links", c.id);
        
        let t1_id = store.insert_triple(t1).await.unwrap();
        let t2_id = store.insert_triple(t2).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        
        let scores = DynamicConfidence::compute_batch_confidence(
            &store,
            &graph,
            &[t1_id, t2_id],
            Some(a.id),
        )
        .await
        .unwrap();
        
        assert_eq!(scores.len(), 2);
        assert!(scores.contains_key(&t1_id));
        assert!(scores.contains_key(&t2_id));
    }

    // === EDGE CASE TESTS ===

    #[tokio::test]
    async fn test_confidence_isolated_triple() {
        let store = MemoryStore::new();
        
        // Create an isolated triple with no other connections
        let a = store.find_or_create_node("IsolatedA").await.unwrap();
        let b = store.find_or_create_node("IsolatedB").await.unwrap();
        
        let triple = Triple::new(a.id, "isolated_rel", b.id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        // No sources
        let graph = GraphView::from_store(&store).await.unwrap();
        
        let confidence = DynamicConfidence::compute_confidence(&store, &graph, triple_id, None)
            .await
            .unwrap();
        
        // Should have very low confidence (no sources, no connections, low centrality)
        assert!(confidence.combined < 0.2);
        assert_eq!(confidence.source_reliability, 0.0); // No sources
    }

    #[tokio::test]
    async fn test_confidence_nonexistent_triple() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        
        // Try to compute confidence for non-existent triple
        let fake_id = uuid::Uuid::new_v4();
        let confidence = DynamicConfidence::compute_confidence(&store, &graph, fake_id, None)
            .await
            .unwrap();
        
        // Should return all zeros
        assert_eq!(confidence.source_reliability, 0.0);
        assert_eq!(confidence.path_diversity, 0.0);
        assert_eq!(confidence.centrality, 0.0);
        assert_eq!(confidence.combined, 0.0);
    }

    #[tokio::test]
    async fn test_source_reliability_no_sources() {
        let store = MemoryStore::new();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        
        // No sources added
        let reliability = DynamicConfidence::source_reliability(&store, &graph, triple_id)
            .await
            .unwrap();
        
        assert_eq!(reliability, 0.0);
    }

    #[tokio::test]
    async fn test_path_diversity_no_paths() {
        let store = MemoryStore::new();
        
        // Create disconnected nodes
        let query = store.find_or_create_node("Query").await.unwrap();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        // Triple between A and B, but no connection to Query
        store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();
        
        let graph = GraphView::from_store(&store).await.unwrap();
        
        // No paths from query to triple nodes
        let diversity = DynamicConfidence::path_diversity(&graph, query.id, a.id, b.id);
        
        assert_eq!(diversity, 0.0);
    }
}
