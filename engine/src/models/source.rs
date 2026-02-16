use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::triple::TripleId;

/// Unique identifier for a source.
pub type SourceId = Uuid;

/// How a piece of knowledge was derived.
///
/// Tracks the provenance of triples to support confidence scoring and explanation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
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
///
/// Sources provide context about where knowledge came from, enabling confidence scoring,
/// explanation, and trust propagation through the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Unique identifier for this source
    pub id: SourceId,
    /// The triples this source supports
    pub triple_ids: Vec<TripleId>,
    /// How this knowledge was derived
    pub source_type: SourceType,
    /// Reference to the original source (session ID, document URL, etc.)
    pub reference: Option<String>,
    /// When this source was created
    pub created_at: DateTime<Utc>,
    /// Optional metadata (arbitrary JSON)
    pub metadata: Option<serde_json::Value>,
}

impl Source {
    /// Create a new source for the given triples.
    ///
    /// The source is assigned a random UUID and timestamp is set to now.
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

    /// Add a reference string to this source (builder pattern).
    ///
    /// The reference typically contains a session ID, document URL, or other identifier
    /// pointing to the original context.
    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }
}
