# VKB: Conversation Tracking System

VKB (Valence Knowledge Base) tracks conversations at three scales:
- **Micro**: Individual exchanges (turns)
- **Meso**: Sessions (one conversation)
- **Macro**: Patterns (across sessions)

## Architecture

```
vkb/
├── models.rs       — Core data types (Session, Exchange, Pattern, Insight)
├── store.rs        — SessionStore trait (CRUD interface)
├── memory.rs       — In-memory implementation (Arc<RwLock<...>>)
├── postgres.rs     — PostgreSQL implementation (feature-gated)
├── patterns.rs     — Pattern lifecycle logic (decay, reinforcement)
├── integration.md  — WorkingSet integration guide
└── README.md       — This file
```

## Data Model

### Session
Represents one conversation:
- `id`: Unique identifier
- `platform`: Where it happened (ClaudeCode, Matrix, Slack, etc.)
- `status`: Active, Completed, or Abandoned
- `project_context`: Optional project name
- `summary`: Optional summary when ended
- `themes`: Tags/topics discussed
- `external_room_id`: For chat platforms (Matrix room, Slack channel)
- `created_at`, `updated_at`, `ended_at`: Timestamps

### Exchange
Represents one turn in a conversation:
- `id`: Unique identifier
- `session_id`: Parent session
- `role`: User, Assistant, or System
- `content`: Message text
- `tokens_approx`: Estimated token count
- `tool_uses`: List of tools used
- `created_at`: Timestamp

Exchanges are stored in chronological order.

### Pattern
Represents a behavioral pattern observed across sessions:
- `id`: Unique identifier
- `pattern_type`: Category (preference, workflow, etc.)
- `description`: Human-readable description
- `confidence`: 0.0-1.0, starts at 0.4
- `evidence_session_ids`: Sessions where this pattern was observed
- `status`: Emerging → Established → Fading → Archived
- `created_at`, `updated_at`: Timestamps

#### Pattern Lifecycle
1. **Created** at confidence 0.4, status "emerging"
2. **Reinforcement** increases confidence by 0.1 (capped at 1.0)
3. When confidence >= 0.7, transitions to "established"
4. **Decay** reduces confidence over time (configurable factor)
5. When confidence < 0.2, transitions to "fading"
6. When confidence < 0.1, transitions to "archived"

### Insight
Links a session to triples in the knowledge graph:
- `id`: Unique identifier
- `session_id`: Source session
- `content`: Textual description of the insight
- `triple_ids`: Related triples in the knowledge graph
- `domain_path`: Hierarchical classification (e.g., `["tech", "rust"]`)
- `created_at`: Timestamp

## Usage

### In-Memory Store

```rust
use valence_engine::vkb::{
    Session, Exchange, Pattern, Insight,
    SessionStatus, ExchangeRole, Platform,
    MemorySessionStore, SessionStore,
};

let store = MemorySessionStore::new();

// Create session
let session = Session::new(Platform::ClaudeCode);
let session_id = store.create_session(session).await?;

// Add exchanges
let ex1 = Exchange::new(session_id, ExchangeRole::User, "How does async work?");
store.add_exchange(ex1).await?;

let ex2 = Exchange::new(session_id, ExchangeRole::Assistant, "Async allows non-blocking I/O...");
store.add_exchange(ex2).await?;

// Record pattern
let pattern = Pattern::new("preference", "User asks about async patterns");
store.record_pattern(pattern).await?;

// Extract insight
let mut insight = Insight::new(session_id, "User is learning async Rust");
insight.domain_path = vec!["tech".to_string(), "rust".to_string()];
store.extract_insight(insight).await?;

// End session
store.end_session(
    session_id,
    SessionStatus::Completed,
    Some("Discussed async patterns".to_string()),
    vec!["rust".to_string(), "async".to_string()],
).await?;
```

### Postgres Store

