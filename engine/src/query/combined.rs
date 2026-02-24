//! Combined "connected AND similar" query — the killer query.
//!
//! Algorithm:
//! 1. Graph walk from anchor node → candidate set (bounded by depth)
//! 2. For each candidate, compute blended embedding similarity to target
//! 3. Final score = graph_distance_score * embedding_similarity_score
//! 4. Return top-k
//!
//! Key insight (category collapse): topology and embeddings are two views of
//! one structure. Graph-adjacent nodes are already embedding-close, so step 2
//! is refinement, not filtering.

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::engine::ValenceEngine;
use crate::embeddings::EmbeddingStore;
use crate::embeddings::spring::EmbeddingStrategy;
use crate::models::NodeId;
use crate::query::fusion::{EmbeddingBlendConfig, EmbeddingBlender, StrategyScores};

/// Parameters for a combined query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CombinedQueryParams {
    /// Anchor node: start of graph walk (value or UUID)
    pub anchor: String,
    /// Target node: embedding similarity measured against this (value or UUID)
    pub target: String,
    /// Maximum graph walk depth from anchor (default: 2)
    #[serde(default = "default_depth")]
    pub depth: u32,
    /// Number of results to return (default: 10)
    #[serde(default = "default_k")]
    pub k: usize,
    /// Embedding blend preset: "balanced", "exploratory", "precise", "discovery"
    #[serde(default = "default_blend")]
    pub blend: String,
}

fn default_depth() -> u32 { 2 }
fn default_k() -> usize { 10 }
fn default_blend() -> String { "balanced".to_string() }

/// A single result from the combined query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CombinedQueryResult {
    /// Node ID
    pub node_id: String,
    /// Node value
    pub value: String,
    /// Graph distance (hops) from anchor
    pub graph_distance: u32,
    /// Blended embedding similarity to target (0.0 to 1.0)
    pub embedding_similarity: f64,
    /// Combined score: graph_distance_score * embedding_similarity
    pub combined_score: f64,
}

/// Response from a combined query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CombinedQueryResponse {
    /// Results sorted by combined_score descending
    pub results: Vec<CombinedQueryResult>,
    /// Number of candidates evaluated (from graph walk)
    pub candidates_evaluated: usize,
    /// Whether multi-strategy embeddings were used (vs fallback)
    pub used_multi_embeddings: bool,
}

