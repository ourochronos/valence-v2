# PostgreSQL Storage Backend

## Overview

The PostgreSQL storage backend (`PgStore`) provides a persistent, production-ready implementation of the `TripleStore` trait backed by PostgreSQL via sqlx.

## Features

- Full implementation of the `TripleStore` trait
- Native UUID support for nodes, triples, and sources
- Optimized indexes for SPO, POS, and OSP query patterns
- Automatic schema initialization
- Recursive CTE for efficient multi-hop neighbor queries
- Cascade delete for source-triple relationships
- JSONB support for source metadata

## Schema

### Tables

**nodes**
- `id` UUID PRIMARY KEY
- `value` TEXT NOT NULL (unique)
- `node_type` TEXT
- `created_at` TIMESTAMPTZ
- `last_accessed` TIMESTAMPTZ
- `access_count` BIGINT

**triples**
- `id` UUID PRIMARY KEY
- `subject_id` UUID REFERENCES nodes(id)
- `predicate` TEXT
- `object_id` UUID REFERENCES nodes(id)
- `weight` DOUBLE PRECISION
- `access_count` BIGINT
- `created_at` TIMESTAMPTZ
- `last_accessed` TIMESTAMPTZ

**sources**
- `id` UUID PRIMARY KEY
- `source_type` TEXT
- `reference` TEXT
- `created_at` TIMESTAMPTZ
- `metadata` JSONB

**source_triples** (junction table)
- `source_id` UUID REFERENCES sources(id) ON DELETE CASCADE
- `triple_id` UUID REFERENCES triples(id) ON DELETE CASCADE

### Indexes

- `idx_nodes_value` - Unique index on node values for fast lookups
- `idx_triples_spo` - Subject-Predicate-Object queries
- `idx_triples_pos` - Predicate-Object-Subject queries
- `idx_triples_osp` - Object-Subject-Predicate queries
- `idx_source_triples_triple` - Source lookup by triple

## Usage

### Cargo.toml

```toml
[dependencies]
valence-engine = { version = "0.1", features = ["postgres"] }

[features]
postgres = ["sqlx"]
```

### Code

```rust
use valence_engine::storage::PgStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = "postgresql://user:pass@localhost:5433/valence_v2";
    let store = PgStore::new(database_url).await?;
    
    // Use like any other TripleStore
    let node = store.find_or_create_node("Alice").await?;
    
    Ok(())
}
```

## Testing

Tests are gated behind the `RUN_PG_TESTS` environment variable to avoid requiring a database in CI:

```bash
# Run PostgreSQL tests
RUN_PG_TESTS=1 cargo test --features postgres

# Skip PostgreSQL tests (default)
cargo test
```

Test database URL: `postgresql://valence:valence@localhost:5433/valence_v2_test`

## Performance Considerations

- Uses connection pooling via sqlx::PgPool
- Prepared statements for all queries
- Batch operations where possible
- Indexes on all common query patterns

## Migration from MemoryStore

The `TripleStore` trait provides a common interface, so switching is straightforward:

```rust
// Before
let store = MemoryStore::new();

// After
let store = PgStore::new(database_url).await?;
```

## Database Setup

The schema is automatically initialized on first connection. For production:

1. Create a dedicated database: `CREATE DATABASE valence_v2;`
2. Create a user with appropriate permissions
3. Connection string: `postgresql://user:pass@host:port/valence_v2`

## Arc<T> Support

The trait implementation includes a blanket implementation for `Arc<T>` where `T: TripleStore`, allowing the PostgreSQL backend to be used seamlessly with `Arc` wrapping (as used in `ValenceEngine`).
