//! ValenceEngine: Unified engine combining storage, embeddings, and lifecycle management.

use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, Context};

use crate::{
    storage::{MemoryStore, TripleStore},
    embeddings::{EmbeddingStore, memory::MemoryEmbeddingStore, spectral},
    embeddings::spring::{MultiEmbeddingStore, EmbeddingStrategy},
    embeddings::strategy_selector::StrategySelector,
    stigmergy::AccessTracker,
    lifecycle::{LifecycleManager, DecayPolicy, MemoryBounds},
    resilience::ResilienceManager,
    inference::{FeedbackRecorder, WeightAdjuster, WeightAdjusterConfig, BlendTuner},
    vkb::MemorySessionStore,
    identity::Keypair,
    models::NodeId,
};

/// ValenceEngine ties together the triple store, embedding store, access tracker,
/// lifecycle manager, resilience manager, and inference training loop components,
/// providing unified knowledge management with graceful degradation.
#[derive(Clone)]
pub struct ValenceEngine {
    /// The triple store (trait object for flexibility)
    pub store: Arc<dyn TripleStore>,
    /// The embedding store (wrapped in RwLock for interior mutability)
    pub embeddings: Arc<RwLock<MemoryEmbeddingStore>>,
    /// The access tracker for stigmergy
    pub access_tracker: Arc<AccessTracker>,
    /// The lifecycle manager for decay and bounds
    pub lifecycle: Arc<LifecycleManager>,
    /// Memory bounds configuration
    pub bounds: Arc<MemoryBounds>,
    /// Resilience manager for graceful degradation
    pub resilience: ResilienceManager,
    /// Feedback recorder for the inference training loop
    pub feedback_recorder: Option<Arc<FeedbackRecorder>>,
    /// Weight adjuster for applying feedback to the substrate
    pub weight_adjuster: Option<Arc<WeightAdjuster>>,
    /// Multi-strategy embedding store (spring + node2vec + spectral)
    pub multi_embeddings: Arc<RwLock<MultiEmbeddingStore>>,
    /// Blend tuner: learns optimal embedding blend weights from feedback
    pub blend_tuner: Option<Arc<BlendTuner>>,
    /// VKB session store
    pub session_store: Option<Arc<RwLock<MemorySessionStore>>>,
    /// Local keypair for signing triples
    pub keypair: Arc<Keypair>,
    /// Strategy selector: monitors ingestion rate and queues batch recomputes
    pub strategy_selector: Arc<StrategySelector>,
}

impl ValenceEngine {
    /// Create a new ValenceEngine with empty stores and default lifecycle settings
    pub fn new() -> Self {
        let store = MemoryStore::new();
        let store_arc: Arc<dyn TripleStore> = Arc::new(store.clone());
        let access_tracker = Arc::new(AccessTracker::new());
        
        // Initialize feedback recorder
        let feedback_recorder = Some(Arc::new(FeedbackRecorder::new()));
        
        // Initialize weight adjuster with stigmergy integration
        // Wrap the store in the format weight_adjuster expects
        let store_boxed: Arc<RwLock<Box<dyn TripleStore>>> = 
            Arc::new(RwLock::new(Box::new(store)));
        let weight_adjuster = Some(Arc::new(WeightAdjuster::with_config(
            store_boxed,
            WeightAdjusterConfig::default(),
            Some(access_tracker.clone()),
        )));

        Self {
            store: store_arc,
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
            multi_embeddings: Arc::new(RwLock::new(MultiEmbeddingStore::new(64))),
            access_tracker,
            lifecycle: Arc::new(LifecycleManager::with_defaults()),
            bounds: Arc::new(MemoryBounds::default()),
            resilience: ResilienceManager::new(),
            feedback_recorder,
            weight_adjuster,
            blend_tuner: Some(Arc::new(BlendTuner::new())),
            session_store: Some(Arc::new(RwLock::new(MemorySessionStore::new()))),
            keypair: Arc::new(Keypair::generate()),
            strategy_selector: Arc::new(StrategySelector::new()),
        }
    }

