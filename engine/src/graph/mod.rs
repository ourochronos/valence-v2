//! In-memory graph algorithms using petgraph.
//!
//! This module sits on top of the TripleStore and provides:
//! - GraphView: builds petgraph DiGraph from TripleStore data
//! - Graph algorithms: PageRank, connected components, shortest paths, centrality
//! - DynamicConfidence: computes confidence from topology at query time

pub mod view;
pub mod algorithms;
pub mod confidence;

pub use view::GraphView;
pub use algorithms::*;
pub use confidence::{DynamicConfidence, ConfidenceScore};
