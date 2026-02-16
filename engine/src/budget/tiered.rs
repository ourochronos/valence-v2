//! Tiered retrieval with progressive refinement.
//!
//! Runs tiers progressively until budget exhausted or confidence threshold met:
//! - Tier 1: Vector search only (fast)
//! - Tier 2: + Graph walk from top results (medium)
//! - Tier 3: + Full confidence computation (slow, thorough)

use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use anyhow::Result;

use crate::{
    engine::ValenceEngine,
    embeddings::EmbeddingStore,
    graph::{GraphView, DynamicConfidence},
    models::NodeId,
    storage::TriplePattern,
};

use super::bounded::OperationBudget;

/// A single scored search result.
#[derive(Debug, Clone)]
pub struct ScoredResult {
    pub node_id: NodeId,
    pub value: String,
    pub similarity: f32,
    pub confidence: Option<f64>,
}

/// Result from tiered retrieval operation.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub results: Vec<ScoredResult>,
    pub tier_reached: u8,
    pub time_ms: u64,
    pub budget_exhausted: bool,
}

/// Tiered retriever that progressively refines results.
///
/// Starts with fast vector search, then optionally expands via graph walk
/// and computes full confidence scores, stopping when budget is exhausted
/// or confidence threshold is met.
pub struct TieredRetriever {
    engine: Arc<ValenceEngine>,
}

impl TieredRetriever {
    /// Create a new tiered retriever.
    pub fn new(engine: Arc<ValenceEngine>) -> Self {
        Self { engine }
    }

    /// Tier 1: Vector search only (fast).
    ///
    /// Returns top-k results based on embedding similarity.
    pub async fn tier1_search(&self, query_node: NodeId, k: usize) -> Result<Vec<ScoredResult>> {
        let embeddings = self.engine.embeddings.read().await;
        
        // Get query embedding
        let query_embedding = embeddings
            .get(query_node)
            .ok_or_else(|| anyhow::anyhow!("No embedding found for query node"))?;

        // Find k nearest neighbors
        let neighbors = embeddings.query_nearest(query_embedding, k)?;
        drop(embeddings);

        // Convert to ScoredResults
        let mut results = Vec::new();
        for (node_id, similarity) in neighbors {
            let node = self.engine.store.get_node(node_id).await?
                .ok_or_else(|| anyhow::anyhow!("Node not found: {:?}", node_id))?;
            
            results.push(ScoredResult {
                node_id,
                value: node.value,
                similarity,
                confidence: None,
            });
        }

        Ok(results)
    }

