//! WorkingSet: the active subgraph for a query or conversation.
//!
//! A WorkingSet represents the conceptual threads currently in play. It's built
//! from a query by finding semantically similar nodes via embeddings, then expanding
//! via graph neighbors to form a coherent local view of relevant knowledge.
//!
//! ## Session-Scoped Updates
//!
//! The working set evolves turn-by-turn:
//! - Active threads strengthen
//! - Resolved threads compress to decisions
//! - New threads get added
//! - Dormant threads weaken (but don't disappear)
//!
//! This provides conceptual continuity without the token cost of full message history.

use std::collections::{HashSet, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Result, Context as AnyhowContext};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

use crate::{
    engine::ValenceEngine,
    embeddings::EmbeddingStore,
    models::{NodeId, Triple, TripleId},
    storage::TripleStore,
    budget::OperationBudget,
};

/// Strength/relevance score for a node or triple in the working set
///
/// Combines:
/// - Base confidence from the knowledge graph
/// - Recency (when was it last mentioned/accessed)
/// - Activation (how many times referenced in this session)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationScore {
    /// Base confidence from the graph
    pub confidence: f64,
    /// Recency score (0.0 = ancient, 1.0 = just mentioned)
    pub recency: f64,
    /// Activation count (how many times referenced)
    pub activation_count: u32,
    /// Last accessed timestamp (Unix epoch ms)
    pub last_accessed_ms: u64,
    /// Combined score (weighted combination)
    pub combined: f64,
}

impl ActivationScore {
    /// Create a new activation score with initial confidence
    pub fn new(confidence: f64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            confidence,
            recency: 1.0, // Fresh
            activation_count: 1,
            last_accessed_ms: now,
            combined: confidence,
        }
    }

    /// Update recency based on time elapsed since last access
    ///
    /// Uses exponential decay: recency = exp(-elapsed_ms / half_life_ms)
    pub fn update_recency(&mut self, half_life_ms: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let elapsed_ms = now.saturating_sub(self.last_accessed_ms);
        
        // Exponential decay
        self.recency = (-1.0 * elapsed_ms as f64 / half_life_ms as f64).exp();
        
        self.recompute_combined();
    }

    /// Mark as accessed/activated (strengthens the score)
    pub fn activate(&mut self) {
        self.activation_count += 1;
        self.recency = 1.0; // Fresh again
        self.last_accessed_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.recompute_combined();
    }

    /// Recompute the combined score from components
    ///
    /// Weighted combination:
    /// - confidence: 0.5
    /// - recency: 0.3
    /// - activation frequency: 0.2
    fn recompute_combined(&mut self) {
        // Normalize activation count to [0, 1] using log scaling
        let activation_norm = (1.0 + self.activation_count as f64).ln() / (1.0 + 10.0_f64).ln();
        
        self.combined = 
            0.5 * self.confidence + 
            0.3 * self.recency + 
            0.2 * activation_norm;
    }
}

/// A thread represents an active topic, question, or decision in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationThread {
    /// Unique identifier for this thread
    pub id: Uuid,
    /// Thread type
    pub thread_type: ThreadType,
    /// Description/summary of the thread
    pub description: String,
    /// Nodes involved in this thread
    pub nodes: HashSet<NodeId>,
    /// Activation score
    pub score: ActivationScore,
    /// Status
    pub status: ThreadStatus,
}

/// Type of conversation thread
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadType {
    /// An active topic being discussed
    Topic,
    /// An open question awaiting answer
    Question,
    /// A decision that was made
    Decision,
    /// Context/background information
    Context,
}

/// Status of a conversation thread
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadStatus {
    /// Currently active in conversation
    Active,
    /// Dormant (was active but conversation moved on)
    Dormant,
    /// Resolved (question answered, decision made)
    Resolved,
    /// Archived (compressed/stored, removed from working set)
    Archived,
}

