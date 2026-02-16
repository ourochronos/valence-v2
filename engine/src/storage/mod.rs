///! Storage backends for triples and nodes.
///!
///! This module provides the [`TripleStore`] trait and multiple implementations:
///! - [`MemoryStore`]: Fast in-memory storage (ephemeral)
///! - [`PgStore`]: PostgreSQL-backed persistent storage (requires `postgres` feature)

pub mod traits;
pub mod memory;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use traits::{TripleStore, TriplePattern};
pub use memory::MemoryStore;

#[cfg(feature = "postgres")]
pub use postgres::PgStore;
