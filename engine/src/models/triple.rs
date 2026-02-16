use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    /// The node is assigned a random UUID, and timestamps are set to the current time.
    pub fn new(value: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            value: value.into(),
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
/// represents a single fact or relationship, with decay/eviction tracking.
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
    /// When this triple was created
    pub created_at: DateTime<Utc>,
    /// When this triple was last accessed
    pub last_accessed: DateTime<Utc>,
    /// Number of times this triple has been accessed
    pub access_count: u64,
    /// Decay weight — decreases over time without access, refreshes on access
    pub weight: f64,
}

impl Triple {
    /// Create a new triple with the given subject, predicate, and object.
    ///
    /// The triple is assigned a random UUID, timestamps are set to now, and weight is set to 1.0.
    pub fn new(subject: NodeId, predicate: impl Into<String>, object: NodeId) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            subject,
            predicate: Predicate::new(predicate),
            object,
            created_at: now,
            last_accessed: now,
            access_count: 0,
            weight: 1.0,
        }
    }

    /// Refresh this triple's weight on access.
    ///
    /// Updates the last_accessed timestamp, increments access_count, and resets the weight to 1.0.
    pub fn touch(&mut self) {
        self.last_accessed = Utc::now();
        self.access_count += 1;
        self.weight = 1.0; // Reset decay
    }
}
