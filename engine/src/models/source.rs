use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::triple::TripleId;

pub type SourceId = Uuid;

/// How a piece of knowledge was derived.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceType {
    /// Direct statement from a conversation
    Conversation,
    /// Observed from system behavior
    Observation,
    /// Inferred from other knowledge
    Inference,
    /// Explicitly provided by user
    UserInput,
    /// Extracted from a document
    Document,
    /// Decomposed from natural language by boundary model
    Decomposition,
}

/// Provenance record linking a source to the triples it supports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    /// The triples this source supports
    pub triple_ids: Vec<TripleId>,
    /// How this knowledge was derived
    pub source_type: SourceType,
    /// Reference to the original source (session ID, document URL, etc.)
    pub reference: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

impl Source {
    pub fn new(triple_ids: Vec<TripleId>, source_type: SourceType) -> Self {
        Self {
            id: Uuid::new_v4(),
            triple_ids,
            source_type,
            reference: None,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }
}