/// A working set is the active subgraph for a query or conversation.
///
/// It contains:
/// - The set of active node IDs with activation scores
/// - The triples connecting them with confidence scores
/// - Conversation threads (topics, questions, decisions)
/// - Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingSet {
    /// Session identifier (optional, for session-scoped working sets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    
    /// Turn counter (increments with each update)
    pub turn: u32,
    
    /// Active nodes in this working set with activation scores
    pub nodes: HashMap<NodeId, ActivationScore>,
    
    /// Triples in this working set with activation scores
    pub triples: HashMap<TripleId, (Triple, ActivationScore)>,
    
    /// Active conversation threads
    pub threads: HashMap<Uuid, ConversationThread>,
    
    /// Decay half-life in milliseconds (default: 5 minutes)
    pub decay_half_life_ms: u64,
}

impl WorkingSet {
    /// Create an empty working set
    pub fn new() -> Self {
        Self {
            session_id: None,
            turn: 0,
            nodes: HashMap::new(),
            triples: HashMap::new(),
            threads: HashMap::new(),
            decay_half_life_ms: 5 * 60 * 1000, // 5 minutes
        }
    }

    /// Create a new session-scoped working set
    pub fn new_session(session_id: Uuid) -> Self {
        Self {
            session_id: Some(session_id),
            turn: 0,
            nodes: HashMap::new(),
            triples: HashMap::new(),
            threads: HashMap::new(),
            decay_half_life_ms: 5 * 60 * 1000,
        }
    }

    /// Add a node to the working set with initial confidence
    pub fn add_node(&mut self, node_id: NodeId, confidence: f64) {
        self.nodes
            .entry(node_id)
            .and_modify(|score| score.activate())
            .or_insert_with(|| ActivationScore::new(confidence));
    }

    /// Add a triple to the working set with its confidence score
    pub fn add_triple(&mut self, triple: Triple, confidence: f64) {
        self.triples
            .entry(triple.id)
            .and_modify(|(_, score)| score.activate())
            .or_insert_with(|| (triple, ActivationScore::new(confidence)));
    }

    /// Check if a node is in the working set
    pub fn contains_node(&self, node_id: NodeId) -> bool {
        self.nodes.contains_key(&node_id)
    }

    /// Check if a triple is in the working set
    pub fn contains_triple(&self, triple_id: TripleId) -> bool {
        self.triples.contains_key(&triple_id)
    }

    /// Get the number of nodes in the working set
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of triples in the working set
    pub fn triple_count(&self) -> usize {
        self.triples.len()
    }

    /// Get the number of active threads
    pub fn active_thread_count(&self) -> usize {
        self.threads
            .values()
            .filter(|t| t.status == ThreadStatus::Active)
            .count()
    }

    /// Add a conversation thread
    pub fn add_thread(
        &mut self,
        thread_type: ThreadType,
        description: String,
        nodes: HashSet<NodeId>,
    ) -> Uuid {
        let thread = ConversationThread {
            id: Uuid::new_v4(),
            thread_type,
            description,
            nodes,
            score: ActivationScore::new(0.8), // Initial high activation
            status: ThreadStatus::Active,
        };
        let id = thread.id;
        self.threads.insert(id, thread);
        id
    }

    /// Mark a thread as resolved
    pub fn resolve_thread(&mut self, thread_id: Uuid) {
        if let Some(thread) = self.threads.get_mut(&thread_id) {
            thread.status = ThreadStatus::Resolved;
        }
    }

