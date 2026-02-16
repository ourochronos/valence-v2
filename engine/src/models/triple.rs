use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type TripleId = Uuid;
pub type NodeId = Uuid;

/// A node in the knowledge graph — either a subject or object of a triple.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Node {
    pub id: NodeId,
    pub value: String,
    pub node_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
}

impl Node {
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Predicate {
    pub value: String,
}

impl Predicate {
    pub fn new(value: impl Into<String>) -> Self {
        Self { value: value.into() }
    }
}

/// The atomic unit of knowledge: subject → predicate → object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Triple {
    pub id: TripleId,
    pub subject: NodeId,
    pub predicate: Predicate,
    pub object: NodeId,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    /// Decay weight — decreases over time without access, refreshes on access
    pub weight: f64,
}

impl Triple {
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

    /// Refresh this triple's weight on access
    pub fn touch(&mut self) {
        self.last_accessed = Utc::now();
        self.access_count += 1;
        self.weight = 1.0; // Reset decay
    }
}
