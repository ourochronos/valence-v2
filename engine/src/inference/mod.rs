//! Inference Training Loop: Query patterns and results feed back to improve retrieval.
//!
//! This module implements the third self-closing loop from the design docs:
//! when the LLM uses context assembled from the graph, we observe which triples
//! were actually useful (cited, led to good answers) vs ignored. This feedback
//! strengthens useful paths and weakens noise.
//!
//! The inference loop provides:
//! - Feedback recording: track which triples were used/ignored in a context window
//! - Weight adjustment: strengthen used paths, decay ignored ones
//! - Integration with stigmergy: update access patterns based on actual usage
//! - API endpoint: submit feedback after LLM processes assembled context
//!
//! ## How It Works
//!
//! 1. Context Assembly: The system retrieves triples for an LLM query
//! 2. LLM Processing: The LLM uses some triples, ignores others
//! 3. Feedback Submission: Agent reports which triples were actually useful
//! 4. Substrate Update: Used triples get weight boost + confidence refresh, unused decay
//!
//! This makes the graph learn from its own usage — no separate training phase needed.

pub mod feedback;
pub mod weight_adjuster;

pub use feedback::{UsageFeedback, FeedbackSignal, FeedbackRecorder, FeedbackRecorderConfig};
pub use weight_adjuster::{WeightAdjuster, WeightAdjusterConfig, AdjustmentStrategy};