    /// Update from a new turn in the conversation
    ///
    /// This is called after each exchange to:
    /// - Apply decay to all nodes/triples/threads
    /// - Prune weak/dormant items
    /// - Archive resolved threads
    /// - Update thread statuses
    pub fn update_turn(&mut self, prune_threshold: f64) {
        self.turn += 1;

        // Update recency for all nodes
        for score in self.nodes.values_mut() {
            score.update_recency(self.decay_half_life_ms);
        }

        // Update recency for all triples
        for (_, score) in self.triples.values_mut() {
            score.update_recency(self.decay_half_life_ms);
        }

        // Update thread scores and status
        for thread in self.threads.values_mut() {
            thread.score.update_recency(self.decay_half_life_ms);
            
            // Mark as dormant if recency drops below threshold
            if thread.status == ThreadStatus::Active && thread.score.recency < 0.3 {
                thread.status = ThreadStatus::Dormant;
            }
        }

        // Prune weak nodes
        self.nodes.retain(|_, score| score.combined >= prune_threshold);

        // Prune weak triples
        self.triples.retain(|_, (_, score)| score.combined >= prune_threshold);

        // Archive resolved threads with low activation
        self.threads.retain(|_, thread| {
            !(thread.status == ThreadStatus::Resolved && thread.score.combined < prune_threshold)
        });
    }

    /// Activate specific nodes (marks them as recently accessed)
    pub fn activate_nodes(&mut self, node_ids: &[NodeId]) {
        for node_id in node_ids {
            if let Some(score) = self.nodes.get_mut(node_id) {
                score.activate();
            }
        }
    }

    /// Build a working set from a query string (convenience wrapper).
    ///
    /// Creates a default budget and calls `from_query_with_budget`.
    ///
    /// # Arguments
    ///
    /// * `engine` - The ValenceEngine to query
    /// * `query` - The query string (matched against node values)
    /// * `k` - Number of nearest neighbors to find
    ///
    /// # Returns
    ///
    /// A WorkingSet containing the relevant subgraph
    pub async fn from_query(
        engine: &ValenceEngine,
        query: &str,
        k: usize,
    ) -> Result<Self> {
        let budget = OperationBudget::new(5000, 3, k * 5);
        Self::from_query_with_budget(engine, query, k, budget).await
    }

    /// Build a working set from a query string with explicit budget control.
    ///
    /// Process (with graceful degradation):
    /// 1. Find query node by value
    /// 2. Try embedding similarity search first
    /// 3. Fall back to graph traversal if embeddings don't exist
    /// 4. Expand via graph neighbors (1-2 hops from each result)
    /// 5. Include confidence scores for each triple
    ///
    /// # Arguments
    ///
    /// * `engine` - The ValenceEngine to query
    /// * `query` - The query string (matched against node values)
    /// * `k` - Number of nearest neighbors to find
    /// * `budget` - Operation budget for bounded retrieval
    ///
    /// # Returns
    ///
    /// A WorkingSet containing the relevant subgraph
    pub async fn from_query_with_budget(
        engine: &ValenceEngine,
        query: &str,
        k: usize,
        budget: OperationBudget,
    ) -> Result<Self> {
        let query_node = engine
            .store
            .find_node_by_value(query)
            .await?
            .context("Query node not found")?;

        // Try embedding search first
        let embeddings_store = engine.embeddings.read().await;
        let query_embedding = embeddings_store.get(query_node.id);

        if let Some(embedding) = query_embedding {
            // Warm mode: use embedding similarity
            let k = budget.check_results(0).then_some(20).unwrap_or(10);
            let neighbors = embeddings_store
                .query_nearest(embedding, k)
                .context("Failed to query nearest neighbors")?;
            
            drop(embeddings_store); // Release lock before async operations

            let mut working_set = WorkingSet::new();
            working_set.add_node(query_node.id, 1.0);

            // Add neighbor nodes and expand via graph
            for (node_id, similarity) in neighbors {
                if budget.is_exhausted() {
                    break;
                }

                working_set.add_node(node_id, similarity as f64);

                // Expand 1 hop from this node
                if budget.check_hop(1) {
                    let first_hop = engine
                        .store
                        .neighbors(node_id, 1)
                        .await
                        .context("Failed to get first-hop neighbors")?;

                    for triple in first_hop {
                        if budget.is_exhausted() || !budget.check_results(working_set.triple_count()) {
                            break;
                        }

                        working_set.add_node(triple.subject, triple.weight);
                        working_set.add_node(triple.object, triple.weight);
                        working_set.add_triple(triple.clone(), triple.weight);
                    }
                }

                // Second hop: neighbors of neighbors (but limit expansion)
                if budget.check_hop(2) && working_set.node_count() < k * 3 {
                    let second_hop = engine
                        .store
                        .neighbors(node_id, 2)
                        .await
                        .context("Failed to get second-hop neighbors")?;

                    for triple in second_hop {
                        if budget.is_exhausted() 
                            || !budget.check_results(working_set.triple_count())
                            || working_set.node_count() >= k * 5 
                        {
                            break;
                        }

                        working_set.add_node(triple.subject, triple.weight * 0.7);
                        working_set.add_node(triple.object, triple.weight * 0.7);

                        // Second hop triples get lower confidence (decay)
                        let decayed_confidence = triple.weight * 0.5;
                        working_set.add_triple(triple.clone(), decayed_confidence);
                    }
                }
            }

            Ok(working_set)
        } else {
            // Cold mode: fall back to graph-only traversal
            drop(embeddings_store);
            
            tracing::warn!(
                "No embedding found for query node '{}', falling back to graph-based traversal",
                query
            );
            
            Self::from_query_graph_only_with_budget(engine, query, 2, budget).await
        }
    }

