use valence_engine::models::*;
use valence_engine::models::source::SourceType;

#[test]
fn test_create_triple() {
    let subject = Node::new("Valence");
    let object = Node::new("knowledge substrate");
    let triple = Triple::new(subject.id, "is_a", object.id);

    assert_eq!(triple.predicate.value, "is_a");
    assert_eq!(triple.subject, subject.id);
    assert_eq!(triple.object, object.id);
    assert_eq!(triple.base_weight, 1.0);
    assert_eq!(triple.local_weight, 1.0);
    assert_eq!(triple.access_count, 0);
}

#[test]
fn test_touch_refreshes_weight() {
    let s = Node::new("A");
    let o = Node::new("B");
    let mut triple = Triple::new(s.id, "knows", o.id);
    triple.local_weight = 0.5; // Simulate decay
    triple.touch();
    assert_eq!(triple.local_weight, 1.0);
    assert_eq!(triple.access_count, 1);
}

#[test]
fn test_source_creation() {
    let s = Node::new("X");
    let o = Node::new("Y");
    let triple = Triple::new(s.id, "relates_to", o.id);
    
    let source = Source::new(vec![triple.id], SourceType::Conversation)
        .with_reference("session:abc123");
    
    assert_eq!(source.triple_ids.len(), 1);
    assert_eq!(source.source_type, SourceType::Conversation);
    assert_eq!(source.reference.unwrap(), "session:abc123");
}
