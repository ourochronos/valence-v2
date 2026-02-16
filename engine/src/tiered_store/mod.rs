//! Tiered storage: Hot (in-memory) + Cold (persistent) with automatic promotion/demotion.
//!
//! This module implements a cold/warm split where:
//! - **Hot tier (MemoryStore)**: Frequently accessed triples stay in memory for fast access
//! - **Cold tier (PgStore)**: Infrequently accessed triples live in PostgreSQL to save memory
//!
//! Access patterns from the stigmergy module drive automatic promotion (cold → hot)
//! and demotion (hot → cold). This enables:
//! - Memory-bounded operation (hot tier has a capacity limit)
//! - Fast access for frequently used knowledge
//! - Persistent storage for the full corpus
//! - Graceful degradation when cold tier is unavailable

mod config;
mod store;

pub use config::{TieredConfig, PromotionPolicy, DemotionPolicy};
pub use store::{TieredStore, Tier};
