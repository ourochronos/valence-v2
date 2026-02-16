//! Budget-bounded operations for efficient, responsive retrieval.
//!
//! This module implements progressive refinement through tiered retrieval,
//! where operations run until a budget (time, hops, results) is exhausted.
//! Good-enough beats perfect when the read boundary is fuzzy anyway.

pub mod bounded;
pub mod tiered;

pub use bounded::OperationBudget;
pub use tiered::{TieredRetriever, RetrievalResult};
