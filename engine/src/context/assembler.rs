//! ContextAssembler: builds structured context blobs for LLM inference.
//!
//! Takes a query and working set, scores triples by relevance, and formats
//! them into a structured context suitable for LLM consumption.

use anyhow::{Result, Context as AnyhowContext};
use serde::{Serialize, Deserialize};

use crate::{
    engine::ValenceEngine,
    embeddings::EmbeddingStore,
    models::{NodeId, TripleId},
    storage::TripleStore,
    query::{FusionConfig, FusionScorer, RetrievalSignals},
    graph::DynamicConfidence,
};

use super::working_set::WorkingSet;

/// Context format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextFormat {
    /// Plain text format (simple, compact)
    Plain,
    /// Markdown format (structured, readable)
    Markdown,
    /// JSON format (machine-readable)
    Json,
}

/// Configuration for context assembly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyConfig {
    /// Maximum number of triples to include
    pub max_triples: usize,
    /// Maximum number of nodes to include
    pub max_nodes: usize,
    /// Whether to include confidence scores
    pub include_confidence: bool,
    /// Whether to include source information
    pub include_sources: bool,
    /// Output format
    pub format: ContextFormat,
    /// Fusion scoring configuration (optional, uses default if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fusion_config: Option<FusionConfig>,
}

impl Default for AssemblyConfig {
    fn default() -> Self {
        Self {
            max_triples: 50,
            max_nodes: 100,
            include_confidence: true,
            include_sources: false,
            format: ContextFormat::Markdown,
            fusion_config: None, // Will use FusionConfig::default() when needed
        }
    }
}

/// A triple with its relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredTriple {
    pub triple_id: TripleId,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// Information about a node in the context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub value: String,
    pub degree: usize, // Number of connections
}

/// The assembled context ready for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    /// Scored and filtered triples
    pub triples: Vec<ScoredTriple>,
    /// Nodes included in the context
    pub nodes: Vec<NodeInfo>,
    /// Total relevance score
    pub total_score: f64,
    /// Formatted context string
    pub formatted: String,
}

impl AssembledContext {
    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize assembled context to JSON")
    }
}

/// Assembles structured context from the knowledge graph for LLM consumption
pub struct ContextAssembler<'a> {
    engine: &'a ValenceEngine,
}

impl<'a> ContextAssembler<'a> {
    /// Create a new ContextAssembler
    pub fn new(engine: &'a ValenceEngine) -> Self {
        Self { engine }
    }