```rust
#[cfg(feature = "postgres")]
use valence_engine::vkb::PgSessionStore;

#[cfg(feature = "postgres")]
async fn use_postgres() -> Result<()> {
    let store = PgSessionStore::from_connection_string(
        "postgresql://valence:valence@localhost:5434/valence_v2"
    ).await?;

    // Same API as MemorySessionStore
    let session = Session::new(Platform::Matrix);
    let session_id = store.create_session(session).await?;

    Ok(())
}
```

### Pattern Management

```rust
use valence_engine::vkb::patterns::{
    create_pattern, reinforce_pattern, search_patterns,
    decay_patterns, PatternDecayConfig,
};

// Create pattern with default confidence (0.4)
let pattern_id = create_pattern(
    &store,
    "workflow",
    "Uses TDD approach",
    Some(vec![session_id]),
).await?;

// Reinforce pattern (increases confidence by 0.1)
reinforce_pattern(&store, pattern_id, Some(session_id)).await?;

// Search patterns (case-insensitive substring match)
let results = search_patterns(&store, "TDD", 10, Some(0.5)).await?;

// Apply decay to all patterns
let config = PatternDecayConfig::default(); // 5% decay per cycle
decay_patterns(&store, Some(config)).await?;
```

## Testing

20 comprehensive tests covering:

### Memory Store (10 tests)
- `test_session_lifecycle` — Create, get, add exchanges, end
- `test_session_listing` — Filter by status, platform, project
- `test_find_by_room` — Find session by external room ID
- `test_exchange_ordering` — Chronological order, pagination
- `test_pattern_creation` — Default confidence and status
- `test_pattern_reinforcement` — Confidence increase, status transition
- `test_pattern_search` — Case-insensitive substring search
- `test_pattern_filtering` — Filter by status, type
- `test_insight_extraction` — Link session to triples
- `test_concurrent_access` — Thread safety with Arc<RwLock>

### Pattern Lifecycle (10 tests)
- `test_create_pattern_default_values` — Confidence 0.4, status emerging
- `test_create_pattern_with_evidence` — With session IDs
- `test_reinforce_pattern_increases_confidence` — +0.1 per reinforcement
- `test_reinforce_pattern_transitions_to_established` — At confidence 0.7
- `test_reinforce_pattern_caps_at_1_0` — Maximum confidence
- `test_search_patterns_case_insensitive` — Substring matching
- `test_search_patterns_with_confidence_filter` — Minimum threshold
- `test_decay_patterns` — Confidence reduction over time
- `test_decay_config_custom` — Custom decay parameters
- `test_decay_skips_archived_patterns` — Don't decay archived

Run tests:
```bash
source ~/.cargo/env
cargo test --lib vkb
```

## Integration with WorkingSet

VKB can optionally integrate with `engine/src/context/working_set.rs`:

- Sessions can have an associated WorkingSet for tracking active concepts
- Exchanges can activate nodes in the WorkingSet
- Patterns can be extracted from WorkingSet threads

See `integration.md` for details.

## Design Decisions

1. **Trait-based design**: `SessionStore` trait allows multiple backends
2. **Thread-safe in-memory**: `Arc<RwLock<...>>` for concurrent access
3. **Chronological ordering**: Exchanges stored in creation order
4. **Pattern lifecycle**: Confidence-based transitions (emerging → established → fading → archived)
5. **Light WorkingSet integration**: Loosely coupled, can use independently
6. **Feature-gated Postgres**: `#[cfg(feature = "postgres")]` for optional persistence

## Future Work

Potential enhancements (not currently implemented):

1. **Pattern decay scheduler**: Periodic background task to decay patterns
2. **Insight linking**: Automatically link insights to triples via similarity
3. **Session resumption**: Resume conversations from previous sessions
4. **Pattern similarity**: Find similar patterns via embedding search
5. **WorkingSet auto-extraction**: Generate patterns from WorkingSet threads
6. **Stigmergy integration**: Use VKB access patterns for activation scores
