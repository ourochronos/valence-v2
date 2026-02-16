//! MCP tool implementation functions
//!
//! These functions implement the actual business logic for each MCP tool,
//! mapping to ValenceEngine operations.

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::{
    api::{
        InsertTriplesRequest, InsertTriplesResponse, QueryTriplesResponse,
        SearchRequest, SearchResponse, NeighborsResponse, SourcesResponse,
        StatsResponse, TripleResponse, NodeResponse, SourceResponse,
    },
    embeddings::EmbeddingStore,
    engine::ValenceEngine,
    graph::{GraphView, DynamicConfidence},
    models::{Triple, Source},
    storage::TriplePattern,
};

// ============================================================================
// Parameter types for tools that need them
// ============================================================================

/// Parameters for query_triples tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct QueryTriplesParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_sources: Option<bool>,
}

/// Parameters for neighbors tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NeighborsParams {
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Parameters for sources tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourcesParams {
    pub triple_id: String,
}

/// Parameters for maintain tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decay_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evict_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recompute_embeddings: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_dimensions: Option<usize>,
}

/// Response for maintain tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decay: Option<MaintainDecayResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evict: Option<MaintainEvictResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recompute_embeddings: Option<MaintainEmbeddingsResult>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainDecayResult {
    pub affected_count: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainEvictResult {
    pub evicted_count: u64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaintainEmbeddingsResult {
    pub embedding_count: usize,
}

// ============================================================================
// Tool implementations
// ============================================================================

/// Insert triples with source provenance
pub async fn insert_triples_impl(
    engine: &ValenceEngine,
    req: InsertTriplesRequest,
) -> Result<InsertTriplesResponse> {
    let mut triple_ids = Vec::new();

    // Insert each triple
    for triple_req in &req.triples {
        // Find or create subject and object nodes
        let subject_node = engine
            .store
            .find_or_create_node(&triple_req.subject)
            .await?;
        let object_node = engine
            .store
            .find_or_create_node(&triple_req.object)
            .await?;

        // Create and insert triple
        let triple = Triple::new(subject_node.id, &triple_req.predicate, object_node.id);
        let triple_id = engine.store.insert_triple(triple).await?;
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
        let source_id = engine.store.insert_source(source).await?;
        Some(source_id)
    } else {
        None
    };

    Ok(InsertTriplesResponse {
        triple_ids: triple_ids.iter().map(|id| id.to_string()).collect(),
        source_id: source_id.map(|id| id.to_string()),
    })
}

/// Query triples by pattern
pub async fn query_triples_impl(
    engine: &ValenceEngine,
    params: QueryTriplesParams,
) -> Result<QueryTriplesResponse> {
    // Resolve node values to IDs
    let subject_id = if let Some(ref subject_value) = params.subject {
        engine
            .store
            .find_node_by_value(subject_value)
            .await?
            .map(|n| n.id)
    } else {
        None
    };

    let object_id = if let Some(ref object_value) = params.object {
        engine
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
        predicate: params.predicate,
        object: object_id,
    };

    let triples = engine.store.query_triples(pattern).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    for triple in triples {
        let subject_node = engine.store.get_node(triple.subject).await?.unwrap();
        let object_node = engine.store.get_node(triple.object).await?.unwrap();

        let sources = if params.include_sources.unwrap_or(false) {
            let sources = engine.store.get_sources_for_triple(triple.id).await?;
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

    Ok(QueryTriplesResponse {
        triples: triple_responses,
    })
}

/// Semantic search using embeddings
pub async fn search_impl(
    engine: &ValenceEngine,
    req: SearchRequest,
) -> Result<SearchResponse> {
    use crate::api::SearchResult;

    // Find the query node by value
    let query_node = engine
        .store
        .find_node_by_value(&req.query_node)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Query node not found: {}", req.query_node))?;

    // Get the embedding for the query node
    let embeddings_store = engine.embeddings.read().await;
    let query_embedding = embeddings_store
        .get(query_node.id)
        .ok_or_else(|| anyhow::anyhow!("No embedding found for node: {}", req.query_node))?
        .clone(); // Clone to release the read lock

    // Find k nearest neighbors
    let neighbors = embeddings_store.query_nearest(&query_embedding, req.k)?;
    drop(embeddings_store); // Release lock before async operations

    // Build response
    let mut results = Vec::new();

    for (node_id, similarity) in neighbors {
        // Get node value
        let node = engine
            .store
            .get_node(node_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {:?}", node_id))?;

        // Optionally compute confidence
        let confidence = if req.include_confidence {
            // Build graph view
            let graph_view = GraphView::from_store(&*engine.store).await?;

            // Find a triple involving this node to compute confidence
            let pattern = TriplePattern {
                subject: Some(node_id),
                predicate: None,
                object: None,
            };
            let triples = engine.store.query_triples(pattern).await?;

            if let Some(triple) = triples.first() {
                let conf = DynamicConfidence::compute_confidence(
                    &*engine.store,
                    &graph_view,
                    triple.id,
                    Some(query_node.id),
                )
                .await?;
                Some(conf.combined)
            } else {
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

    Ok(SearchResponse {
        results,
        tier_reached: None,
        time_ms: None,
        budget_exhausted: None,
        fallback: None, // MCP search uses embeddings (warm mode only)
    })
}

/// Get k-hop neighborhood
pub async fn neighbors_impl(
    engine: &ValenceEngine,
    params: NeighborsParams,
) -> Result<NeighborsResponse> {
    // Try to parse as UUID, otherwise lookup by value
    let node_id = if let Ok(uuid) = Uuid::parse_str(&params.node) {
        uuid
    } else {
        engine
            .store
            .find_node_by_value(&params.node)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", params.node))?
            .id
    };

    let depth = params.depth.unwrap_or(1);
    let triples = engine.store.neighbors(node_id, depth).await?;

    // Convert to response format
    let mut triple_responses = Vec::new();
    let mut unique_nodes = HashSet::new();

    for triple in &triples {
        let subject_node = engine.store.get_node(triple.subject).await?.unwrap();
        let object_node = engine.store.get_node(triple.object).await?.unwrap();

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

    Ok(NeighborsResponse {
        triples: triple_responses,
        node_count: unique_nodes.len(),
        triple_count: triples.len(),
    })
}

/// Get provenance sources for a triple
pub async fn sources_impl(
    engine: &ValenceEngine,
    params: SourcesParams,
) -> Result<SourcesResponse> {
    let triple_id = Uuid::parse_str(&params.triple_id)
        .map_err(|_| anyhow::anyhow!("Invalid triple ID: {}", params.triple_id))?;

    let sources = engine.store.get_sources_for_triple(triple_id).await?;

    let source_responses: Vec<SourceResponse> = sources
        .into_iter()
        .map(|s| SourceResponse {
            id: s.id.to_string(),
            source_type: s.source_type,
            reference: s.reference,
            created_at: s.created_at,
        })
        .collect();

    Ok(SourcesResponse {
        sources: source_responses,
    })
}

/// Get engine statistics
pub async fn stats_impl(engine: &ValenceEngine) -> Result<StatsResponse> {
    let triple_count = engine.store.count_triples().await?;
    let node_count = engine.store.count_nodes().await?;

    // Calculate average weight
    let pattern = TriplePattern {
        subject: None,
        predicate: None,
        object: None,
    };
    let triples = engine.store.query_triples(pattern).await?;
    let avg_weight = if !triples.is_empty() {
        triples.iter().map(|t| t.weight).sum::<f64>() / triples.len() as f64
    } else {
        0.0
    };

    Ok(StatsResponse {
        triple_count,
        node_count,
        avg_weight,
    })
}

/// Run maintenance operations
pub async fn maintain_impl(
    engine: &ValenceEngine,
    params: MaintainParams,
) -> Result<MaintainResponse> {
    let mut response = MaintainResponse {
        decay: None,
        evict: None,
        recompute_embeddings: None,
    };

    // Decay if requested
    if let Some(decay_factor) = params.decay_factor {
        let affected_count = engine.store.decay(decay_factor, 0.0).await?;
        response.decay = Some(MaintainDecayResult { affected_count });
    }

    // Evict if requested
    if let Some(threshold) = params.evict_threshold {
        let evicted_count = engine.store.evict_below_weight(threshold).await?;
        response.evict = Some(MaintainEvictResult { evicted_count });
    }

    // Recompute embeddings if requested
    if params.recompute_embeddings.unwrap_or(false) {
        let dimensions = params.embedding_dimensions.unwrap_or(64);
        let embedding_count = engine.recompute_embeddings(dimensions).await?;
        response.recompute_embeddings = Some(MaintainEmbeddingsResult { embedding_count });
    }

    Ok(response)
}
