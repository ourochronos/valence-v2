//! Context assembly: the read boundary for LLM agents.
//!
//! This module handles building structured context from the knowledge graph
//! for LLM inference calls. Unlike traditional conversation history, context
//! is freshly assembled for each turn based on relevance to the current query.

pub mod working_set;
pub mod assembler;

pub use working_set::WorkingSet;
pub use assembler::{ContextAssembler, AssemblyConfig, AssembledContext, ContextFormat, ScoredTriple, NodeInfo};
