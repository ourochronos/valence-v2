use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::triple::TripleId;

/// Unique identifier for a source.
pub type SourceId = Uuid;

/// Maximum supersession chain depth to walk before declaring a cycle.
pub const MAX_CHAIN_DEPTH: usize = 64;

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
///
/// Supersession chains are encoded via `superseded_by`: if this source has been replaced
/// by a newer source, `superseded_by` points to that newer source.  Walking the chain
/// forward (following `superseded_by` links) reaches the authoritative *head*.
///
/// Example: A supersedes B supersedes C
///   C.superseded_by = Some(B.id)
///   B.superseded_by = Some(A.id)
///   A.superseded_by = None   <- head / authoritative
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
    /// If this source has been superseded, points to the newer authoritative source.
    ///
    /// `None` means this source is authoritative (head of its chain).
    /// `Some(id)` means this source is historical; follow `id` to reach the head.
    #[serde(default)]
    pub superseded_by: Option<SourceId>,
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
            superseded_by: None,
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

    /// Mark this source as superseded by `newer_id` (builder pattern).
    ///
    /// After calling this, the source is considered historical; `newer_id` is authoritative.
    pub fn mark_superseded_by(mut self, newer_id: SourceId) -> Self {
        self.superseded_by = Some(newer_id);
        self
    }

    /// Returns `true` if this source has been superseded and is no longer authoritative.
    pub fn is_superseded(&self) -> bool {
        self.superseded_by.is_some()
    }
}
