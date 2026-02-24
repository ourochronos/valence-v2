use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::content_id;

/// Unique identifier for a triple.
pub type TripleId = Uuid;

/// Unique identifier for a node.
pub type NodeId = Uuid;

/// A node in the knowledge graph — either a subject or object of a triple.
///
/// Nodes represent entities or concepts in the knowledge base. Each node has a string value,
/// optional type information, and access tracking metadata for decay/eviction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Node {
    /// Unique identifier for this node
    pub id: NodeId,
    /// The string value/label of the node
    pub value: String,
    /// Optional type annotation (e.g., "person", "concept")
    pub node_type: Option<String>,
    /// When this node was first created
    pub created_at: DateTime<Utc>,
    /// When this node was last accessed
    pub last_accessed: DateTime<Utc>,
    /// How many times this node has been accessed
    pub access_count: u64,
}

impl Node {
    /// Create a new node with the given value.
    ///
    /// The node ID is deterministic: same value always produces the same ID (content-addressed).
    /// Timestamps are set to the current time.
    pub fn new(value: impl Into<String>) -> Self {
        let now = Utc::now();
        let value = value.into();
        let id = content_id::node_id(&value);
        Self {
            id,
            value,
            node_type: None,
            created_at: now,
            last_accessed: now,
            access_count: 0,
        }
    }
}

/// A predicate (relationship type) connecting subject to object.
///
/// Predicates define the type of relationship between two nodes in a triple.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Predicate {
    /// The predicate label (e.g., "knows", "likes", "is_a")
    pub value: String,
}

impl Predicate {
    /// Create a new predicate with the given value.
    pub fn new(value: impl Into<String>) -> Self {
        Self { value: value.into() }
    }
}

/// The atomic unit of knowledge: subject → predicate → object.
///
/// Triples are the fundamental building blocks of the knowledge graph. Each triple
/// represents a single fact or relationship, with provenance, weight split, and
/// decay/eviction tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triple {
    /// Unique identifier for this triple
    pub id: TripleId,
    /// The subject node ID
    pub subject: NodeId,
    /// The relationship type
    pub predicate: Predicate,
    /// The object node ID
    pub object: NodeId,
    // Provenance (first-class, not metadata)
    /// Who authored this triple (DID string)
    pub origin_did: Option<String>,
    /// When this triple was created
    pub timestamp: DateTime<Utc>,
    /// Ed25519 signature of (subject, predicate, object, origin_did, timestamp)
    pub signature: Option<Vec<u8>>,
    // Weight split
    /// Weight from source/network (shareable)
    pub base_weight: f64,
    /// Private, stigmergy-driven weight
    pub local_weight: f64,
    /// When this triple was last accessed
    pub last_accessed: Option<DateTime<Utc>>,
    /// Number of times this triple has been accessed
    pub access_count: u64,
}

impl Triple {
    /// Create a new triple with the given subject, predicate, and object.
    ///
    /// The triple ID is deterministic: same (S, P, O) always produces the same ID (content-addressed).
    /// Timestamp is set to now, and weights are set to 1.0.
    pub fn new(subject: NodeId, predicate: impl Into<String>, object: NodeId) -> Self {
        let now = Utc::now();
        let predicate = Predicate::new(predicate);
        let id = content_id::triple_id(subject, &predicate.value, object);
        Self {
            id,
            subject,
            predicate,
            object,
            origin_did: None,
            timestamp: now,
            signature: None,
            base_weight: 1.0,
            local_weight: 1.0,
            last_accessed: Some(now),
            access_count: 0,
        }
    }

    /// Refresh this triple's local weight on access.
    ///
    /// Updates the last_accessed timestamp, increments access_count, and resets the local_weight to 1.0.
    pub fn touch(&mut self) {
        self.last_accessed = Some(Utc::now());
        self.access_count += 1;
        self.local_weight = 1.0; // Reset decay
    }

    /// Combined effective weight (base + local).
    pub fn effective_weight(&self) -> f64 {
        self.base_weight * self.local_weight
    }
}