    /// Tier 2: Expand via graph walk from top results (medium).
    ///
    /// Takes the tier1 results and expands them by following graph edges,
    /// finding neighboring nodes and re-ranking by combined similarity + graph distance.
    pub async fn tier2_expand(
        &self,
        tier1_results: &[ScoredResult],
        hops: u32,
    ) -> Result<Vec<ScoredResult>> {
        if tier1_results.is_empty() {
            return Ok(Vec::new());
        }

        let mut expanded_nodes = HashMap::new();
        
        // Start with tier1 results (score = similarity)
        for result in tier1_results {
            expanded_nodes.insert(result.node_id, result.clone());
        }

        // Expand from top results (use top 5 or fewer)
        let top_count = tier1_results.len().min(5);
        for result in &tier1_results[..top_count] {
            // Get neighbors within hop budget
            let neighbors_triples = self.engine.store.neighbors(result.node_id, hops).await?;
            
            // Collect unique neighbor nodes
            let mut neighbor_nodes = HashSet::new();
            for triple in &neighbors_triples {
                neighbor_nodes.insert(triple.subject);
                neighbor_nodes.insert(triple.object);
            }

            // Add neighbors to expanded set with adjusted score
            let embeddings = self.engine.embeddings.read().await;
            for neighbor_id in neighbor_nodes {
                if !expanded_nodes.contains_key(&neighbor_id) {
                    // Get node value
                    if let Ok(Some(node)) = self.engine.store.get_node(neighbor_id).await {
                        // Get similarity (if embedding exists)
                        let similarity = if let Some(neighbor_emb) = embeddings.get(neighbor_id) {
                            if let Some(query_emb) = embeddings.get(tier1_results[0].node_id) {
                                // Compute cosine similarity
                                cosine_similarity(query_emb, neighbor_emb)
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };
                        
                        expanded_nodes.insert(neighbor_id, ScoredResult {
                            node_id: neighbor_id,
                            value: node.value,
                            similarity,
                            confidence: None,
                        });
                    }
                }
            }
            drop(embeddings);
        }

        // Convert to vec and sort by similarity
        let mut results: Vec<_> = expanded_nodes.into_values().collect();
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

        Ok(results)
    }

    /// Tier 3: Add full confidence computation (slow, thorough).
    ///
    /// Computes dynamic confidence scores from graph topology for each result.
    pub async fn tier3_confidence(&self, tier2_results: &[ScoredResult]) -> Result<Vec<ScoredResult>> {
        if tier2_results.is_empty() {
            return Ok(Vec::new());
        }

        // Build graph view for confidence computation
        let graph_view = GraphView::from_store(&*self.engine.store).await?;
        
        let mut results = Vec::new();
        
        for result in tier2_results {
            // Find a triple involving this node to compute confidence
            let pattern = TriplePattern {
                subject: Some(result.node_id),
                predicate: None,
                object: None,
            };
            let triples = self.engine.store.query_triples(pattern).await?;
            
            let confidence = if let Some(triple) = triples.first() {
                // Use the first result as query context
                let query_node = tier2_results[0].node_id;
                
                let conf = DynamicConfidence::compute_confidence(
                    &*self.engine.store,
                    &graph_view,
                    triple.id,
                    Some(query_node),
                )
                .await?;
                Some(conf.combined)
            } else {
                // No triples found for this node
                Some(0.0)
            };

            results.push(ScoredResult {
                node_id: result.node_id,
                value: result.value.clone(),
                similarity: result.similarity,
                confidence,
            });
        }

        // Re-sort by combined score: similarity * 0.5 + confidence * 0.5
        results.sort_by(|a, b| {
            let score_a = a.similarity as f64 * 0.5 
                + a.confidence.unwrap_or(0.0) * 0.5;
            let score_b = b.similarity as f64 * 0.5 
                + b.confidence.unwrap_or(0.0) * 0.5;
            score_b.partial_cmp(&score_a).unwrap()
        });

        Ok(results)
    }

    /// Auto-tiered retrieval: runs tiers progressively until budget exhausted
    /// or confidence threshold met.
    ///
    /// # Arguments
    /// * `query` - The query node value (will be looked up)
    /// * `budget` - Operation budget (time, hops, results)
    /// * `confidence_threshold` - Minimum confidence to stop early (0.0-1.0)
    pub async fn retrieve(
        &self,
        query: &str,
        budget: OperationBudget,
        confidence_threshold: f64,
    ) -> Result<RetrievalResult> {
        let start_time = std::time::Instant::now();
        
        // Find query node
        let query_node = self.engine.store.find_node_by_value(query).await?
            .ok_or_else(|| anyhow::anyhow!("Query node not found: {}", query))?;

        // Tier 1: Vector search
        let mut tier_reached = 1;
        let k = budget.check_results(100).then_some(100).unwrap_or(20);
        let mut results = self.tier1_search(query_node.id, k).await?;

        // Check if we should stop after Tier 1
        if budget.is_exhausted() {
            let elapsed = start_time.elapsed().as_millis() as u64;
            return Ok(RetrievalResult {
                results,
                tier_reached,
                time_ms: elapsed,
                budget_exhausted: true,
            });
        }

        // Check if top result meets confidence threshold (using similarity as proxy)
        if let Some(top_result) = results.first() {
            if top_result.similarity >= confidence_threshold as f32 {
                let elapsed = start_time.elapsed().as_millis() as u64;
                return Ok(RetrievalResult {
                    results,
                    tier_reached,
                    time_ms: elapsed,
                    budget_exhausted: false,
                });
            }
        }

        // Tier 2: Graph expansion
        tier_reached = 2;
        let max_hops = if budget.check_hop(3) { 3 } else { 1 };
        results = self.tier2_expand(&results, max_hops).await?;

        // Limit to result budget
        if results.len() > k {
            results.truncate(k);
        }

        // Check if we should stop after Tier 2
        if budget.is_exhausted() {
            let elapsed = start_time.elapsed().as_millis() as u64;
            return Ok(RetrievalResult {
                results,
                tier_reached,
                time_ms: elapsed,
                budget_exhausted: true,
            });
        }

        // Tier 3: Full confidence computation
        tier_reached = 3;
        results = self.tier3_confidence(&results).await?;

        // Check final confidence
        let budget_exhausted = budget.is_exhausted();
        let elapsed = start_time.elapsed().as_millis() as u64;

        Ok(RetrievalResult {
            results,
            tier_reached,
            time_ms: elapsed,
            budget_exhausted,
        })
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::ValenceEngine, models::Triple};

    #[tokio::test]
    async fn test_tier1_search() {
        let engine = Arc::new(ValenceEngine::new());

        // Create a small graph
        let a = engine.store.find_or_create_node("Alice").await.unwrap();
        let b = engine.store.find_or_create_node("Bob").await.unwrap();
        let c = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();

        // Compute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        // Create retriever
        let retriever = TieredRetriever::new(engine.clone());

        // Tier 1 search
        let results = retriever.tier1_search(a.id, 3).await.unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].node_id, a.id); // Self should be top result
        assert!(results[0].similarity > 0.99);
        assert!(results[0].confidence.is_none()); // Tier 1 doesn't compute confidence
    }

    #[tokio::test]
    async fn test_tier2_expand() {
        let engine = Arc::new(ValenceEngine::new());

        // Create a connected graph
        let a = engine.store.find_or_create_node("Alice").await.unwrap();
        let b = engine.store.find_or_create_node("Bob").await.unwrap();
        let c = engine.store.find_or_create_node("Carol").await.unwrap();
        let d = engine.store.find_or_create_node("Dave").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(c.id, "knows", d.id)).await.unwrap();

        // Compute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        let retriever = TieredRetriever::new(engine.clone());

        // Tier 1
        let tier1_results = retriever.tier1_search(a.id, 2).await.unwrap();

        // Tier 2 - expand 2 hops
        let tier2_results = retriever.tier2_expand(&tier1_results, 2).await.unwrap();

        // Should have more results than tier1 due to expansion
        assert!(tier2_results.len() >= tier1_results.len());
        
        // Should include Carol (2 hops from Alice)
        assert!(tier2_results.iter().any(|r| r.value == "Carol"));
    }

    #[tokio::test]
    async fn test_tier3_confidence() {
        let engine = Arc::new(ValenceEngine::new());

        // Create graph with sources
        let a = engine.store.find_or_create_node("Alice").await.unwrap();
        let b = engine.store.find_or_create_node("Bob").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();

        // Compute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        let retriever = TieredRetriever::new(engine.clone());

        // Get tier 1 results
        let tier1_results = retriever.tier1_search(a.id, 2).await.unwrap();

        // Tier 3 - add confidence scores
        let tier3_results = retriever.tier3_confidence(&tier1_results).await.unwrap();

        // Should have same results but with confidence scores
        assert_eq!(tier3_results.len(), tier1_results.len());
        
        // All results should have confidence scores
        for result in &tier3_results {
            assert!(result.confidence.is_some());
        }
    }

    #[tokio::test]
    async fn test_auto_tiered_retrieval() {
        let engine = Arc::new(ValenceEngine::new());

        // Create graph
        let a = engine.store.find_or_create_node("Alice").await.unwrap();
        let b = engine.store.find_or_create_node("Bob").await.unwrap();
        let c = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();

        // Compute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        let retriever = TieredRetriever::new(engine.clone());

        // Create budget: 1000ms, 3 hops, 20 results
        let budget = OperationBudget::new(1000, 3, 20);

        // Auto-tiered retrieval
        let result = retriever.retrieve("Alice", budget, 0.8).await.unwrap();

        assert!(!result.results.is_empty());
        assert!(result.tier_reached >= 1);
        assert!(result.tier_reached <= 3);
        assert!(result.time_ms < 1000); // Should complete within budget
    }

    #[tokio::test]
    async fn test_budget_exhaustion() {
        let engine = Arc::new(ValenceEngine::new());

        // Create graph
        let a = engine.store.find_or_create_node("Alice").await.unwrap();
        let b = engine.store.find_or_create_node("Bob").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        engine.recompute_embeddings(4).await.unwrap();

        let retriever = TieredRetriever::new(engine.clone());

        // Very tight budget: 1ms (likely to exhaust)
        let budget = OperationBudget::new(1, 1, 5);

        let result = retriever.retrieve("Alice", budget, 0.99).await.unwrap();

        // Should still return results even if budget exhausted
        assert!(!result.results.is_empty());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![1.0, 0.0];
        let d = vec![0.0, 1.0];
        assert!((cosine_similarity(&c, &d) - 0.0).abs() < 0.001);

        let e = vec![1.0, 1.0];
        let f = vec![1.0, 1.0];
        assert!((cosine_similarity(&e, &f) - 1.0).abs() < 0.001);
    }
}
