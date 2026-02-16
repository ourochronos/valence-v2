//! Configuration for tiered storage behavior.

/// Policy for promoting triples from cold to hot tier.
#[derive(Debug, Clone)]
pub enum PromotionPolicy {
    /// Promote after N accesses within the tracking window
    AccessThreshold {
        /// Minimum number of accesses to promote
        min_accesses: u64,
    },
    /// Promote if access frequency exceeds this rate (accesses per hour)
    FrequencyThreshold {
        /// Minimum accesses per hour
        min_frequency: f64,
    },
    /// Promote on first access (aggressive caching)
    Immediate,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self::AccessThreshold { min_accesses: 3 }
    }
}

/// Policy for demoting triples from hot to cold tier.
#[derive(Debug, Clone)]
pub enum DemotionPolicy {
    /// Demote if not accessed for N hours
    IdleTimeout {
        /// Hours without access before demotion
        hours: i64,
    },
    /// Demote the least recently used triple when hot tier is full (LRU)
    LeastRecentlyUsed,
    /// Never demote (hot tier keeps growing until capacity is reached, then errors)
    Never,
}

impl Default for DemotionPolicy {
    fn default() -> Self {
        Self::LeastRecentlyUsed
    }
}

/// Configuration for the TieredStore.
#[derive(Debug, Clone)]
pub struct TieredConfig {
    /// Maximum number of triples in hot tier (0 = unlimited)
    pub hot_capacity: usize,
    
    /// Policy for promoting triples to hot tier
    pub promotion_policy: PromotionPolicy,
    
    /// Policy for demoting triples to cold tier
    pub demotion_policy: DemotionPolicy,
    
    /// How often to run the demotion sweep (in seconds)
    pub demotion_interval_secs: u64,
    
    /// Enable cold tier (if false, acts as memory-only store)
    pub enable_cold_tier: bool,
    
    /// Track access patterns for promotion decisions
    pub track_accesses: bool,
}

impl Default for TieredConfig {
    fn default() -> Self {
        Self {
            hot_capacity: 10_000, // 10k triples in memory by default
            promotion_policy: PromotionPolicy::default(),
            demotion_policy: DemotionPolicy::default(),
            demotion_interval_secs: 300, // 5 minutes
            enable_cold_tier: true,
            track_accesses: true,
        }
    }
}

impl TieredConfig {
    /// Create a config optimized for small deployments (aggressive caching)
    pub fn small() -> Self {
        Self {
            hot_capacity: 1_000,
            promotion_policy: PromotionPolicy::Immediate,
            demotion_policy: DemotionPolicy::LeastRecentlyUsed,
            demotion_interval_secs: 60,
            enable_cold_tier: true,
            track_accesses: true,
        }
    }
    
    /// Create a config optimized for large deployments (conservative caching)
    pub fn large() -> Self {
        Self {
            hot_capacity: 100_000,
            promotion_policy: PromotionPolicy::AccessThreshold { min_accesses: 5 },
            demotion_policy: DemotionPolicy::IdleTimeout { hours: 24 },
            demotion_interval_secs: 600,
            enable_cold_tier: true,
            track_accesses: true,
        }
    }
    
    /// Create a memory-only config (no cold tier)
    pub fn memory_only() -> Self {
        Self {
            hot_capacity: 0, // unlimited
            promotion_policy: PromotionPolicy::Immediate,
            demotion_policy: DemotionPolicy::Never,
            demotion_interval_secs: 0,
            enable_cold_tier: false,
            track_accesses: false,
        }
    }
}