    /// Create a new ValenceEngine with custom lifecycle policy and bounds
    pub fn with_lifecycle(policy: DecayPolicy, bounds: MemoryBounds) -> Self {
        let store = MemoryStore::new();
        let store_arc: Arc<dyn TripleStore> = Arc::new(store.clone());
        let access_tracker = Arc::new(AccessTracker::new());
        
        let feedback_recorder = Some(Arc::new(FeedbackRecorder::new()));
        let store_boxed: Arc<RwLock<Box<dyn TripleStore>>> = 
            Arc::new(RwLock::new(Box::new(store)));
        let weight_adjuster = Some(Arc::new(WeightAdjuster::with_config(
            store_boxed,
            WeightAdjusterConfig::default(),
            Some(access_tracker.clone()),
        )));

        Self {
            store: store_arc,
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
            multi_embeddings: Arc::new(RwLock::new(MultiEmbeddingStore::new(64))),
            access_tracker,
            lifecycle: Arc::new(LifecycleManager::new(policy)),
            bounds: Arc::new(bounds),
            resilience: ResilienceManager::new(),
            feedback_recorder,
            weight_adjuster,
            blend_tuner: Some(Arc::new(BlendTuner::new())),
            session_store: Some(Arc::new(RwLock::new(MemorySessionStore::new()))),
            keypair: Arc::new(Keypair::generate()),
            strategy_selector: Arc::new(StrategySelector::new()),
        }
    }

    /// Create a ValenceEngine from an existing MemoryStore
    pub fn from_store(store: MemoryStore) -> Self {
        let store_arc: Arc<dyn TripleStore> = Arc::new(store.clone());
        let access_tracker = Arc::new(AccessTracker::new());

        let feedback_recorder = Some(Arc::new(FeedbackRecorder::new()));
        let store_boxed: Arc<RwLock<Box<dyn TripleStore>>> =
            Arc::new(RwLock::new(Box::new(store)));
        let weight_adjuster = Some(Arc::new(WeightAdjuster::with_config(
            store_boxed,
            WeightAdjusterConfig::default(),
            Some(access_tracker.clone()),
        )));

        Self {
            store: store_arc,
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
            multi_embeddings: Arc::new(RwLock::new(MultiEmbeddingStore::new(64))),
            access_tracker,
            lifecycle: Arc::new(LifecycleManager::with_defaults()),
            bounds: Arc::new(MemoryBounds::default()),
            resilience: ResilienceManager::new(),
            feedback_recorder,
            weight_adjuster,
            blend_tuner: Some(Arc::new(BlendTuner::new())),
            session_store: Some(Arc::new(RwLock::new(MemorySessionStore::new()))),
            keypair: Arc::new(Keypair::generate()),
            strategy_selector: Arc::new(StrategySelector::new()),
        }
    }

    /// Create a ValenceEngine from any TripleStore implementation
    pub fn from_triple_store<S: TripleStore + 'static>(store: S) -> Self {
        let store_arc: Arc<dyn TripleStore> = Arc::new(store);
        let access_tracker = Arc::new(AccessTracker::new());
        
        // Note: weight_adjuster will be None for non-clonable stores
        // Users should call enable_inference_loop() separately if needed
        
