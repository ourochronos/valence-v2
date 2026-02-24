//! Content-addressable ID generation using blake3.
//!
//! Same content always produces the same ID, on any node.
//! This is the foundation for trivial CRDT federation merge.
//!
//! IDs are blake3 hashes truncated to 16 bytes, packed into [`Uuid`].
//! 128 bits from blake3 provides more than enough collision resistance
//! at our scale, and every existing API that accepts/returns UUID strings
//! continues to work unchanged.

use uuid::Uuid;

use crate::models::NodeId;

/// Compute a content-addressable UUID from raw bytes.
///
/// Hashes the input with blake3, truncates to 16 bytes, and packs into a Uuid.
fn content_id(data: &[u8]) -> Uuid {
    let hash = blake3::hash(data);
    let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
    Uuid::from_bytes(bytes)
}

/// Compute a deterministic NodeId from a node's value string.
///
/// Same value always produces the same NodeId, on any node.
pub fn node_id(value: &str) -> Uuid {
    content_id(value.as_bytes())
}

/// Compute a deterministic TripleId from (subject, predicate, object).
///
/// Same (S, P, O) always produces the same TripleId, on any node.
/// Uses the raw bytes of subject and object UUIDs plus the predicate string.
pub fn triple_id(subject: NodeId, predicate: &str, object: NodeId) -> Uuid {
    let mut input = Vec::with_capacity(16 + predicate.len() + 16);
    input.extend_from_slice(subject.as_bytes());
    input.extend_from_slice(predicate.as_bytes());
    input.extend_from_slice(object.as_bytes());
    content_id(&input)
}

/// Compute a deterministic UUID for a predicate string.
///
/// Useful for index key generation and consistency.
pub fn predicate_id(predicate: &str) -> Uuid {
    // Prefix with "predicate:" to avoid collisions with node_id for the same string
    let mut input = Vec::with_capacity(10 + predicate.len());
    input.extend_from_slice(b"predicate:");
    input.extend_from_slice(predicate.as_bytes());
    content_id(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_deterministic() {
        let id1 = node_id("hello");
        let id2 = node_id("hello");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_node_id_different_values() {
        let id1 = node_id("hello");
        let id2 = node_id("world");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_triple_id_deterministic() {
        let s = node_id("Alice");
        let o = node_id("Bob");
        let id1 = triple_id(s, "knows", o);
        let id2 = triple_id(s, "knows", o);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_triple_id_different_components() {
        let alice = node_id("Alice");
        let bob = node_id("Bob");
        let carol = node_id("Carol");

        let id1 = triple_id(alice, "knows", bob);
        let id2 = triple_id(alice, "knows", carol);
        let id3 = triple_id(alice, "likes", bob);
        let id4 = triple_id(bob, "knows", alice);

        // All should be different
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id1, id4);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_predicate_id_deterministic() {
        let id1 = predicate_id("knows");
        let id2 = predicate_id("knows");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_predicate_id_no_collision_with_node_id() {
        // A predicate "hello" should not produce the same ID as a node "hello"
        let nid = node_id("hello");
        let pid = predicate_id("hello");
        assert_ne!(nid, pid);
    }

    #[test]
    fn test_content_id_is_valid_uuid() {
        let id = node_id("test");
        // Should be a valid UUID string
        let s = id.to_string();
        assert_eq!(s.len(), 36); // UUID format: 8-4-4-4-12
        assert!(Uuid::parse_str(&s).is_ok());
    }

    #[test]
    fn test_empty_string_node_id() {
        let id1 = node_id("");
        let id2 = node_id("");
        assert_eq!(id1, id2);
        // Empty string still produces a valid, non-nil UUID
        assert_ne!(id1, Uuid::nil());
    }

    #[test]
    fn test_similar_strings_produce_different_ids() {
        // Ensure near-miss strings don't collide
        let id1 = node_id("test1");
        let id2 = node_id("test2");
        let id3 = node_id("test10");
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id2, id3);
    }
}
