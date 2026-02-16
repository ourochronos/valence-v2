//! Integration test for stigmergy: access patterns reshaping the graph.
//!
//! This test verifies the complete stigmergy loop:
//! 1. Query triples together (co-access)
//! 2. Access tracker records the patterns
//! 3. Reinforcement creates structural edges
//! 4. Graph topology now reflects usage patterns

use valence_engine::{
    ValenceEngine,
    Triple,
};

#[tokio::test]
async fn test_stigmergy_full_cycle() {
    // Create engine
    let engine = ValenceEngine::new();

    // Create a knowledge graph
    let alice = engine.store.find_or_create_node("Alice").await.unwrap();
    let bob = engine.store.find_or_create_node("Bob").await.unwrap();
    let charlie = engine.store.find_or_create_node("Charlie").await.unwrap();
    let diana = engine.store.find_or_create_node("Diana").await.unwrap();
    let eve = engine.store.find_or_create_node("Eve").await.unwrap();
    let frank = engine.store.find_or_create_node("Frank").await.unwrap();

    // Insert triples
    let t1 = Triple::new(alice.id, "knows", bob.id);
    let t2 = Triple::new(charlie.id, "knows", diana.id);
    let t3 = Triple::new(eve.id, "knows", frank.id);

    let id1 = engine.store.insert_triple(t1).await.unwrap();
    let id2 = engine.store.insert_triple(t2).await.unwrap();
    let id3 = engine.store.insert_triple(t3).await.unwrap();

    // Initially: 3 triples, 6 nodes
    assert_eq!(engine.store.count_triples().await.unwrap(), 3);
    assert_eq!(engine.store.count_nodes().await.unwrap(), 6);

    // Simulate usage pattern: t1 and t2 are frequently accessed together
    for i in 0..10 {
        engine.access_tracker
            .record_access(&[id1, id2], &format!("query_{}", i))
            .await;
    }

    // t3 is accessed alone (no co-access pattern)
    for i in 0..5 {
        engine.access_tracker
            .record_access(&[id3], &format!("solo_query_{}", i))
            .await;
    }

    // Verify access tracking
    let co_access_count = engine.access_tracker.get_co_access_count(id1, id2).await;
    assert_eq!(co_access_count, 10);

    let co_access_count_t3 = engine.access_tracker.get_co_access_count(id1, id3).await;
    assert_eq!(co_access_count_t3, 0);

    // Run stigmergy reinforcement (threshold is 3 by default)
    let edges_created = engine.run_stigmergy_reinforcement().await.unwrap();

    // Should create 2 co-retrieval edges:
    // - Alice <-> Charlie (subjects of frequently co-accessed triples)
    // - Bob <-> Diana (objects of frequently co-accessed triples)
    assert_eq!(edges_created, 2);

    // Now we have 5 triples total (original 3 + 2 co-retrieval edges)
    assert_eq!(engine.store.count_triples().await.unwrap(), 5);

    // Verify the co-retrieval edges exist
    use valence_engine::storage::TriplePattern;

    let co_retrieval_edges = engine.store.query_triples(TriplePattern {
        subject: None,
        predicate: Some("co_retrieved_with".to_string()),
        object: None,
    }).await.unwrap();

    assert_eq!(co_retrieval_edges.len(), 2);

    // The graph structure now reflects the usage pattern:
    // Frequently co-accessed triples have become structurally closer
    println!("✓ Stigmergy: usage patterns have reshaped the graph structure");
}

#[tokio::test]
async fn test_stigmergy_maintenance_cycle() {
    let engine = ValenceEngine::new();

    // Create simple graph
    let a = engine.store.find_or_create_node("A").await.unwrap();
    let b = engine.store.find_or_create_node("B").await.unwrap();
    let c = engine.store.find_or_create_node("C").await.unwrap();
    let d = engine.store.find_or_create_node("D").await.unwrap();

    let t1 = Triple::new(a.id, "rel", b.id);
    let t2 = Triple::new(c.id, "rel", d.id);

    let id1 = engine.store.insert_triple(t1).await.unwrap();
    let id2 = engine.store.insert_triple(t2).await.unwrap();

    // Record co-accesses
    for _ in 0..5 {
        engine.access_tracker
            .record_access(&[id1, id2], "query")
            .await;
    }

    // Run full maintenance cycle
    let (created, decayed) = engine.run_stigmergy_maintenance().await.unwrap();

    // Should create edges
    assert_eq!(created, 2);

    // With default config (24h decay window), no events should decay
    assert_eq!(decayed, 0);

    // Verify edges were created
    assert_eq!(engine.store.count_triples().await.unwrap(), 4);
}

#[tokio::test]
async fn test_no_reinforcement_below_threshold() {
    let engine = ValenceEngine::new();

    // Create triples
    let a = engine.store.find_or_create_node("A").await.unwrap();
    let b = engine.store.find_or_create_node("B").await.unwrap();
    let c = engine.store.find_or_create_node("C").await.unwrap();
    let d = engine.store.find_or_create_node("D").await.unwrap();

    let t1 = Triple::new(a.id, "rel", b.id);
    let t2 = Triple::new(c.id, "rel", d.id);

    let id1 = engine.store.insert_triple(t1).await.unwrap();
    let id2 = engine.store.insert_triple(t2).await.unwrap();

    // Record only 2 co-accesses (below threshold of 3)
    engine.access_tracker.record_access(&[id1, id2], "query1").await;
    engine.access_tracker.record_access(&[id1, id2], "query2").await;

    // Run reinforcement
    let created = engine.run_stigmergy_reinforcement().await.unwrap();

    // Should NOT create edges (below threshold)
    assert_eq!(created, 0);

    // Still only original 2 triples
    assert_eq!(engine.store.count_triples().await.unwrap(), 2);
}