    /// Build a working set using pure graph traversal (no embeddings, convenience wrapper).
    ///
    /// Creates a default budget and calls `from_query_graph_only_with_budget`.
    ///
    /// # Arguments
    ///
    /// * `engine` - The ValenceEngine to query
    /// * `query` - The query string (matched against node values)
    /// * `depth` - Maximum graph traversal depth
    ///
    /// # Returns
    ///
    /// A WorkingSet containing the graph neighborhood
    pub async fn from_query_graph_only(
        engine: &ValenceEngine,
        query: &str,
        depth: u32,
    ) -> Result<Self> {
        let budget = OperationBudget::new(5000, depth, 100);
        Self::from_query_graph_only_with_budget(engine, query, depth, budget).await
    }

    /// Build a working set using pure graph traversal with explicit budget control.
    ///
    /// This is the cold engine fallback — works without any embeddings by
    /// traversing the graph starting from the query node.
    ///
    /// # Arguments
    ///
    /// * `engine` - The ValenceEngine to query
    /// * `query` - The query string (matched against node values)
    /// * `depth` - Maximum graph traversal depth
    /// * `budget` - Operation budget
    ///
    /// # Returns
    ///
    /// A WorkingSet containing the graph neighborhood
    pub async fn from_query_graph_only_with_budget(
        engine: &ValenceEngine,
        query: &str,
        depth: u32,
        budget: OperationBudget,
    ) -> Result<Self> {
        let mut working_set = WorkingSet::new();

        // Find query node
        let query_node = engine
            .store
            .find_node_by_value(query)
            .await?
            .context("Query node not found")?;

        working_set.add_node(query_node.id, 1.0);

        // Graph traversal from query node
        // Check if we're allowed to traverse (depth - 1 because check_hop is 0-indexed)
        if budget.check_hop(depth - 1) {
            let triples = engine
                .store
                .neighbors(query_node.id, depth)
                .await
                .context("Failed to traverse graph neighbors")?;

            for triple in triples {
                if budget.is_exhausted() || !budget.check_results(working_set.triple_count()) {
                    break;
                }

                working_set.add_node(triple.subject, triple.weight);
                working_set.add_node(triple.object, triple.weight);
                working_set.add_triple(triple.clone(), triple.weight);
            }
        }

        Ok(working_set)
    }

