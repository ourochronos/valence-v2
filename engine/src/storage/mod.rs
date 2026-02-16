pub mod traits;
pub mod memory;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use traits::{TripleStore, TriplePattern};
pub use memory::MemoryStore;

#[cfg(feature = "postgres")]
pub use postgres::PgStore;