/// Execute the combined "connected AND similar" query.
///
/// 1. Resolve anchor and target nodes
/// 2. BFS from anchor up to `depth` hops → candidate set with distances
/// 3. Get target's embeddings (multi-strategy)
/// 4. For each candidate, compute blended similarity to target
/// 5. Score = graph_distance_score * embedding_similarity
/// 6. Return top-k sorted by score descending
pub async fn combined_query(
    engine: &ValenceEngine,
    params: CombinedQueryParams,
) -> Result<CombinedQueryResponse> {
    // Validate parameters
    if params.depth == 0 {
        anyhow::bail!("depth must be at least 1");
    }
    if params.depth > 10 {
        anyhow::bail!("depth cannot exceed 10");
    }
    if params.k == 0 {
        anyhow::bail!("k must be at least 1");
    }
    if params.k > 1000 {
        anyhow::bail!("k cannot exceed 1000");
    }

    // Resolve anchor node
    let anchor_id = resolve_node_id(engine, &params.anchor).await?;
    // Resolve target node
    let target_id = resolve_node_id(engine, &params.target).await?;

    // Step 1: BFS from anchor → candidate set with distances
    let candidates = bfs_with_distances(engine, anchor_id, params.depth).await?;
    let candidates_evaluated = candidates.len();

    // Step 2: Get target embeddings and set up blender
    let blend_config = parse_blend_config(&params.blend)?;
    let blender = EmbeddingBlender::new(blend_config);

    // Try multi-strategy embeddings first, fall back to single-strategy
    let multi = engine.multi_embeddings.read().await;
    let target_spring = multi.get_strategy(target_id, EmbeddingStrategy::Spring).cloned();
    let target_n2v = multi.get_strategy(target_id, EmbeddingStrategy::Node2Vec).cloned();
    let target_spectral = multi.get_strategy(target_id, EmbeddingStrategy::Spectral).cloned();
    let has_any_target_embedding = target_spring.is_some() || target_n2v.is_some() || target_spectral.is_some();

    // Step 3: Score each candidate
    let mut scored: Vec<CombinedQueryResult> = Vec::with_capacity(candidates.len());

    for (&candidate_id, &distance) in &candidates {
        // Skip the anchor itself if it equals target (optional: keep it)
        // We include everything and let scoring sort it out.

        // Compute graph distance score: 1.0 / (1.0 + distance)
        // This gives: distance=0 → 1.0, distance=1 → 0.5, distance=2 → 0.33, etc.
        let graph_distance_score = 1.0 / (1.0 + distance as f64);

        // Compute embedding similarity
        let embedding_similarity = if has_any_target_embedding {
            // Multi-strategy: compute per-strategy cosine similarities
            let cand_spring = multi.get_strategy(candidate_id, EmbeddingStrategy::Spring);
            let cand_n2v = multi.get_strategy(candidate_id, EmbeddingStrategy::Node2Vec);
            let cand_spectral = multi.get_strategy(candidate_id, EmbeddingStrategy::Spectral);

            let spring_sim = match (&target_spring, cand_spring) {
                (Some(t), Some(c)) => Some(cosine_similarity_f32(t, c) as f64),
                _ => None,
            };
            let n2v_sim = match (&target_n2v, cand_n2v) {
                (Some(t), Some(c)) => Some(cosine_similarity_f32(t, c) as f64),
                _ => None,
            };
            let spectral_sim = match (&target_spectral, cand_spectral) {
                (Some(t), Some(c)) => Some(cosine_similarity_f32(t, c) as f64),
                _ => None,
            };

            let scores = StrategyScores::new(spring_sim, n2v_sim, spectral_sim);
            let blended = blender.blend(&scores);

            // Normalize from [-1, 1] to [0, 1]
            ((blended + 1.0) / 2.0).clamp(0.0, 1.0)
        } else {
            // Fallback: use single-strategy embedding store
            drop_multi_and_use_single(engine, target_id, candidate_id).await
        };

        let combined_score = graph_distance_score * embedding_similarity;

        // Resolve node value
        let node = engine.store.get_node(candidate_id).await?;
        let value = node.map(|n| n.value).unwrap_or_else(|| candidate_id.to_string());

        scored.push(CombinedQueryResult {
            node_id: candidate_id.to_string(),
            value,
            graph_distance: distance,
            embedding_similarity,
            combined_score,
        });
    }

    drop(multi);

    // Step 4: Sort by combined score descending, take top-k
    scored.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(params.k);

    Ok(CombinedQueryResponse {
        results: scored,
        candidates_evaluated,
        used_multi_embeddings: has_any_target_embedding,
    })
}

/// Resolve a node identifier (value string or UUID) to a NodeId.
async fn resolve_node_id(engine: &ValenceEngine, identifier: &str) -> Result<NodeId> {
    // Try UUID first
    if let Ok(uuid) = uuid::Uuid::parse_str(identifier) {
        // Verify it exists
        if engine.store.get_node(uuid).await?.is_some() {
            return Ok(uuid);
        }
    }
    // Fall back to value lookup
    engine
        .store
        .find_node_by_value(identifier)
        .await?
        .map(|n| n.id)
        .ok_or_else(|| anyhow::anyhow!("Node not found: {}", identifier))
}

/// BFS from a starting node, returning all reachable nodes with their minimum distance.
async fn bfs_with_distances(
    engine: &ValenceEngine,
    start: NodeId,
    max_depth: u32,
) -> Result<HashMap<NodeId, u32>> {
    let mut distances: HashMap<NodeId, u32> = HashMap::new();
    let mut queue: VecDeque<(NodeId, u32)> = VecDeque::new();
    let mut visited: HashSet<NodeId> = HashSet::new();

    distances.insert(start, 0);
    visited.insert(start);
    queue.push_back((start, 0));

    while let Some((node_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        // Get 1-hop neighbors
        let neighbor_triples = engine.store.neighbors(node_id, 1).await?;

        for triple in &neighbor_triples {
            for &neighbor_id in &[triple.subject, triple.object] {
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id);
                    let new_depth = depth + 1;
                    distances.insert(neighbor_id, new_depth);
                    queue.push_back((neighbor_id, new_depth));
                }
            }
        }
    }

    Ok(distances)
}

/// Parse a blend preset name into an EmbeddingBlendConfig.
fn parse_blend_config(blend: &str) -> Result<EmbeddingBlendConfig> {
    match blend.to_lowercase().as_str() {
        "balanced" | "default" => Ok(EmbeddingBlendConfig::default()),
        "exploratory" | "explore" => Ok(EmbeddingBlendConfig::exploratory()),
        "precise" | "exact" => Ok(EmbeddingBlendConfig::precise()),
        "discovery" | "serendipity" => Ok(EmbeddingBlendConfig::discovery()),
        _ => anyhow::bail!(
            "Unknown blend preset '{}'. Options: balanced, exploratory, precise, discovery",
            blend
        ),
    }
}