        Self {
            store: store_arc,
            embeddings: Arc::new(RwLock::new(MemoryEmbeddingStore::new())),
            multi_embeddings: Arc::new(RwLock::new(MultiEmbeddingStore::new(64))),
            access_tracker,
            lifecycle: Arc::new(LifecycleManager::with_defaults()),
            bounds: Arc::new(MemoryBounds::default()),
            resilience: ResilienceManager::new(),
            feedback_recorder: Some(Arc::new(FeedbackRecorder::new())),
            weight_adjuster: None,
            blend_tuner: Some(Arc::new(BlendTuner::new())),
            session_store: Some(Arc::new(RwLock::new(MemorySessionStore::new()))),
            keypair: Arc::new(Keypair::generate()),
            strategy_selector: Arc::new(StrategySelector::new()),
        }
    }

    /// Recompute embeddings from the current graph state
    ///
    /// This uses spectral embedding to derive a vector representation
    /// from the graph topology.
    pub async fn recompute_embeddings(&self, dimensions: usize) -> Result<usize> {
        // Compute embeddings from current graph
        match spectral::compute_embeddings(self.store.as_ref(), dimensions).await {
            Ok(embeddings_map) => {
                let count = embeddings_map.len();

                // Replace the embedding store with new embeddings
                let new_store = MemoryEmbeddingStore::from_embeddings(embeddings_map)
                    .context("Failed to create embedding store from computed embeddings")?;

                let mut embeddings = self.embeddings.write().await;
                *embeddings = new_store;

                // Record success
                self.resilience.record_success("embeddings").await;

                Ok(count)
            }
            Err(e) => {
                // Record failure
                self.resilience.record_failure("embeddings", &e.to_string()).await;
                Err(e)
            }
        }
    }

    /// Recompute Node2Vec embeddings from the current graph state
    ///
    /// This uses random-walk-based embeddings to capture local neighborhood
    /// structure, complementing the global structure captured by spectral embeddings.
    pub async fn recompute_node2vec(&self, config: crate::embeddings::node2vec::Node2VecConfig) -> Result<usize> {
        // Compute Node2Vec embeddings from current graph
        let embeddings_map: std::collections::HashMap<crate::models::NodeId, Vec<f32>> = 
            crate::embeddings::node2vec::compute_node2vec(self.store.as_ref(), config)
                .await
                .context("Failed to compute Node2Vec embeddings")?;

        let count = embeddings_map.len();

        // Replace the embedding store with new embeddings
        let new_store = MemoryEmbeddingStore::from_embeddings(embeddings_map)
            .context("Failed to create embedding store from Node2Vec embeddings")?;

        let mut embeddings = self.embeddings.write().await;
        *embeddings = new_store;

        Ok(count)
    }
    /// Recompute spectral embeddings and store them in the multi-embedding store.
    pub async fn recompute_spectral_multi(&self, dimensions: usize) -> Result<usize> {
        let embeddings_map = spectral::compute_embeddings(self.store.as_ref(), dimensions)
            .await
            .context("Failed to compute spectral embeddings")?;
        let count = embeddings_map.len();

        let mut multi = self.multi_embeddings.write().await;
        multi.store_batch(EmbeddingStrategy::Spectral, embeddings_map)?;

        self.resilience.record_success("embeddings_spectral").await;
        Ok(count)
    }

    /// Recompute Node2Vec embeddings and store them in the multi-embedding store.
    pub async fn recompute_node2vec_multi(&self, config: crate::embeddings::node2vec::Node2VecConfig) -> Result<usize> {
        let embeddings_map = crate::embeddings::node2vec::compute_node2vec(self.store.as_ref(), config)
            .await
            .context("Failed to compute Node2Vec embeddings")?;
        let count = embeddings_map.len();

        let mut multi = self.multi_embeddings.write().await;
        multi.store_batch(EmbeddingStrategy::Node2Vec, embeddings_map)?;

        Ok(count)
    }

    /// Spring nudge: after inserting a triple (subject -> object), nudge their
    /// spring embeddings toward each other in the multi-embedding store.
    ///
    /// This is O(1) per edge — just two vector additions. Called automatically
    /// by the engine on each triple insert, or can be called manually.
    ///
    /// `edge_weight` controls nudge strength (typically the triple's weight, default 1.0).
    pub async fn spring_nudge_on_insert(
        &self,
        subject: NodeId,
        object: NodeId,
        edge_weight: f32,
    ) -> Result<bool> {
        // Record the insert for strategy selection
        self.strategy_selector.record_insert();

        // Collect neighbor spring embeddings for lazy init.
        // We use depth=1 neighbors of both subject and object.
        let subj_neighbors = self.store.neighbors(subject, 1).await.unwrap_or_default();
        let obj_neighbors = self.store.neighbors(object, 1).await.unwrap_or_default();

        let multi = self.multi_embeddings.read().await;
        let mut neighbor_embs = std::collections::HashMap::new();

        // Collect spring embeddings of all neighbors
        for triple in subj_neighbors.iter().chain(obj_neighbors.iter()) {
            for &node_id in &[triple.subject, triple.object] {
                if let Some(emb) = multi.get_strategy(node_id, EmbeddingStrategy::Spring) {
                    neighbor_embs.insert(node_id, emb.clone());
                }
            }
        }
        drop(multi);

        let mut multi = self.multi_embeddings.write().await;
        let updated = multi.spring_nudge(subject, object, edge_weight, &neighbor_embs);

        Ok(updated)
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

    /// Run stigmergy reinforcement: create edges based on co-access patterns.
    ///
    /// This creates structural edges between frequently co-accessed triples,
    /// making the graph topology reflect usage patterns.
    ///
    /// Returns the number of new edges created.
    pub async fn run_stigmergy_reinforcement(&self) -> Result<u64> {
        use crate::stigmergy::CoRetrievalEngine;

        let engine = CoRetrievalEngine::new(
            self.store.clone(),
            self.access_tracker.clone(),
        );

        engine.reinforce().await
    }

    /// Run a full stigmergy maintenance cycle: reinforce then decay.
    ///
    /// This creates edges based on current frequent co-access patterns,
    /// then applies decay to the access tracker.
    ///
    /// Returns (edges_created, events_decayed).
    pub async fn run_stigmergy_maintenance(&self) -> Result<(u64, usize)> {
        use crate::stigmergy::CoRetrievalEngine;

        let engine = CoRetrievalEngine::new(
            self.store.clone(),
            self.access_tracker.clone(),
        );

        engine.run_maintenance_cycle().await
    }
    
    /// Run a full lifecycle cycle: structural decay + eviction.
    ///
    /// This applies decay considering structural properties (sources, centrality),
    /// then enforces memory bounds by evicting lowest-weight triples.
    ///
    /// Returns (decay_result, enforce_result).
    pub async fn run_lifecycle_cycle(&self) -> Result<(crate::lifecycle::DecayCycleResult, crate::lifecycle::EnforceResult)> {
        // Run structural decay
        let decay_result = self.lifecycle.decay_cycle(self).await?;
        
        // Enforce memory bounds
        let enforce_result = self.bounds.enforce(&*self.store).await?;
        
        Ok((decay_result, enforce_result))
    }
    
    /// Check lifecycle status (bounds and utilization).
    pub async fn lifecycle_status(&self) -> Result<crate::lifecycle::BoundsStatus> {
        self.bounds.check(&*self.store).await
    }
    
    /// Get a reference to the feedback recorder (if enabled).
    pub fn feedback_recorder(&self) -> Option<Arc<FeedbackRecorder>> {
        self.feedback_recorder.clone()
    }
    
    /// Get a reference to the weight adjuster (if enabled).
    pub fn weight_adjuster(&self) -> Option<Arc<WeightAdjuster>> {
        self.weight_adjuster.clone()
    }

    /// Get a reference to the blend tuner (if enabled).
    pub fn blend_tuner(&self) -> Option<Arc<BlendTuner>> {
        self.blend_tuner.clone()
    }

    /// Execute the combined "connected AND similar" query.
    ///
    /// Finds nodes that are both graph-connected to `anchor` AND
    /// embedding-similar to `target`. This is the killer query that
    /// combines topology and embedding similarity in a single operation.
    pub async fn combined_query(
        &self,
        params: crate::query::combined::CombinedQueryParams,
    ) -> Result<crate::query::combined::CombinedQueryResponse> {
        crate::query::combined::combined_query(self, params).await
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

    #[tokio::test]
    async fn test_stigmergy_integration() {
        let engine = ValenceEngine::new();

        // Create some triples
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let charlie = engine.store.find_or_create_node("Charlie").await.unwrap();
        let diana = engine.store.find_or_create_node("Diana").await.unwrap();

        let t1 = Triple::new(alice.id, "knows", bob.id);
        let t2 = Triple::new(charlie.id, "knows", diana.id);

        let id1 = engine.store.insert_triple(t1).await.unwrap();
        let id2 = engine.store.insert_triple(t2).await.unwrap();

        // Record co-accesses (threshold is 3 by default)
        for _ in 0..5 {
            engine.access_tracker
                .record_access(&[id1, id2], "test_query")
                .await;
        }

        // Initially just 2 triples
        assert_eq!(engine.store.count_triples().await.unwrap(), 2);

        // Run stigmergy reinforcement
        let created = engine.run_stigmergy_reinforcement().await.unwrap();

        // Should create 2 co-retrieval edges
        assert_eq!(created, 2);

        // Now should have 4 triples total (original 2 + 2 co-retrieval edges)
        assert_eq!(engine.store.count_triples().await.unwrap(), 4);
    }

    #[tokio::test]
    async fn test_stigmergy_maintenance_cycle() {
        let engine = ValenceEngine::new();

        // Create triples
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let charlie = engine.store.find_or_create_node("Charlie").await.unwrap();
        let diana = engine.store.find_or_create_node("Diana").await.unwrap();

        let t1 = Triple::new(alice.id, "knows", bob.id);
        let t2 = Triple::new(charlie.id, "knows", diana.id);

        let id1 = engine.store.insert_triple(t1).await.unwrap();
        let id2 = engine.store.insert_triple(t2).await.unwrap();

        // Record co-accesses
        for i in 0..5 {
            engine.access_tracker
                .record_access(&[id1, id2], &format!("query_{}", i))
                .await;
        }

        // Run full maintenance cycle
        let (created, decayed) = engine.run_stigmergy_maintenance().await.unwrap();

        // Should create 2 co-retrieval edges
        assert_eq!(created, 2);

        // Events don't decay unless they're old (24 hours by default)
        assert_eq!(decayed, 0);
    }

    #[tokio::test]
    async fn test_recompute_node2vec() {
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

        // Compute Node2Vec embeddings
        let config = crate::embeddings::node2vec::Node2VecConfig {
            dimensions: 8,
            walk_length: 10,
            walks_per_node: 5,
            epochs: 3,
            ..Default::default()
        };

        let count = engine.recompute_node2vec(config).await.unwrap();

        // Should have 3 embeddings (one per node)
        assert_eq!(count, 3);
        assert!(engine.has_embeddings().await);
        assert_eq!(engine.embedding_count().await, 3);

        // Verify we can get an embedding
        let embeddings = engine.embeddings.read().await;
        let emb_a = embeddings.get(a.id);
        assert!(emb_a.is_some());
        assert_eq!(emb_a.unwrap().len(), 8);
    }

    // === Spring Model Integration Tests ===

    #[tokio::test]
    async fn test_spring_nudge_on_insert() {
        let engine = ValenceEngine::new();

        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();

        // Spring nudge on the new edge
        let updated = engine.spring_nudge_on_insert(a.id, b.id, 1.0).await.unwrap();
        assert!(updated, "Spring nudge should have initialized and updated embeddings");

        // Both nodes should now have spring embeddings in the multi store
        let multi = engine.multi_embeddings.read().await;
        let emb_a = multi.get_strategy(a.id, crate::embeddings::spring::EmbeddingStrategy::Spring);
        let emb_b = multi.get_strategy(b.id, crate::embeddings::spring::EmbeddingStrategy::Spring);
        assert!(emb_a.is_some(), "A should have a spring embedding after nudge");
        assert!(emb_b.is_some(), "B should have a spring embedding after nudge");
        assert_eq!(emb_a.unwrap().len(), 64);
    }

    #[tokio::test]
    async fn test_spring_nudge_convergence_via_engine() {
        let engine = ValenceEngine::new();

        let a = engine.store.find_or_create_node("A").await.unwrap();
        let b = engine.store.find_or_create_node("B").await.unwrap();

        engine.store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();

        // First nudge initializes
        engine.spring_nudge_on_insert(a.id, b.id, 1.0).await.unwrap();

        // Get initial similarity
        let initial_sim = {
            let multi = engine.multi_embeddings.read().await;
            let ea = multi.get_strategy(a.id, crate::embeddings::spring::EmbeddingStrategy::Spring).unwrap();
            let eb = multi.get_strategy(b.id, crate::embeddings::spring::EmbeddingStrategy::Spring).unwrap();
            cosine_sim(ea, eb)
        };

        // Nudge multiple times
        for _ in 0..10 {
            engine.spring_nudge_on_insert(a.id, b.id, 1.0).await.unwrap();
        }

        let final_sim = {
            let multi = engine.multi_embeddings.read().await;
            let ea = multi.get_strategy(a.id, crate::embeddings::spring::EmbeddingStrategy::Spring).unwrap();
            let eb = multi.get_strategy(b.id, crate::embeddings::spring::EmbeddingStrategy::Spring).unwrap();
            cosine_sim(ea, eb)
        };

        assert!(
            final_sim > initial_sim,
            "Repeated nudges should increase similarity: initial={:.3}, final={:.3}",
            initial_sim, final_sim
        );
    }

    #[tokio::test]
    async fn test_multi_embedding_store_spectral_and_spring() {
        // Use a graph large enough for 64-dim spectral embeddings (need >64 nodes)
        // Instead, use a small graph and a matching small dimension for the multi store
        let engine = ValenceEngine::new();

        // Create enough nodes for meaningful spectral embeddings
        // With 3 nodes, spectral produces 2-dim embeddings (node_count - 1).
        // So we create the multi store with matching dimensions.
        let mut nodes = Vec::new();
        for i in 0..5 {
            let n = engine.store.find_or_create_node(&format!("N{}", i)).await.unwrap();
            nodes.push(n);
        }
        // Create a connected graph (ring + some cross-edges)
        for i in 0..5 {
            engine.store.insert_triple(Triple::new(nodes[i].id, "connects", nodes[(i+1)%5].id)).await.unwrap();
        }
        engine.store.insert_triple(Triple::new(nodes[0].id, "connects", nodes[2].id)).await.unwrap();

        // Spectral on 5-node graph produces up to 4-dim embeddings
        // The multi store has 64 dims, so spectral with < 64 nodes will produce fewer dims.
        // We need to match dimensions. Let's recompute spectral with dims=4 and
        // use a multi store with dims=4.
        {
            let mut multi = engine.multi_embeddings.write().await;
            *multi = crate::embeddings::spring::MultiEmbeddingStore::new(4);
        }

        let spectral_count = engine.recompute_spectral_multi(4).await.unwrap();
        assert_eq!(spectral_count, 5);

        // Nudge spring on each edge
        for i in 0..5 {
            engine.spring_nudge_on_insert(nodes[i].id, nodes[(i+1)%5].id, 1.0).await.unwrap();
        }

        // Both strategies should be present
        let multi = engine.multi_embeddings.read().await;
        for node in &nodes {
            assert!(
                multi.get_strategy(node.id, crate::embeddings::spring::EmbeddingStrategy::Spectral).is_some(),
                "Node should have spectral embedding"
            );
            assert!(
                multi.get_strategy(node.id, crate::embeddings::spring::EmbeddingStrategy::Spring).is_some(),
                "Node should have spring embedding"
            );
        }
    }

    fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag_a == 0.0 || mag_b == 0.0 { 0.0 } else { dot / (mag_a * mag_b) }
    }
}
