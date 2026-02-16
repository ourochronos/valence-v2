//! Knowledge lifecycle management: structural decay and bounded memory.
//!
//! This module implements enhanced decay that considers structural properties
//! (not just time) and hard memory bounds with intelligent eviction.

pub mod decay;
pub mod bounds;

pub use decay::{DecayPolicy, LifecycleManager, DecayCycleResult};
pub use bounds::{MemoryBounds, BoundsStatus, EnforceResult};
