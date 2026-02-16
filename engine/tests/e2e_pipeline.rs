//! End-to-end integration test for Valence v2 pipeline.
//!
//! This test exercises the full knowledge substrate workflow:
//! 1. Insert realistic knowledge graph (software project)
//! 2. Compute topology-derived embeddings (spectral + node2vec)
//! 3. Search for relevant context
//! 4. Assemble context with budget constraints
//! 5. Record access feedback
//! 6. Run stigmergy reinforcement (usage → structure)
//! 7. Verify graph topology evolved
//! 8. Run lifecycle decay
//! 9. Verify low-value triples lost weight
//! 10. Confirm the whole pipeline works without panics

use valence_engine::{
    ValenceEngine,
    models::Triple,
    embeddings::{node2vec::Node2VecConfig, EmbeddingStore},
    context::{ContextAssembler, AssemblyConfig, ContextFormat},
    storage::TriplePattern,
};
use anyhow::Result;

/// Build a realistic knowledge graph modeling an agent learning about a software project.
///
/// This creates triples representing:
/// - Project structure (modules, components)
/// - Dependencies between modules
/// - API surface areas
/// - Test coverage
/// - Issue tracking
/// - Recent changes and their authors
///
/// Returns the triple IDs for verification.
async fn build_project_knowledge_graph(engine: &ValenceEngine) -> Result<Vec<uuid::Uuid>> {
    let mut triple_ids = Vec::new();
    
    // === Project structure ===
    
    // Core modules
    let valence = engine.store.find_or_create_node("valence-engine").await?;
    let storage = engine.store.find_or_create_node("storage-module").await?;
    let embeddings = engine.store.find_or_create_node("embeddings-module").await?;
    let graph = engine.store.find_or_create_node("graph-module").await?;
    let api = engine.store.find_or_create_node("api-module").await?;
    let stigmergy_mod = engine.store.find_or_create_node("stigmergy-module").await?;
    let lifecycle = engine.store.find_or_create_node("lifecycle-module").await?;
    
    // Project relationships
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_module", storage.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_module", embeddings.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_module", graph.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_module", api.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_module", stigmergy_mod.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_module", lifecycle.id)).await?);
    
    // === Dependencies ===
    
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "depends_on", storage.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "depends_on", graph.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(api.id, "depends_on", storage.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(api.id, "depends_on", embeddings.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(stigmergy_mod.id, "depends_on", storage.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(lifecycle.id, "depends_on", storage.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(lifecycle.id, "depends_on", graph.id)).await?);
    
    // === Key concepts ===
    
    let triple_concept = engine.store.find_or_create_node("Triple").await?;
    let node_concept = engine.store.find_or_create_node("Node").await?;
    let embedding_concept = engine.store.find_or_create_node("Embedding").await?;
    let decay_concept = engine.store.find_or_create_node("Decay").await?;
    let stigmergy_concept = engine.store.find_or_create_node("Stigmergy").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(storage.id, "defines", triple_concept.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(storage.id, "defines", node_concept.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "defines", embedding_concept.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(lifecycle.id, "implements", decay_concept.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(stigmergy_mod.id, "implements", stigmergy_concept.id)).await?);
    
    // === Algorithms ===
    
    let spectral = engine.store.find_or_create_node("spectral-embedding").await?;
    let node2vec = engine.store.find_or_create_node("node2vec-embedding").await?;
    let co_retrieval = engine.store.find_or_create_node("co-retrieval-clustering").await?;
    let structural_decay = engine.store.find_or_create_node("structural-decay").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "implements", spectral.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "implements", node2vec.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(stigmergy_mod.id, "implements", co_retrieval.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(lifecycle.id, "implements", structural_decay.id)).await?);
    
    triple_ids.push(engine.store.insert_triple(Triple::new(spectral.id, "uses", graph.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(node2vec.id, "uses", graph.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(co_retrieval.id, "creates", triple_concept.id)).await?);
    
    // === Test coverage ===
    
    let integration_tests = engine.store.find_or_create_node("integration-tests").await?;
    let unit_tests = engine.store.find_or_create_node("unit-tests").await?;
    let e2e_tests = engine.store.find_or_create_node("e2e-tests").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(storage.id, "has_tests", unit_tests.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "has_tests", unit_tests.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(stigmergy_mod.id, "has_tests", integration_tests.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_tests", e2e_tests.id)).await?);
    
    // === Issues and work items ===
    
    let issue_123 = engine.store.find_or_create_node("issue-123").await?;
    let issue_456 = engine.store.find_or_create_node("issue-456").await?;
    let pr_789 = engine.store.find_or_create_node("pr-789").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(issue_123.id, "affects", embeddings.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(issue_123.id, "title", 
        engine.store.find_or_create_node("spectral embeddings fail on disconnected graphs").await?.id)).await?);
    
    triple_ids.push(engine.store.insert_triple(Triple::new(issue_456.id, "affects", lifecycle.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(issue_456.id, "title",
        engine.store.find_or_create_node("decay should consider source reliability").await?.id)).await?);
    
    triple_ids.push(engine.store.insert_triple(Triple::new(pr_789.id, "fixes", issue_123.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(pr_789.id, "modifies", embeddings.id)).await?);
    
    // === Recent changes ===
    
    let alice = engine.store.find_or_create_node("Alice").await?;
    let bob = engine.store.find_or_create_node("Bob").await?;
    let carol = engine.store.find_or_create_node("Carol").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(alice.id, "authored", pr_789.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(bob.id, "reviewed", pr_789.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(carol.id, "maintains", embeddings.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(carol.id, "maintains", graph.id)).await?);
    
    // === Documentation ===
    
    let arch_doc = engine.store.find_or_create_node("architecture-doc").await?;
    let api_doc = engine.store.find_or_create_node("api-documentation").await?;
    let readme = engine.store.find_or_create_node("README").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_doc", readme.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(valence.id, "has_doc", arch_doc.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(api.id, "has_doc", api_doc.id)).await?);
    
    triple_ids.push(engine.store.insert_triple(Triple::new(arch_doc.id, "describes", stigmergy_concept.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(arch_doc.id, "describes", decay_concept.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(arch_doc.id, "describes", embedding_concept.id)).await?);
    
    // === External dependencies ===
    
    let petgraph = engine.store.find_or_create_node("petgraph").await?;
    let nalgebra = engine.store.find_or_create_node("nalgebra").await?;
    let tokio = engine.store.find_or_create_node("tokio").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(graph.id, "uses_library", petgraph.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "uses_library", nalgebra.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(api.id, "uses_library", tokio.id)).await?);
    
    // === Performance characteristics ===
    
    let fast = engine.store.find_or_create_node("fast").await?;
    let memory_intensive = engine.store.find_or_create_node("memory-intensive").await?;
    let scalable = engine.store.find_or_create_node("scalable").await?;
    
    triple_ids.push(engine.store.insert_triple(Triple::new(storage.id, "characteristic", fast.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(embeddings.id, "characteristic", memory_intensive.id)).await?);
    triple_ids.push(engine.store.insert_triple(Triple::new(stigmergy_mod.id, "characteristic", scalable.id)).await?);
    
    Ok(triple_ids)
}

#[tokio::test]
async fn test_full_e2e_pipeline() -> Result<()> {
    println!("\n=== Starting E2E Pipeline Test ===\n");
    
    // === PHASE 1: Setup and Insert ===
    println!("Phase 1: Building knowledge graph...");
    
    let engine = ValenceEngine::new();
    let triple_ids = build_project_knowledge_graph(&engine).await?;
    
    println!("  ✓ Inserted {} triples", triple_ids.len());
    assert!(triple_ids.len() >= 50, "Should have at least 50 triples");
    
    let initial_count = engine.store.count_triples().await?;
    println!("  ✓ Total triples in store: {}", initial_count);
    
    let node_count = engine.store.count_nodes().await?;
    println!("  ✓ Total nodes in store: {}", node_count);
    assert!(node_count > 30, "Should have substantial node count");
    
    // === PHASE 2: Compute Embeddings ===
    println!("\nPhase 2: Computing topology-derived embeddings...");
    
    // Compute spectral embeddings
    let spectral_count = engine.recompute_embeddings(16).await?;
    println!("  ✓ Computed spectral embeddings for {} nodes", spectral_count);
    assert_eq!(spectral_count, node_count as usize, "Should have embedding for each node");
    
    // Also test Node2Vec embeddings
    let node2vec_config = Node2VecConfig {
        dimensions: 16,
        walk_length: 10,
        walks_per_node: 5,
        epochs: 3,
        ..Default::default()
    };
    let node2vec_count = engine.recompute_node2vec(node2vec_config).await?;
    println!("  ✓ Computed Node2Vec embeddings for {} nodes", node2vec_count);
    assert_eq!(node2vec_count, node_count as usize);
    
    // === PHASE 3: Search for Relevant Context ===
    println!("\nPhase 3: Searching for relevant context...");
    
    // Query about embeddings functionality
    let embeddings_node = engine.store.find_node_by_value("embeddings-module").await?.expect("Should find embeddings node");
    
    // Get semantic neighbors
    let embeddings_lock = engine.embeddings.read().await;
    let embedding_vector = embeddings_lock.get(embeddings_node.id)
        .expect("Should have embedding for embeddings node");
    let neighbors = embeddings_lock.query_nearest(embedding_vector, 10)?;
    drop(embeddings_lock);
    
    println!("  ✓ Found {} semantic neighbors for embeddings-module", neighbors.len());
    assert!(!neighbors.is_empty(), "Should find neighbors");
    
    // Get structural neighbors (graph traversal)
    let neighborhood = engine.store.neighbors(embeddings_node.id, 2).await?;
    println!("  ✓ Found {} triples in 2-hop neighborhood", neighborhood.len());
    assert!(neighborhood.len() >= 5, "Should have multiple connected triples");
    
    // === PHASE 4: Assemble Context with Budget ===
    println!("\nPhase 4: Assembling context with budget constraints...");
    
    let assembler = ContextAssembler::new(&engine);
    let config = AssemblyConfig {
        max_triples: 20,
        max_nodes: 30,
        include_confidence: true,
        include_sources: false,
        format: ContextFormat::Markdown,
        fusion_config: None,
    };
    
    // Assemble context (query must be an existing node value)
    let query_text = "embeddings-module";
    let context = assembler.assemble(query_text, config).await?;
    
    println!("  ✓ Assembled context with {} triples", context.triples.len());
    println!("  ✓ Context includes {} nodes", context.nodes.len());
    println!("  ✓ Total relevance score: {:.3}", context.total_score);
    assert!(context.triples.len() <= 20, "Should respect max_triples budget");
    assert!(context.nodes.len() <= 30, "Should respect max_nodes budget");
    assert!(!context.formatted.is_empty(), "Should have formatted output");
    
    // === PHASE 5: Record Access Feedback ===
    println!("\nPhase 5: Recording access feedback for stigmergy...");
    
    // Simulate multiple queries that access related triples together
    let storage_node = engine.store.find_node_by_value("storage-module").await?.expect("Should find storage");
    let graph_node = engine.store.find_node_by_value("graph-module").await?.expect("Should find graph");
    let lifecycle_node = engine.store.find_node_by_value("lifecycle-module").await?.expect("Should find lifecycle");
    
    // Find triples involving these nodes
    let storage_triples = engine.store.query_triples(TriplePattern {
        subject: Some(storage_node.id),
        ..Default::default()
    }).await?;
    let graph_triples = engine.store.query_triples(TriplePattern {
        subject: Some(graph_node.id),
        ..Default::default()
    }).await?;
    let lifecycle_triples = engine.store.query_triples(TriplePattern {
        subject: Some(lifecycle_node.id),
        ..Default::default()
    }).await?;
    
    println!("  ✓ Found {} triples for storage-module", storage_triples.len());
    println!("  ✓ Found {} triples for graph-module", graph_triples.len());
    println!("  ✓ Found {} triples for lifecycle-module", lifecycle_triples.len());
    
    // Simulate co-access patterns (these modules are frequently queried together)
    let mut co_accessed_ids: Vec<uuid::Uuid> = Vec::new();
    co_accessed_ids.extend(storage_triples.iter().take(3).map(|t| t.id));
    co_accessed_ids.extend(graph_triples.iter().take(3).map(|t| t.id));
    
    // Record multiple co-access events (need to exceed threshold of 3)
    for i in 0..5 {
        engine.access_tracker
            .record_access(&co_accessed_ids, &format!("query_about_graph_storage_{}", i))
            .await;
    }
    println!("  ✓ Recorded 5 co-access events for storage-graph relationship");
    
    // Another co-access pattern: lifecycle and graph
    let mut co_accessed_ids_2: Vec<uuid::Uuid> = Vec::new();
    co_accessed_ids_2.extend(lifecycle_triples.iter().take(3).map(|t| t.id));
    co_accessed_ids_2.extend(graph_triples.iter().take(2).map(|t| t.id));
    
    for i in 0..4 {
        engine.access_tracker
            .record_access(&co_accessed_ids_2, &format!("query_about_lifecycle_{}", i))
            .await;
    }
    println!("  ✓ Recorded 4 co-access events for lifecycle-graph relationship");
    
    // === PHASE 6: Run Stigmergy Reinforcement ===
    println!("\nPhase 6: Running stigmergy reinforcement...");
    
    let triples_before_stigmergy = engine.store.count_triples().await?;
    println!("  • Triples before reinforcement: {}", triples_before_stigmergy);
    
    let edges_created = engine.run_stigmergy_reinforcement().await?;
    println!("  ✓ Created {} new co-retrieval edges", edges_created);
    
    let triples_after_stigmergy = engine.store.count_triples().await?;
    println!("  • Triples after reinforcement: {}", triples_after_stigmergy);
    
    // Verify that edges were created
    assert!(edges_created > 0, "Should have created co-retrieval edges");
    assert_eq!(
        triples_after_stigmergy,
        triples_before_stigmergy + edges_created,
        "Triple count should increase by number of edges created"
    );
    
    // === PHASE 7: Verify Graph Topology Evolved ===
    println!("\nPhase 7: Verifying graph topology evolution...");
    
    // Check that co-accessed nodes now have structural connections
    let storage_neighborhood_after = engine.store.neighbors(storage_node.id, 1).await?;
    let graph_neighborhood_after = engine.store.neighbors(graph_node.id, 1).await?;
    
    println!("  ✓ Storage node now has {} 1-hop triples", storage_neighborhood_after.len());
    println!("  ✓ Graph node now has {} 1-hop triples", graph_neighborhood_after.len());
    
    // The co-retrieval edges should mean these nodes are now closer in the graph
    // (They should have co-retrieval predicates connecting their associated triples)
    let all_triples = engine.store.query_triples(TriplePattern::default()).await?;
    let co_retrieval_edges: Vec<_> = all_triples.iter()
        .filter(|t| t.predicate.value.contains("co_retrieved"))
        .collect();
    
    println!("  ✓ Found {} co-retrieval edges in graph", co_retrieval_edges.len());
    assert!(!co_retrieval_edges.is_empty(), "Should have co-retrieval edges");
    
    // === PHASE 8: Run Lifecycle Decay ===
    println!("\nPhase 8: Running lifecycle decay...");
    
    // Get weights before decay
    let triples_before_decay = engine.store.query_triples(TriplePattern::default()).await?;
    let avg_weight_before: f64 = triples_before_decay.iter().map(|t| t.weight).sum::<f64>() 
        / triples_before_decay.len() as f64;
    println!("  • Average weight before decay: {:.3}", avg_weight_before);
    
    // Apply decay (50% factor)
    let decay_factor = 0.5;
    let decayed_count = engine.store.decay(decay_factor, 0.0).await?;
    println!("  ✓ Decayed {} triples by factor {}", decayed_count, decay_factor);
    
    // Get weights after decay
    let triples_after_decay = engine.store.query_triples(TriplePattern::default()).await?;
    let avg_weight_after: f64 = triples_after_decay.iter().map(|t| t.weight).sum::<f64>() 
        / triples_after_decay.len() as f64;
    println!("  • Average weight after decay: {:.3}", avg_weight_after);
    
    // Verify weights decreased
    assert!(avg_weight_after < avg_weight_before, "Average weight should decrease after decay");
    assert!((avg_weight_after - avg_weight_before * decay_factor).abs() < 0.01, 
            "Weights should be approximately scaled by decay factor");
    
    // === PHASE 9: Verify Low-Value Triples Lost Weight ===
    println!("\nPhase 9: Verifying low-value triple eviction...");
    
    // Count triples below various thresholds
    let below_0_3: usize = triples_after_decay.iter().filter(|t| t.weight < 0.3).count();
    let below_0_5: usize = triples_after_decay.iter().filter(|t| t.weight < 0.5).count();
    let below_0_7: usize = triples_after_decay.iter().filter(|t| t.weight < 0.7).count();
    
    println!("  • {} triples below 0.3 weight", below_0_3);
    println!("  • {} triples below 0.5 weight", below_0_5);
    println!("  • {} triples below 0.7 weight", below_0_7);
    
    // Evict triples below 0.3 threshold
    let evicted_count = engine.store.evict_below_weight(0.3).await?;
    println!("  ✓ Evicted {} low-weight triples (threshold 0.3)", evicted_count);
    
    assert_eq!(evicted_count, below_0_3 as u64, "Should evict all triples below threshold");
    
    let triples_after_eviction = engine.store.count_triples().await?;
    println!("  • Triples remaining after eviction: {}", triples_after_eviction);
    
    assert_eq!(
        triples_after_eviction,
        triples_after_decay.len() as u64 - evicted_count,
        "Triple count should decrease by eviction count"
    );
    
    // === PHASE 10: Final Verification ===
    println!("\nPhase 10: Final system verification...");
    
    // Verify we can still compute embeddings after topology changes
    let final_embedding_count = engine.recompute_embeddings(8).await?;
    println!("  ✓ Recomputed embeddings after topology changes ({} nodes)", final_embedding_count);
    assert!(final_embedding_count > 0, "Should still have embeddings");
    
    // Verify we can still query the graph
    let final_triples = engine.store.query_triples(TriplePattern::default()).await?;
    println!("  ✓ Graph is still queryable ({} triples remain)", final_triples.len());
    
    // Verify all remaining triples have acceptable weights
    let min_weight = final_triples.iter().map(|t| t.weight).fold(f64::INFINITY, f64::min);
    let max_weight = final_triples.iter().map(|t| t.weight).fold(f64::NEG_INFINITY, f64::max);
    println!("  ✓ Weight range: [{:.3}, {:.3}]", min_weight, max_weight);
    assert!(min_weight >= 0.3, "All remaining triples should be above eviction threshold");
    
    // Verify access tracking is still operational
    let test_triple_id = final_triples[0].id;
    engine.access_tracker.record_access(&[test_triple_id], "final_test_query").await;
    println!("  ✓ Access tracking still operational");
    
    // Run full stigmergy maintenance cycle to verify everything integrates
    let (edges_created_final, events_decayed) = engine.run_stigmergy_maintenance().await?;
    println!("  ✓ Full stigmergy maintenance cycle completed");
    println!("    • Edges created: {}", edges_created_final);
    println!("    • Events decayed: {}", events_decayed);
    
    println!("\n=== E2E Pipeline Test Complete ===");
    println!("✓ All phases passed without panics");
    println!("✓ Graph evolved: {} → {} triples", initial_count, engine.store.count_triples().await?);
    println!("✓ Stigmergy created {} new structural edges", edges_created + edges_created_final);
    println!("✓ Lifecycle management evicted {} low-value triples", evicted_count);
    println!("✓ System remains operational after full pipeline\n");
    
    Ok(())
}

/// Test that the pipeline handles edge cases gracefully
#[tokio::test]
async fn test_pipeline_edge_cases() -> Result<()> {
    println!("\n=== Testing Pipeline Edge Cases ===\n");
    
    let engine = ValenceEngine::new();
    
    // Empty graph embeddings
    println!("Testing embeddings on empty graph...");
    let result = engine.recompute_embeddings(8).await;
    assert!(result.is_ok(), "Should handle empty graph gracefully");
    println!("  ✓ Empty graph handled");
    
    // Single node (spectral embeddings require at least 2 nodes)
    println!("Testing with single isolated node...");
    let _node = engine.store.find_or_create_node("isolated").await?;
    let count = engine.recompute_embeddings(8).await?;
    // Spectral embeddings don't work well with single nodes, so we expect 0
    assert_eq!(count, 0, "Single isolated node cannot have meaningful spectral embedding");
    println!("  ✓ Single node handled (no embedding computed, as expected)");
    
    // Stigmergy with no access events
    println!("Testing stigmergy with no access history...");
    let edges = engine.run_stigmergy_reinforcement().await?;
    assert_eq!(edges, 0, "Should create no edges without access history");
    println!("  ✓ No access history handled");
    
    // Decay on empty graph
    println!("Testing decay on graph with no triples...");
    let decayed = engine.store.decay(0.5, 0.0).await?;
    assert_eq!(decayed, 0, "Should decay 0 triples in empty graph");
    println!("  ✓ Empty decay handled");
    
    // Eviction on empty graph
    println!("Testing eviction on empty graph...");
    let evicted = engine.store.evict_below_weight(0.5).await?;
    assert_eq!(evicted, 0, "Should evict 0 triples from empty graph");
    println!("  ✓ Empty eviction handled");
    
    println!("\n=== Edge Cases Test Complete ===\n");
    Ok(())
}

/// Test that the pipeline is resilient to concurrent operations
#[tokio::test]
async fn test_pipeline_concurrency() -> Result<()> {
    println!("\n=== Testing Pipeline Concurrency ===\n");
    
    let engine = ValenceEngine::new();
    
    // Build initial graph
    let _ = build_project_knowledge_graph(&engine).await?;
    println!("  ✓ Built initial graph");
    
    // Spawn concurrent operations
    let engine1 = engine.clone();
    let engine2 = engine.clone();
    let engine3 = engine.clone();
    
    let handle1 = tokio::spawn(async move {
        engine1.recompute_embeddings(8).await
    });
    
    let handle2 = tokio::spawn(async move {
        engine2.run_stigmergy_reinforcement().await
    });
    
    let handle3 = tokio::spawn(async move {
        engine3.store.decay(0.9, 0.0).await
    });
    
    // Wait for all to complete
    let result1 = handle1.await??;
    let result2 = handle2.await??;
    let result3 = handle3.await??;
    
    println!("  ✓ Concurrent operations completed:");
    println!("    • Embeddings: {} nodes", result1);
    println!("    • Stigmergy edges: {}", result2);
    println!("    • Decayed triples: {}", result3);
    
    println!("\n=== Concurrency Test Complete ===\n");
    Ok(())
}
