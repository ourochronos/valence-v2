use anyhow::Result;
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
pub trait TripleStore: Send + Sync {
    // Node operations
    fn insert_node(&self, node: Node) -> impl std::future::Future<Output = Result<NodeId>> + Send;
    fn get_node(&self, id: NodeId) -> impl std::future::Future<Output = Result<Option<Node>>> + Send;
    fn find_node_by_value(&self, value: &str) -> impl std::future::Future<Output = Result<Option<Node>>> + Send;
    fn find_or_create_node(&self, value: &str) -> impl std::future::Future<Output = Result<Node>> + Send;

    // Triple operations  
    fn insert_triple(&self, triple: Triple) -> impl std::future::Future<Output = Result<TripleId>> + Send;
    fn get_triple(&self, id: TripleId) -> impl std::future::Future<Output = Result<Option<Triple>>> + Send;
    fn query_triples(&self, pattern: TriplePattern) -> impl std::future::Future<Output = Result<Vec<Triple>>> + Send;
    fn touch_triple(&self, id: TripleId) -> impl std::future::Future<Output = Result<()>> + Send;
    fn delete_triple(&self, id: TripleId) -> impl std::future::Future<Output = Result<()>> + Send;

    // Source operations
    fn insert_source(&self, source: Source) -> impl std::future::Future<Output = Result<SourceId>> + Send;
    fn get_sources_for_triple(&self, triple_id: TripleId) -> impl std::future::Future<Output = Result<Vec<Source>>> + Send;

    // Graph operations
    fn neighbors(&self, node_id: NodeId, depth: u32) -> impl std::future::Future<Output = Result<Vec<Triple>>> + Send;
    fn count_triples(&self) -> impl std::future::Future<Output = Result<u64>> + Send;
    fn count_nodes(&self) -> impl std::future::Future<Output = Result<u64>> + Send;

    // Maintenance
    fn decay(&self, factor: f64, min_weight: f64) -> impl std::future::Future<Output = Result<u64>> + Send;
    fn evict_below_weight(&self, threshold: f64) -> impl std::future::Future<Output = Result<u64>> + Send;
}

// Blanket implementation for Arc<T> where T: TripleStore
impl<T: TripleStore> TripleStore for Arc<T> {
    async fn insert_node(&self, node: Node) -> Result<NodeId> {
        (**self).insert_node(node).await
    }

    async fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        (**self).get_node(id).await
    }

    async fn find_node_by_value(&self, value: &str) -> Result<Option<Node>> {
        (**self).find_node_by_value(value).await
    }

    async fn find_or_create_node(&self, value: &str) -> Result<Node> {
        (**self).find_or_create_node(value).await
    }

    async fn insert_triple(&self, triple: Triple) -> Result<TripleId> {
        (**self).insert_triple(triple).await
    }

    async fn get_triple(&self, id: TripleId) -> Result<Option<Triple>> {
        (**self).get_triple(id).await
    }

    async fn query_triples(&self, pattern: TriplePattern) -> Result<Vec<Triple>> {
        (**self).query_triples(pattern).await
    }

    async fn touch_triple(&self, id: TripleId) -> Result<()> {
        (**self).touch_triple(id).await
    }

    async fn delete_triple(&self, id: TripleId) -> Result<()> {
        (**self).delete_triple(id).await
    }

    async fn insert_source(&self, source: Source) -> Result<SourceId> {
        (**self).insert_source(source).await
    }

    async fn get_sources_for_triple(&self, triple_id: TripleId) -> Result<Vec<Source>> {
        (**self).get_sources_for_triple(triple_id).await
    }

    async fn neighbors(&self, node_id: NodeId, depth: u32) -> Result<Vec<Triple>> {
        (**self).neighbors(node_id, depth).await
    }

    async fn count_triples(&self) -> Result<u64> {
        (**self).count_triples().await
    }

    async fn count_nodes(&self) -> Result<u64> {
        (**self).count_nodes().await
    }

    async fn decay(&self, factor: f64, min_weight: f64) -> Result<u64> {
        (**self).decay(factor, min_weight).await
    }

    async fn evict_below_weight(&self, threshold: f64) -> Result<u64> {
        (**self).evict_below_weight(threshold).await
    }
}