/// Fallback: compute similarity using the single-strategy MemoryEmbeddingStore.
/// Returns a normalized similarity in [0, 1].
async fn drop_multi_and_use_single(
    engine: &ValenceEngine,
    target_id: NodeId,
    candidate_id: NodeId,
) -> f64 {
    let embeddings = engine.embeddings.read().await;
    let target_emb = embeddings.get(target_id);
    let cand_emb = embeddings.get(candidate_id);

    match (target_emb, cand_emb) {
        (Some(t), Some(c)) => {
            let sim = cosine_similarity_f32(t, c) as f64;
            ((sim + 1.0) / 2.0).clamp(0.0, 1.0)
        }
        _ => 0.5, // No embeddings available: neutral score
    }
}

/// Cosine similarity for f32 vectors.
fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;

    /// Helper: build a small graph for testing.
    async fn build_test_graph() -> ValenceEngine {
        let engine = ValenceEngine::new();

        // Create nodes
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();
        let dave = engine.store.find_or_create_node("Dave").await.unwrap();
        let eve = engine.store.find_or_create_node("Eve").await.unwrap();

        // Alice -> knows -> Bob -> knows -> Carol -> knows -> Dave
        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(carol.id, "knows", dave.id)).await.unwrap();
        // Alice -> likes -> Eve (separate cluster)
        engine.store.insert_triple(Triple::new(alice.id, "likes", eve.id)).await.unwrap();
        // Bob -> works_with -> Carol (extra edge for richer structure)
        engine.store.insert_triple(Triple::new(bob.id, "works_with", carol.id)).await.unwrap();

        engine
    }

    #[tokio::test]
    async fn test_combined_query_basic() {
        let engine = build_test_graph().await;

        // Compute spectral embeddings so we have something to compare
        engine.recompute_embeddings(4).await.unwrap();

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 2,
            k: 5,
            blend: "balanced".to_string(),
        };

        let response = combined_query(&engine, params).await.unwrap();

        // Should have results
        assert!(!response.results.is_empty(), "Should return results");
        assert!(response.candidates_evaluated > 0, "Should evaluate candidates");

        // Results should be sorted by combined_score descending
        for i in 1..response.results.len() {
            assert!(
                response.results[i - 1].combined_score >= response.results[i].combined_score,
                "Results should be sorted by combined_score descending"
            );
        }

        // All results should have graph_distance <= depth
        for r in &response.results {
            assert!(r.graph_distance <= 2, "All results should be within depth 2");
        }
    }

    #[tokio::test]
    async fn test_combined_query_depth_1() {
        let engine = build_test_graph().await;
        engine.recompute_embeddings(4).await.unwrap();

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 1,
            k: 10,
            blend: "balanced".to_string(),
        };

        let response = combined_query(&engine, params).await.unwrap();

        // With depth=1, should only find Alice's direct neighbors (Bob, Eve) + Alice itself
        for r in &response.results {
            assert!(r.graph_distance <= 1, "depth=1 results should be at most 1 hop away");
        }
    }

    #[tokio::test]
    async fn test_combined_query_same_anchor_and_target() {
        let engine = build_test_graph().await;
        engine.recompute_embeddings(4).await.unwrap();

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Alice".to_string(),
            depth: 2,
            k: 5,
            blend: "balanced".to_string(),
        };

        let response = combined_query(&engine, params).await.unwrap();

        // Should still work and return results
        assert!(!response.results.is_empty());

        // Alice itself should be at distance 0 and have high similarity to itself
        let alice_result = response.results.iter().find(|r| r.value == "Alice");
        assert!(alice_result.is_some(), "Alice should appear in results");
        let alice = alice_result.unwrap();
        assert_eq!(alice.graph_distance, 0);
        assert!(alice.embedding_similarity > 0.9, "Self-similarity should be high");
    }

    #[tokio::test]
    async fn test_combined_query_no_embeddings_fallback() {
        let engine = build_test_graph().await;
        // Do NOT compute embeddings — test fallback behavior

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 2,
            k: 5,
            blend: "balanced".to_string(),
        };

        let response = combined_query(&engine, params).await.unwrap();

        // Should still work with neutral similarity scores
        assert!(!response.results.is_empty(), "Should return results even without embeddings");
        assert!(!response.used_multi_embeddings, "Should not use multi embeddings");
    }

    #[tokio::test]
    async fn test_combined_query_k_limit() {
        let engine = build_test_graph().await;
        engine.recompute_embeddings(4).await.unwrap();

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 3,
            k: 2,
            blend: "balanced".to_string(),
        };

        let response = combined_query(&engine, params).await.unwrap();

        // Should return at most k results
        assert!(response.results.len() <= 2, "Should return at most k=2 results");
    }

    #[tokio::test]
    async fn test_combined_query_invalid_anchor() {
        let engine = build_test_graph().await;

        let params = CombinedQueryParams {
            anchor: "NonexistentNode".to_string(),
            target: "Carol".to_string(),
            depth: 2,
            k: 5,
            blend: "balanced".to_string(),
        };

        let result = combined_query(&engine, params).await;
        assert!(result.is_err(), "Should error on nonexistent anchor node");
    }

    #[tokio::test]
    async fn test_combined_query_invalid_target() {
        let engine = build_test_graph().await;

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "NonexistentNode".to_string(),
            depth: 2,
            k: 5,
            blend: "balanced".to_string(),
        };

        let result = combined_query(&engine, params).await;
        assert!(result.is_err(), "Should error on nonexistent target node");
    }

    #[tokio::test]
    async fn test_combined_query_blend_presets() {
        let engine = build_test_graph().await;
        engine.recompute_embeddings(4).await.unwrap();

        for blend in &["balanced", "exploratory", "precise", "discovery"] {
            let params = CombinedQueryParams {
                anchor: "Alice".to_string(),
                target: "Carol".to_string(),
                depth: 2,
                k: 5,
                blend: blend.to_string(),
            };

            let response = combined_query(&engine, params).await;
            assert!(response.is_ok(), "Blend preset '{}' should work", blend);
        }
    }

    #[tokio::test]
    async fn test_combined_query_invalid_blend() {
        let engine = build_test_graph().await;

        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 2,
            k: 5,
            blend: "invalid_preset".to_string(),
        };

        let result = combined_query(&engine, params).await;
        assert!(result.is_err(), "Should error on invalid blend preset");
    }

    #[tokio::test]
    async fn test_combined_query_depth_validation() {
        let engine = build_test_graph().await;

        // depth=0 should fail
        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 0,
            k: 5,
            blend: "balanced".to_string(),
        };
        assert!(combined_query(&engine, params).await.is_err());

        // depth=11 should fail
        let params = CombinedQueryParams {
            anchor: "Alice".to_string(),
            target: "Carol".to_string(),
            depth: 11,
            k: 5,
            blend: "balanced".to_string(),
        };
        assert!(combined_query(&engine, params).await.is_err());
    }

    #[tokio::test]
    async fn test_bfs_with_distances() {
        let engine = build_test_graph().await;

        let alice = engine.store.find_node_by_value("Alice").await.unwrap().unwrap();

        let distances = bfs_with_distances(&engine, alice.id, 2).await.unwrap();

        // Alice at distance 0
        assert_eq!(distances[&alice.id], 0);

        // Bob and Eve at distance 1
        let bob = engine.store.find_node_by_value("Bob").await.unwrap().unwrap();
        let eve = engine.store.find_node_by_value("Eve").await.unwrap().unwrap();
        assert_eq!(distances[&bob.id], 1);
        assert_eq!(distances[&eve.id], 1);

        // Carol at distance 2
        let carol = engine.store.find_node_by_value("Carol").await.unwrap().unwrap();
        assert_eq!(distances[&carol.id], 2);

        // Dave should NOT be reachable at depth 2
        let dave = engine.store.find_node_by_value("Dave").await.unwrap().unwrap();
        assert!(!distances.contains_key(&dave.id), "Dave should not be reachable at depth 2");
    }

    #[test]
    fn test_cosine_similarity_f32() {
        // Identical
        assert!((cosine_similarity_f32(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 0.001);

        // Orthogonal
        assert!(cosine_similarity_f32(&[1.0, 0.0], &[0.0, 1.0]).abs() < 0.001);

        // Opposite
        assert!((cosine_similarity_f32(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 0.001);

        // Zero vector
        assert_eq!(cosine_similarity_f32(&[0.0, 0.0], &[1.0, 0.0]), 0.0);

        // Empty
        assert_eq!(cosine_similarity_f32(&[], &[]), 0.0);

        // Different lengths
        assert_eq!(cosine_similarity_f32(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn test_parse_blend_config() {
        assert!(parse_blend_config("balanced").is_ok());
        assert!(parse_blend_config("exploratory").is_ok());
        assert!(parse_blend_config("precise").is_ok());
        assert!(parse_blend_config("discovery").is_ok());
        assert!(parse_blend_config("explore").is_ok());
        assert!(parse_blend_config("BALANCED").is_ok()); // case-insensitive
        assert!(parse_blend_config("invalid").is_err());
    }
}
