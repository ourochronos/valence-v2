pub mod models;
pub mod store;
pub mod memory;
pub mod patterns;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use models::{Session, Exchange, Pattern, Insight, SessionStatus, ExchangeRole, PatternStatus, Platform};
pub use store::SessionStore;
pub use memory::MemorySessionStore;
pub use patterns::{PatternDecayConfig, decay_patterns, search_patterns, create_pattern, reinforce_pattern};

#[cfg(feature = "postgres")]
pub use postgres::PgSessionStore;