    /// Assemble context for a query (with graceful degradation)
    ///
    /// Process:
    /// 1. Build working set from query (falls back to graph-only if no embeddings)
    /// 2. Score triples by relevance:
    ///    - Warm mode: embedding similarity × confidence
    ///    - Cold mode: graph distance × confidence
    /// 3. Sort by score, take top N (token budget)
    /// 4. Format as structured text
    ///
    /// # Arguments
    ///
    /// * `query` - The query string
    /// * `config` - Assembly configuration
    ///
    /// # Returns
    ///
    /// An AssembledContext ready for LLM consumption
    pub async fn assemble(&self, query: &str, config: AssemblyConfig) -> Result<AssembledContext> {
        // Step 1: Build working set from query (with fallback)
        let k = config.max_nodes.min(100); // Cap at 100 for performance
        let working_set = WorkingSet::from_query(self.engine, query, k).await?;

        // Step 2: Score triples by relevance
        let query_node = self.engine
            .store
            .find_node_by_value(query)
            .await?
            .context("Query node not found")?;

        let embeddings_store = self.engine.embeddings.read().await;
        let query_embedding = embeddings_store.get(query_node.id);

        let mut scored_triples = Vec::new();

        if let Some(query_emb) = query_embedding {
            // Warm mode: use embedding similarity
            for (triple_id, (triple, confidence)) in &working_set.triples {
                // Get embeddings for subject and object
                let subject_emb = embeddings_store.get(triple.subject);
                let object_emb = embeddings_store.get(triple.object);

                // Compute average similarity to query
                let mut total_similarity = 0.0;
                let mut count = 0;

                if let Some(emb) = subject_emb {
                    total_similarity += cosine_similarity(query_emb, emb);
                    count += 1;
                }

                if let Some(emb) = object_emb {
                    total_similarity += cosine_similarity(query_emb, emb);
                    count += 1;
                }

                let avg_similarity = if count > 0 {
                    total_similarity / count as f32
                } else {
                    0.0
                };

                // Combine similarity and confidence
                let score = (avg_similarity as f64) * confidence.combined;

                // Get node values for formatting
                let subject_node = self.engine.store.get_node(triple.subject).await?
                    .context("Subject node not found")?;
                let object_node = self.engine.store.get_node(triple.object).await?
                    .context("Object node not found")?;

                scored_triples.push(ScoredTriple {
                    triple_id: *triple_id,
                    subject: subject_node.value,
                    predicate: triple.predicate.value.clone(),
                    object: object_node.value,
                    score,
                    confidence: if config.include_confidence {
                        Some(confidence.combined)
                    } else {
                        None
                    },
                });
            }
        } else {
            // Cold mode: fall back to graph distance scoring
            tracing::warn!(
                "No embedding found for query node '{}', using graph-distance scoring",
                query
            );

            // Score by graph proximity: triples involving query node get highest score,
            // 1-hop neighbors get medium score, 2-hop get lower score
            for (triple_id, (triple, confidence)) in &working_set.triples {
                // Calculate graph distance from query node
                let involves_query = triple.subject == query_node.id || triple.object == query_node.id;
                
                // Score based on proximity:
                // - Direct involvement: 1.0 × confidence
                // - 1-hop: 0.5 × confidence
                // - 2-hop or more: 0.25 × confidence
                let proximity_score = if involves_query {
                    1.0
                } else if working_set.contains_node(triple.subject) || working_set.contains_node(triple.object) {
                    0.5
                } else {
                    0.25
                };

                let score = proximity_score * confidence.combined;

                // Get node values for formatting
                let subject_node = self.engine.store.get_node(triple.subject).await?
                    .context("Subject node not found")?;
                let object_node = self.engine.store.get_node(triple.object).await?
                    .context("Object node not found")?;

                scored_triples.push(ScoredTriple {
                    triple_id: *triple_id,
                    subject: subject_node.value,
                    predicate: triple.predicate.value.clone(),
                    object: object_node.value,
                    score,
                    confidence: if config.include_confidence {
                        Some(confidence.combined)
                    } else {
                        None
                    },
                });
            }
        }

        drop(embeddings_store); // Release lock

        // Step 3: Sort by score descending and take top N
        scored_triples.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored_triples.truncate(config.max_triples);

        // Calculate total score
        let total_score: f64 = scored_triples.iter().map(|t| t.score).sum();

        // Build node info
        let mut node_degrees = std::collections::HashMap::new();
        for triple in &scored_triples {
            *node_degrees.entry(&triple.subject).or_insert(0) += 1;
            *node_degrees.entry(&triple.object).or_insert(0) += 1;
        }

        let mut nodes = Vec::new();
        let mut seen_nodes = std::collections::HashSet::new();

        for triple in &scored_triples {
            for (node_value, node_id) in [
                (&triple.subject, working_set.triples.get(&triple.triple_id).map(|(t, _)| t.subject)),
                (&triple.object, working_set.triples.get(&triple.triple_id).map(|(t, _)| t.object)),
            ] {
                if let Some(nid) = node_id {
                    if !seen_nodes.contains(&nid) {
                        seen_nodes.insert(nid);
                        nodes.push(NodeInfo {
                            node_id: nid,
                            value: node_value.clone(),
                            degree: *node_degrees.get(node_value).unwrap_or(&0),
                        });
                    }
                }
            }

            if nodes.len() >= config.max_nodes {
                break;
            }
        }

        // Always include the query node, even if there are no triples
        if !seen_nodes.contains(&query_node.id) && nodes.len() < config.max_nodes {
            nodes.push(NodeInfo {
                node_id: query_node.id,
                value: query_node.value.clone(),
                degree: *node_degrees.get(&query_node.value).unwrap_or(&0),
            });
        }

        // Step 4: Format as structured text
        let formatted = self.format_context(&scored_triples, &nodes, &config)?;

        Ok(AssembledContext {
            triples: scored_triples,
            nodes,
            total_score,
            formatted,
        })
    }