    /// Serialize to a text summary suitable for LLM context
    ///
    /// Formats the working set as structured text that provides:
    /// - Active threads (topics, questions, decisions)
    /// - Key nodes and relationships
    /// - Conceptual continuity without full message history
    pub fn to_context_summary(&self) -> String {
        let mut output = String::new();

        // Session metadata
        if let Some(session_id) = self.session_id {
            output.push_str(&format!("Session: {}\n", session_id));
        }
        output.push_str(&format!("Turn: {}\n\n", self.turn));

        // Active threads
        let active_threads: Vec<_> = self
            .threads
            .values()
            .filter(|t| t.status == ThreadStatus::Active)
            .collect();

        if !active_threads.is_empty() {
            output.push_str("## Active Threads\n\n");
            for thread in active_threads {
                let type_icon = match thread.thread_type {
                    ThreadType::Topic => "💬",
                    ThreadType::Question => "❓",
                    ThreadType::Decision => "✅",
                    ThreadType::Context => "📋",
                };
                output.push_str(&format!(
                    "{} {}\n  Score: {:.2}, Nodes: {}\n\n",
                    type_icon,
                    thread.description,
                    thread.score.combined,
                    thread.nodes.len()
                ));
            }
        }

        // Recent decisions
        let decisions: Vec<_> = self
            .threads
            .values()
            .filter(|t| t.thread_type == ThreadType::Decision && t.status == ThreadStatus::Resolved)
            .collect();

        if !decisions.is_empty() {
            output.push_str("## Recent Decisions\n\n");
            for decision in decisions {
                output.push_str(&format!("✓ {}\n", decision.description));
            }
            output.push_str("\n");
        }

        // Top nodes by activation
        let mut top_nodes: Vec<_> = self.nodes.iter().collect();
        top_nodes.sort_by(|a, b| b.1.combined.partial_cmp(&a.1.combined).unwrap());
        
        if !top_nodes.is_empty() {
            output.push_str("## Key Concepts\n\n");
            for (node_id, score) in top_nodes.iter().take(10) {
                output.push_str(&format!(
                    "- {} (score: {:.2}, refs: {})\n",
                    node_id, score.combined, score.activation_count
                ));
            }
            output.push_str("\n");
        }

        // Summary stats
        output.push_str(&format!(
            "---\nNodes: {} | Triples: {} | Active Threads: {}\n",
            self.node_count(),
            self.triple_count(),
            self.active_thread_count()
        ));

        output
    }

    /// Serialize the working set to JSON for inspection
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize working set to JSON")
    }
}

