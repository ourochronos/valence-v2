//! Storage backends for triples and nodes.
//!
//! This module provides the [`TripleStore`] trait and multiple implementations:
//! - [`MemoryStore`]: Fast in-memory storage (ephemeral)
//! - [`PgStore`]: PostgreSQL-backed persistent storage (requires `postgres` feature)
//! - [`SledStore`]: Embedded persistent storage via sled (requires `embedded` feature)

pub mod traits;
pub mod memory;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "embedded")]
pub mod sled_store;

pub use traits::{TripleStore, TriplePattern};
pub use memory::MemoryStore;

#[cfg(feature = "postgres")]
pub use postgres::PgStore;

#[cfg(feature = "embedded")]
pub use sled_store::SledStore;
