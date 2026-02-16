//! ValenceEngine: Unified engine combining storage, embeddings, and lifecycle management.

use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};

use crate::{
    storage::{MemoryStore, TripleStore},
    embeddings::{EmbeddingStore, memory::MemoryEmbeddingStore, spectral},
};

/// ValenceEngine ties together the triple store and embedding store,
/// providing unified lifecycle management.
#[derive(Clone)]
pub struct ValenceEngine {
    /// The triple store (trait object for flexibility)
    pub store: Arc<dyn TripleStore>,
    /// The embedding store (wrapped in RwLock for interior mutability)
    pub embeddings: Arc<RwLock<MemoryEmbeddingStore>>,
}

impl ValenceEngine {
    /// Create a new ValenceEngine with empty stores
    pub fn new() -> Self {
        Self {
            store: Arc::new(MemoryStore::new()),
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
        }
    }

    /// Create a ValenceEngine from an existing MemoryStore
    pub fn from_store(store: MemoryStore) -> Self {
        Self {
            store: Arc::new(store),
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
        }
    }

    /// Create a ValenceEngine from any TripleStore implementation
    pub fn from_triple_store<S: TripleStore + 'static>(store: S) -> Self {
        Self {
            store: Arc::new(store),
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
        }
    }

    /// Recompute embeddings from the current graph state
    ///
    /// This uses spectral embedding to derive a vector representation
    /// from the graph topology.
    pub async fn recompute_embeddings(&self, dimensions: usize) -> Result<usize> {
        // Compute embeddings from current graph
        let embeddings_map = spectral::compute_embeddings(self.store.as_ref(), dimensions)
            .await
            .context("Failed to compute spectral embeddings")?;

        let count = embeddings_map.len();

        // Replace the embedding store with new embeddings
        let new_store = MemoryEmbeddingStore::from_embeddings(embeddings_map)
            .context("Failed to create embedding store from computed embeddings")?;

        let mut embeddings = self.embeddings.write().await;
        *embeddings = new_store;

        Ok(count)
    }

    /// Run a decay + eviction cycle
    ///
    /// This applies exponential decay to all triple weights, then removes
    /// triples below a threshold.
    pub async fn run_maintenance_cycle(
        &self,
        decay_factor: f64,
        min_weight: f64,
        evict_threshold: f64,
    ) -> Result<(u64, u64)> {
        // Apply decay
        let decayed = self.store.decay(decay_factor, min_weight).await?;

        // Evict low-weight triples
        let evicted = self.store.evict_below_weight(evict_threshold).await?;

        Ok((decayed, evicted))
    }

    /// Get the number of stored embeddings
    pub async fn embedding_count(&self) -> usize {
        let embeddings = self.embeddings.read().await;
        embeddings.len()
    }

    /// Check if embeddings are available
    pub async fn has_embeddings(&self) -> bool {
        let embeddings = self.embeddings.read().await;
        !embeddings.is_empty()
    }
}

impl Default for ValenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::Triple,
        storage::TripleStore,
    };

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = ValenceEngine::new();
        assert_eq!(engine.store.count_triples().await.unwrap(), 0);
        assert_eq!(engine.embedding_count().await, 0);
    }

    #[tokio::test]
    async fn test_recompute_embeddings() {
        let engine = ValenceEngine::new();

        // Create a small graph
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        let c = engine.store.find_or_create_node("C").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(b.id, "knows", c.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(a.id, "likes", c.id)).await.unwrap();

        // Initially no embeddings
        assert!(!engine.has_embeddings().await);

        // Compute embeddings
        let count = engine.recompute_embeddings(4).await.unwrap();

        // Should have 3 embeddings (one per node)
        assert_eq!(count, 3);
        assert!(engine.has_embeddings().await);
        assert_eq!(engine.embedding_count().await, 3);

        // Verify we can get an embedding
        let embeddings = engine.embeddings.read().await;
        let emb_a = embeddings.get(a.id);
        assert!(emb_a.is_some());
        assert_eq!(emb_a.unwrap().len(), 2); // Capped by node count - 1
    }

    #[tokio::test]
    async fn test_maintenance_cycle() {
        let engine = ValenceEngine::new();

        // Insert a triple
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();

        // Run maintenance: decay by 0.5, evict below 0.6
        let (decayed, evicted) = engine.run_maintenance_cycle(0.5, 0.0, 0.6).await.unwrap();

        assert_eq!(decayed, 1); // One triple decayed
        assert_eq!(evicted, 1); // One triple evicted (weight goes from 1.0 to 0.5)

        // Triple should be gone
        assert_eq!(engine.store.count_triples().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_from_existing_store() {
        let store = MemoryStore::new();

        // Add some data to the store
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        store.insert_triple(Triple::new(a.id, "test", b.id)).await.unwrap();

        // Create engine from store
        let engine = ValenceEngine::from_store(store);

        // Should have the triple
        assert_eq!(engine.store.count_triples().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_embeddings_persist_after_clone() {
        let engine = ValenceEngine::new();

        // Add nodes
        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();
        engine.store.insert_triple(Triple::new(a.id, "x", b.id)).await.unwrap();

        // Compute embeddings
        engine.recompute_embeddings(2).await.unwrap();

        // Clone the engine
        let engine2 = engine.clone();

        // Both should have embeddings (Arc sharing)
        assert!(engine.has_embeddings().await);
        assert!(engine2.has_embeddings().await);
    }
}
