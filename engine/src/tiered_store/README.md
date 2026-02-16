# Tiered Storage: Cold/Warm Split

## Overview

The `TieredStore` implements a cold/warm split for Valence's knowledge substrate:
- **Hot tier** (MemoryStore): Frequently accessed triples stay in memory for fast access
- **Cold tier** (PgStore): Infrequently accessed triples live in PostgreSQL to save memory

This enables:
- **Memory-bounded operation**: Hot tier has a configurable capacity limit
- **Fast access for hot data**: Frequently used knowledge stays in memory
- **Persistent storage for cold data**: Full corpus lives in PostgreSQL
- **Graceful degradation**: System works without cold tier (memory-only mode)

## Architecture

```
┌─────────────────────────────────────────────────┐
│               TieredStore                        │
│  (implements TripleStore trait)                  │
├─────────────────────────────────────────────────┤
│                                                  │
│  ┌────────────────┐      ┌──────────────────┐  │
│  │   Hot Tier     │      │   Cold Tier      │  │
│  │  MemoryStore   │◄────►│   PgStore        │  │
│  │  (in-memory)   │      │  (PostgreSQL)    │  │
│  └────────────────┘      └──────────────────┘  │
│         ▲                                        │
│         │                                        │
│  ┌──────┴──────────┐                            │
│  │ Access Tracker  │                            │
│  │  (stigmergy)    │                            │
│  └─────────────────┘                            │
│                                                  │
└─────────────────────────────────────────────────┘
```

## Usage

### Memory-Only Mode (No Cold Tier)

```rust
use valence_engine::TieredStore;

// Simplest setup: everything in memory
let store = TieredStore::new_memory_only();
```

### With PostgreSQL Cold Tier

```rust
use valence_engine::{TieredStore, TieredConfig, PgStore};

// Create cold tier
let pg_store = PgStore::new(&database_url).await?;

// Create tiered store with custom config
let config = TieredConfig {
    hot_capacity: 10_000,      // Max 10k triples in memory
    promotion_policy: PromotionPolicy::AccessThreshold { min_accesses: 3 },
    demotion_policy: DemotionPolicy::LeastRecentlyUsed,
    enable_cold_tier: true,
    track_accesses: true,
    ..Default::default()
};

let store = TieredStore::with_postgres(config, pg_store);
```

### Preset Configurations

```rust
// Small deployment (aggressive caching)
let config = TieredConfig::small();

// Large deployment (conservative caching)
let config = TieredConfig::large();

// Memory-only (no cold tier)
let config = TieredConfig::memory_only();
```

## Promotion Policies

### AccessThreshold (Default)

Promote to hot tier after N accesses:

```rust
PromotionPolicy::AccessThreshold { min_accesses: 3 }
```

### FrequencyThreshold

Promote based on access frequency (accesses per hour):

```rust
PromotionPolicy::FrequencyThreshold { min_frequency: 2.0 }
```

### Immediate

Promote on first access (aggressive caching):

```rust
PromotionPolicy::Immediate
```

## Demotion Policies

### LeastRecentlyUsed (Default)

When hot tier is full, demote the LRU triple:

```rust
DemotionPolicy::LeastRecentlyUsed
```

### IdleTimeout

Demote triples not accessed for N hours:

```rust
DemotionPolicy::IdleTimeout { hours: 24 }
```

### Never

Never demote (hot tier grows until capacity is reached):

```rust
DemotionPolicy::Never
```

## Query Flow

1. **Check hot tier first**: `O(1)` lookup in HashMap
2. **Fall back to cold tier**: Query PostgreSQL if not in hot
3. **Consider promotion**: If found in cold and meets promotion criteria
4. **Track access**: Record for future promotion decisions

```rust
// All queries automatically check both tiers
let triple = store.get_triple(triple_id).await?;

// Queries merge results from hot and cold
let results = store.query_triples(pattern).await?;
```

## Maintenance

### Manual Demotion Sweep

```rust
// Run demotion sweep based on configured policy
let demoted_count = store.run_demotion_sweep().await?;
println!("Demoted {} triples", demoted_count);
```

### Monitoring

```rust
// Check hot tier size
let hot_size = store.hot_size().await;

// Get metadata for a specific triple
if let Some((tier, last_accessed, access_count)) = store.get_metadata(triple_id).await {
    println!("Tier: {:?}, Last accessed: {}, Access count: {}", 
             tier, last_accessed, access_count);
}
```

## Stigmergy Integration

The tiered store integrates with Valence's stigmergy system (access tracking):
- Every `get_triple()` call records an access
- Access patterns inform promotion decisions
- Co-accessed triples trend toward hot tier together

This creates a self-organizing system where the structure (which triples are hot vs cold) reflects usage patterns.

## Design Rationale

### Why Not LRU Cache?

A simple LRU cache would work, but the tiered approach provides:
1. **Explicit control**: Separate policies for promotion vs demotion
2. **Stigmergy integration**: Access patterns influence structure
3. **Graceful degradation**: Works without cold tier
4. **Transparency**: Clear tier boundaries, not just cache hits/misses

### Why Track Access Patterns?

Access tracking enables:
- **Smarter promotion**: Not just "was it accessed?" but "how often?"
- **Co-retrieval awareness**: Triples accessed together can be promoted together
- **Usage-driven structure**: The graph organizes around how it's actually used

### Cold/Warm Split vs Hot/Cold

The design doc uses "cold/warm" terminology (cold engine = deterministic core, warm engine = inference enrichment). This implementation uses "hot/cold" for storage tiers to avoid confusion:
- **Hot tier** = in-memory (fast)
- **Cold tier** = on-disk (persistent)

Both are part of the "cold engine" (deterministic core, no inference).

## Performance Characteristics

| Operation | Hot Tier | Cold Tier | Tiered |
|-----------|----------|-----------|--------|
| Insert | O(1) | O(1) + disk | O(1) + disk |
| Get (hot) | O(1) | - | O(1) |
| Get (cold) | - | O(1) + disk | O(1) + disk + promotion |
| Query | O(n) | O(n) + disk | O(n) + O(m) + merge |
| Promotion | - | - | O(1) + copy |
| Demotion | - | - | O(1) + write |

Where:
- `n` = results in hot tier
- `m` = results in cold tier
- `disk` = PostgreSQL query time

## Future Work

- [ ] Batch promotion/demotion for efficiency
- [ ] Predictive promotion based on co-retrieval patterns
- [ ] Multi-level hierarchy (hot → warm → cold)
- [ ] Compression for cold tier to save space
- [ ] Background worker for automatic demotion sweeps
- [ ] Metrics and observability (Prometheus integration)
- [ ] Configurable promotion/demotion strategies via trait
