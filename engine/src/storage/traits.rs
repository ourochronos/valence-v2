use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use crate::models::{Triple, TripleId, Node, NodeId, Source, SourceId};

/// Pattern for querying triples — None means wildcard.
#[derive(Debug, Clone, Default)]
pub struct TriplePattern {
    pub subject: Option<NodeId>,
    pub predicate: Option<String>,
    pub object: Option<NodeId>,
}

/// The core storage interface. Implementations can be Kuzu, PostgreSQL, 
/// in-memory, or anything else. Clean interface enables swapping.
#[async_trait]
pub trait TripleStore: Send + Sync {
    // Node operations
    async fn insert_node(&self, node: Node) -> Result<NodeId>;
    async fn get_node(&self, id: NodeId) -> Result<Option<Node>>;
    async fn find_node_by_value(&self, value: &str) -> Result<Option<Node>>;
    async fn find_or_create_node(&self, value: &str) -> Result<Node>;

    // Triple operations  
    async fn insert_triple(&self, triple: Triple) -> Result<TripleId>;
    async fn get_triple(&self, id: TripleId) -> Result<Option<Triple>>;
    async fn query_triples(&self, pattern: TriplePattern) -> Result<Vec<Triple>>;
    async fn touch_triple(&self, id: TripleId) -> Result<()>;
    async fn delete_triple(&self, id: TripleId) -> Result<()>;

    // Source operations
    async fn insert_source(&self, source: Source) -> Result<SourceId>;
    async fn get_sources_for_triple(&self, triple_id: TripleId) -> Result<Vec<Source>>;

    // Graph operations
    async fn neighbors(&self, node_id: NodeId, depth: u32) -> Result<Vec<Triple>>;
    async fn count_triples(&self) -> Result<u64>;
    async fn count_nodes(&self) -> Result<u64>;

    // Maintenance
    async fn decay(&self, factor: f64, min_weight: f64) -> Result<u64>;
    async fn evict_below_weight(&self, threshold: f64) -> Result<u64>;
}