impl Default for WorkingSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Triple;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_empty_working_set() {
        let ws = WorkingSet::new();
        assert_eq!(ws.node_count(), 0);
        assert_eq!(ws.triple_count(), 0);
        assert_eq!(ws.turn, 0);
    }

    #[test]
    fn test_session_scoped() {
        let session_id = Uuid::new_v4();
        let ws = WorkingSet::new_session(session_id);
        assert_eq!(ws.session_id, Some(session_id));
        assert_eq!(ws.turn, 0);
    }

    #[test]
    fn test_add_node() {
        let mut ws = WorkingSet::new();
        let node_id = Uuid::new_v4();
        
        ws.add_node(node_id, 0.8);
        assert_eq!(ws.node_count(), 1);
        assert!(ws.contains_node(node_id));
        
        let score = ws.nodes.get(&node_id).unwrap();
        assert_eq!(score.confidence, 0.8);
        assert_eq!(score.activation_count, 1);
    }

    #[test]
    fn test_node_activation() {
        let mut ws = WorkingSet::new();
        let node_id = Uuid::new_v4();
        
        ws.add_node(node_id, 0.8);
        ws.add_node(node_id, 0.8); // Second reference
        
        let score = ws.nodes.get(&node_id).unwrap();
        assert_eq!(score.activation_count, 2);
    }

    #[test]
    fn test_add_triple() {
        let mut ws = WorkingSet::new();
        let s = Uuid::new_v4();
        let o = Uuid::new_v4();
        let triple = Triple::new(s, "knows", o);
        
        ws.add_triple(triple.clone(), 0.85);
        assert_eq!(ws.triple_count(), 1);
        assert!(ws.contains_triple(triple.id));
        
        let (stored_triple, score) = ws.triples.get(&triple.id).unwrap();
        assert_eq!(stored_triple.id, triple.id);
        assert_eq!(score.confidence, 0.85);
    }

    #[test]
    fn test_thread_management() {
        let mut ws = WorkingSet::new();
        let nodes = HashSet::from([Uuid::new_v4(), Uuid::new_v4()]);
        
        let thread_id = ws.add_thread(
            ThreadType::Question,
            "How does X work?".to_string(),
            nodes.clone(),
        );
        
        assert_eq!(ws.threads.len(), 1);
        assert_eq!(ws.active_thread_count(), 1);
        
        let thread = ws.threads.get(&thread_id).unwrap();
        assert_eq!(thread.thread_type, ThreadType::Question);
        assert_eq!(thread.status, ThreadStatus::Active);
        
        // Resolve the thread
        ws.resolve_thread(thread_id);
        let thread = ws.threads.get(&thread_id).unwrap();
        assert_eq!(thread.status, ThreadStatus::Resolved);
        assert_eq!(ws.active_thread_count(), 0);
    }

    #[test]
    fn test_update_turn_decay() {
        let mut ws = WorkingSet::new();
        ws.decay_half_life_ms = 100; // 100ms half-life for testing
        
        let node_id = Uuid::new_v4();
        ws.add_node(node_id, 0.8);
        
        let initial_score = ws.nodes.get(&node_id).unwrap().combined;
        
        // Wait for decay
        thread::sleep(Duration::from_millis(150));
        
        // Update turn to apply decay
        ws.update_turn(0.1);
        
        let decayed_score = ws.nodes.get(&node_id).unwrap().combined;
        assert!(decayed_score < initial_score, "Score should decay over time");
    }

    #[test]
    fn test_update_turn_pruning() {
        let mut ws = WorkingSet::new();
        ws.decay_half_life_ms = 50; // Short half-life
        
        let node_id = Uuid::new_v4();
        ws.add_node(node_id, 0.3); // Low confidence
        
        // Wait for significant decay
        thread::sleep(Duration::from_millis(200));
        
        // Update turn with pruning threshold
        ws.update_turn(0.5);
        
        // Node should be pruned due to low combined score
        assert_eq!(ws.node_count(), 0, "Weak node should be pruned");
    }

    #[test]
    fn test_activate_nodes() {
        let mut ws = WorkingSet::new();
        let node_id = Uuid::new_v4();
        
        ws.add_node(node_id, 0.8);
        
        let initial_count = ws.nodes.get(&node_id).unwrap().activation_count;
        
        ws.activate_nodes(&[node_id]);
        
        let updated_count = ws.nodes.get(&node_id).unwrap().activation_count;
        assert_eq!(updated_count, initial_count + 1);
    }

    #[test]
    fn test_context_summary() {
        let mut ws = WorkingSet::new_session(Uuid::new_v4());
        
        // Add some content
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        ws.add_node(node1, 0.9);
        ws.add_node(node2, 0.8);
        
        let nodes = HashSet::from([node1, node2]);
        ws.add_thread(
            ThreadType::Topic,
            "Discussing Rust async patterns".to_string(),
            nodes.clone(),
        );
        
        let summary = ws.to_context_summary();
        
        assert!(summary.contains("Session:"));
        assert!(summary.contains("Turn: 0"));
        assert!(summary.contains("Active Threads"));
        assert!(summary.contains("Discussing Rust async patterns"));
        assert!(summary.contains("Nodes: 2"));
    }

    #[test]
    fn test_serialization() {
        let mut ws = WorkingSet::new();
        let node_id = Uuid::new_v4();
        ws.add_node(node_id, 0.8);
        
        let json = ws.to_json().unwrap();
        assert!(json.contains("nodes"));
        assert!(json.contains("triples"));
        assert!(json.contains("threads"));
    }

    #[tokio::test]
    async fn test_from_query_with_budget() {
        let engine = ValenceEngine::new();

        // Build a small graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(alice.id, "likes", carol.id)).await.unwrap();

        // Recompute embeddings
        engine.recompute_embeddings(4).await.unwrap();

        // Build working set
        let ws = WorkingSet::from_query(&engine, "Alice", 2).await.unwrap();

        // Should have found nodes
        assert!(ws.node_count() > 0);
        assert!(ws.triple_count() > 0);

        // Alice should be in the working set
        assert!(ws.contains_node(alice.id));
    }

    #[tokio::test]
    async fn test_from_query_budget_limits() {
        let engine = ValenceEngine::new();

        // Build a larger graph
        let mut nodes = vec![];
        for i in 0..20 {
            let node = engine.store.find_or_create_node(&format!("Node{}", i)).await.unwrap();
            nodes.push(node);
        }

        // Connect them
        for i in 0..19 {
            engine.store.insert_triple(
                Triple::new(nodes[i].id, "connects_to", nodes[i + 1].id)
            ).await.unwrap();
        }

        engine.recompute_embeddings(4).await.unwrap();

        // Use explicit budget control
        let budget = OperationBudget::new(1000, 1, 5);
        let ws = WorkingSet::from_query_with_budget(&engine, "Node0", 5, budget).await.unwrap();

        // Should respect budget limits
        assert!(ws.triple_count() <= 25, "Should respect result budget");
    }

    #[tokio::test]
    async fn test_from_query_graph_only() {
        let engine = ValenceEngine::new();

        // Build a small graph
        let alice = engine.store.find_or_create_node("Alice").await.unwrap();
        let bob = engine.store.find_or_create_node("Bob").await.unwrap();
        let carol = engine.store.find_or_create_node("Carol").await.unwrap();

        engine.store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();

        // Use graph-only mode (no embeddings needed)
        let ws = WorkingSet::from_query_graph_only(&engine, "Alice", 2).await.unwrap();

        // Should have found nodes via graph traversal
        assert!(ws.node_count() > 0);
        assert!(ws.contains_node(alice.id));
        assert!(ws.triple_count() > 0);
    }

    #[tokio::test]
    async fn test_working_set_empty_graph() {
        let engine = ValenceEngine::new();

        // Create a single node with no connections
        let _lonely = engine.store.find_or_create_node("lonely").await.unwrap();

        // Don't compute embeddings — should fall back to graph traversal
        let result = WorkingSet::from_query(&engine, "lonely", 5).await;
        assert!(result.is_ok());
        
        let ws = result.unwrap();
        // Should have at least the query node
        assert_eq!(ws.node_count(), 1);
    }

    #[tokio::test]
    async fn test_session_workflow() {
        let engine = ValenceEngine::new();
        
        // Build a knowledge graph
        let rust = engine.store.find_or_create_node("Rust").await.unwrap();
        let async_prog = engine.store.find_or_create_node("async programming").await.unwrap();
        let tokio = engine.store.find_or_create_node("Tokio").await.unwrap();
        
        engine.store.insert_triple(Triple::new(rust.id, "supports", async_prog.id)).await.unwrap();
        engine.store.insert_triple(Triple::new(tokio.id, "implements", async_prog.id)).await.unwrap();
        
        engine.recompute_embeddings(4).await.unwrap();
        
        // Start a session
        let session_id = Uuid::new_v4();
        let mut ws = WorkingSet::from_query(&engine, "Rust", 2).await.unwrap();
        ws.session_id = Some(session_id);
        
        // Add a question thread
        let nodes = HashSet::from([rust.id, async_prog.id]);
        let thread_id = ws.add_thread(
            ThreadType::Question,
            "How does async work in Rust?".to_string(),
            nodes,
        );
        
        assert_eq!(ws.active_thread_count(), 1);
        
        // Simulate conversation progressing
        ws.update_turn(0.2);
        assert_eq!(ws.turn, 1);
        
        // Answer the question, resolve thread
        ws.resolve_thread(thread_id);
        
        // Add a decision
        ws.add_thread(
            ThreadType::Decision,
            "Use Tokio for async runtime".to_string(),
            HashSet::from([tokio.id]),
        );
        
        // Generate context summary
        let summary = ws.to_context_summary();
        assert!(summary.contains("Session:"));
        assert!(summary.contains("Turn: 1"));
        assert!(summary.contains("Active Threads"));
    }
}