    /// Format the context according to the specified format
    fn format_context(
        &self,
        triples: &[ScoredTriple],
        nodes: &[NodeInfo],
        config: &AssemblyConfig,
    ) -> Result<String> {
        match config.format {
            ContextFormat::Plain => self.format_plain(triples, nodes, config),
            ContextFormat::Markdown => self.format_markdown(triples, nodes, config),
            ContextFormat::Json => self.format_json(triples, nodes, config),
        }
    }

    /// Format as plain text
    fn format_plain(
        &self,
        triples: &[ScoredTriple],
        nodes: &[NodeInfo],
        config: &AssemblyConfig,
    ) -> Result<String> {
        let mut output = String::new();
        
        output.push_str("=== Relevant Knowledge ===\n\n");

        for triple in triples {
            output.push_str(&format!(
                "{} {} {}\n",
                triple.subject,
                triple.predicate,
                triple.object
            ));

            if config.include_confidence {
                if let Some(conf) = triple.confidence {
                    output.push_str(&format!("  (confidence: {:.3})\n", conf));
                }
            }
        }

        output.push_str(&format!("\n{} entities, {} facts\n", nodes.len(), triples.len()));

        Ok(output)
    }

    /// Format as Markdown
    fn format_markdown(
        &self,
        triples: &[ScoredTriple],
        nodes: &[NodeInfo],
        config: &AssemblyConfig,
    ) -> Result<String> {
        let mut output = String::new();
        
        output.push_str("## Relevant Knowledge\n\n");

        // Group triples by subject for better readability
        let mut by_subject: std::collections::HashMap<String, Vec<&ScoredTriple>> =
            std::collections::HashMap::new();

        for triple in triples {
            by_subject
                .entry(triple.subject.clone())
                .or_insert_with(Vec::new)
                .push(triple);
        }

        for (subject, subject_triples) in by_subject {
            output.push_str(&format!("### {}\n\n", subject));

            for triple in subject_triples {
                let conf_str = if config.include_confidence {
                    if let Some(conf) = triple.confidence {
                        format!(" _(conf: {:.2})_", conf)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                output.push_str(&format!(
                    "- **{}**: {}{}\n",
                    triple.predicate,
                    triple.object,
                    conf_str
                ));
            }

            output.push('\n');
        }

        output.push_str(&format!(
            "---\n\n_{} entities, {} facts_\n",
            nodes.len(),
            triples.len()
        ));

        Ok(output)
    }

    /// Format as JSON
    fn format_json(
        &self,
        triples: &[ScoredTriple],
        nodes: &[NodeInfo],
        _config: &AssemblyConfig,
    ) -> Result<String> {
        let context = serde_json::json!({
            "triples": triples,
            "nodes": nodes,
            "triple_count": triples.len(),
            "node_count": nodes.len(),
        });

        serde_json::to_string_pretty(&context)
            .context("Failed to serialize context to JSON")
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![1.0, 0.0];
        let d = vec![0.0, 1.0];
        assert!((cosine_similarity(&c, &d) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_default_config() {
        let config = AssemblyConfig::default();
        assert_eq!(config.max_triples, 50);
        assert_eq!(config.max_nodes, 100);
        assert!(config.include_confidence);
        assert!(!config.include_sources);
        assert_eq!(config.format, ContextFormat::Markdown);
    }

    #[tokio::test]
    async fn test_assemble_empty_graph() {
        let engine = ValenceEngine::new();
        
        // Create a single isolated node — no embeddings
        let _lonely = engine.store.find_or_create_node("lonely").await.unwrap();

        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig::default();

        // Should succeed with graph-based fallback (cold mode)
        let result = assembler.assemble("lonely", config).await;
        assert!(result.is_ok());
        
        let context = result.unwrap();
        // Should have at least the query node
        assert_eq!(context.nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_assemble_cold_mode() {
        let engine = ValenceEngine::new();

        // Build a small knowledge graph WITHOUT embeddings
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "likes", carol.id)).await.unwrap();

        // DO NOT compute embeddings — test cold mode
        // Assemble context in cold mode
        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig {
            max_triples: 10,
            max_nodes: 10,
            include_confidence: true,
            include_sources: false,
            format: ContextFormat::Markdown,
            fusion_config: None,
        };

        let context = assembler.assemble("Alice", config).await.unwrap();

        // Should have found relevant triples via graph traversal
        assert!(context.triples.len() > 0);
        assert!(context.nodes.len() > 0);
        assert!(!context.formatted.is_empty());
    }

    #[tokio::test]
    async fn test_assemble_basic() {
        let engine = ValenceEngine::new();

        // Build a small knowledge graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "likes", carol.id)).await.unwrap();

        // Recompute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        // Assemble context
        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig {
            max_triples: 10,
            max_nodes: 10,
            include_confidence: true,
            include_sources: false,
            format: ContextFormat::Markdown,
            fusion_config: None,
        };

        let context = assembler.assemble("Alice", config).await.unwrap();

        // Should have found relevant triples
        assert!(context.triples.len() > 0);
        assert!(context.nodes.len() > 0);
        assert!(context.total_score >= 0.0);
        assert!(!context.formatted.is_empty());
    }

    #[tokio::test]
    async fn test_assemble_respects_max_triples() {
        let engine = ValenceEngine::new();

        // Build a larger graph
        for i in 0..20 {
            let from = engine.store.find_or_create_node(&format!("Node{}", i)).await.unwrap();
            let to = engine.store.find_or_create_node(&format!("Node{}", i + 1)).await.unwrap();
            engine.store.insert_triple(Triple::new(from.id, "next", to.id)).await.unwrap();
        }

        engine.recompute_embeddings(8).await.unwrap();

        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig {
            max_triples: 5,
            max_nodes: 20,
            include_confidence: false,
            include_sources: false,
            format: ContextFormat::Plain,
            fusion_config: None,
        };

        let context = assembler.assemble("Node0", config).await.unwrap();

        // Should respect max_triples limit
        assert!(context.triples.len() <= 5);
    }

    #[tokio::test]
    async fn test_format_plain() {
        let engine = ValenceEngine::new();

        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.recompute_embeddings(4).await.unwrap();

        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig {
            format: ContextFormat::Plain,
            fusion_config: None,
            ..Default::default()
        };

        let context = assembler.assemble("Alice", config).await.unwrap();

        assert!(context.formatted.contains("Relevant Knowledge"));
        assert!(context.formatted.contains("Alice"));
    }

    #[tokio::test]
    async fn test_format_json() {
        let engine = ValenceEngine::new();

        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.recompute_embeddings(4).await.unwrap();

        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig {
            format: ContextFormat::Json,
            fusion_config: None,
            ..Default::default()
        };

        let context = assembler.assemble("Alice", config).await.unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&context.formatted).unwrap();
        assert!(parsed.get("triples").is_some());
        assert!(parsed.get("nodes").is_some());
    }

    #[tokio::test]
    async fn test_scoring_ranks_by_relevance() {
        let engine = ValenceEngine::new();

        // Create a graph where some nodes are more connected to the query
        let query = engine.store.find_or_create_node("Query").await.unwrap();
        let close1 = engine.store.find_or_create_node("Close1").await.unwrap();
        let close2 = engine.store.find_or_create_node("Close2").await.unwrap();
        let distant = engine.store.find_or_create_node("Distant").await.unwrap();

        engine.store.insert_triple(Triple::new(query.id, "relates_to", close1.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(query.id, "relates_to", close2.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(close1.id, "relates_to", close2.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(distant.id, "relates_to", distant.id)).await.unwrap();

        engine.recompute_embeddings(4).await.unwrap();

        let assembler = ContextAssembler::new(&engine);
        let config = AssemblyConfig::default();

        let context = assembler.assemble("Query", config).await.unwrap();

        // Should have scored triples
        assert!(!context.triples.is_empty());

        // Scores should be sorted descending
        for i in 1..context.triples.len() {
            assert!(context.triples[i - 1].score >= context.triples[i].score);
        }
    }
}
