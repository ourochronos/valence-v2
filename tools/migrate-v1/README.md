# migrate-v1: Valence v1 to v2 Migration Tool

Migrate Valence v1 beliefs from PostgreSQL to Valence v2 triple-based knowledge graph.

## Overview

This tool reads beliefs from a Valence v1 PostgreSQL database and converts them into v2 triples using a simple heuristic approach:

- **Each belief becomes a content node** (truncated to 200 chars)
- **Domain paths become hierarchical nodes** connected via `subdomain_of` predicates
- **Beliefs link to domains** via `belongs_to_domain` predicates
- **Entity references become nodes** with `mentions`/`about`/`in_context` predicates
- **Provenance metadata** preserves v1 belief ID, created_at, and confidence

## Installation

```bash
cd ~/projects/valence-v2
cargo build --release -p migrate-v1
```

Binary will be at `target/release/migrate-v1`.

## Usage

### Basic Migration

```bash
migrate-v1 \
  --v1-url "postgresql://user:pass@localhost:5433/valence" \
  --v2-url "http://localhost:8421"
```

### Environment Variables

```bash
export V1_URL="postgresql://user:pass@localhost:5433/valence"
export V2_URL="http://localhost:8421"

migrate-v1
```

### Dry Run (Output JSON)

```bash
migrate-v1 \
  --v1-url "postgresql://..." \
  --dry-run \
  > migration.json
```

The JSON output includes:
- `triples`: Array of all generated triples
- `stats`: Migration statistics

### Test with Limited Beliefs

```bash
migrate-v1 \
  --v1-url "postgresql://..." \
  --v2-url "http://localhost:8421" \
  --limit 100
```

## Triple Generation Strategy

### 1. Belief Content Node

```json
{
  "subject": "belief:<uuid>",
  "predicate": "has_content",
  "object": "Truncated belief content (max 200 chars)..."
}
```

### 2. Provenance Metadata

```json
{
  "subject": "belief:<uuid>",
  "predicate": "has_provenance",
  "object": "v1_migration",
  "metadata": {
    "v1_belief_id": "<uuid>",
    "v1_created_at": "2025-01-15T10:30:00Z",
    "v1_source_type": "manual",
    "v1_confidence": {"overall": 0.85, ...}
  }
}
```

### 3. Domain Structure

For a belief with `domain_path: ["tech", "languages", "rust"]`:

```json
// Leaf connection
{"subject": "belief:<uuid>", "predicate": "belongs_to_domain", "object": "domain:tech/languages/rust"}

// Domain labels
{"subject": "domain:tech", "predicate": "domain_label", "object": "tech"}
{"subject": "domain:tech/languages", "predicate": "domain_label", "object": "languages"}
{"subject": "domain:tech/languages/rust", "predicate": "domain_label", "object": "rust"}

// Hierarchy
{"subject": "domain:tech/languages", "predicate": "subdomain_of", "object": "domain:tech"}
{"subject": "domain:tech/languages/rust", "predicate": "subdomain_of", "object": "domain:tech/languages"}
```

### 4. Entity References

```json
// Entity node
{
  "subject": "entity:tool:<uuid>",
  "predicate": "entity_name",
  "object": "PostgreSQL",
  "metadata": {
    "entity_type": "tool",
    "v1_entity_id": "<uuid>"
  }
}

// Belief-entity link (role-based predicate)
{
  "subject": "belief:<uuid>",
  "predicate": "about",  // or "mentions", "in_context", "from_source"
  "object": "entity:tool:<uuid>",
  "metadata": {"role": "subject"}
}
```

## v1 Database Schema Assumptions

The tool expects the following v1 tables:

### `beliefs`
- `id` (UUID) - Primary key
- `content` (TEXT) - Belief content
- `domain_path` (TEXT[]) - Hierarchical domain
- `confidence` (JSONB) - Confidence dimensions
- `created_at` (TIMESTAMPTZ) - Creation timestamp
- `source_type` (TEXT) - Source type (optional)
- `status` (TEXT) - Must be 'active' to migrate

### `entities`
- `id` (UUID) - Primary key
- `name` (TEXT) - Entity name
- `type` (TEXT) - Entity type (person, organization, tool, concept, etc.)

### `belief_entities`
- `belief_id` (UUID) - FK to beliefs
- `entity_id` (UUID) - FK to entities
- `role` (TEXT) - subject, object, context, source

## Output to v2 API

Triples are sent to `POST {v2_url}/triples` in batches of 100:

```json
{
  "triples": [
    {"subject": "...", "predicate": "...", "object": "...", "metadata": {...}},
    ...
  ]
}
```

## Testing

```bash
cargo test -p migrate-v1
```

Tests cover:
- Content truncation
- Domain node ID generation
- Basic triple generation
- Entity linking
- Domain path decomposition

## Migration Statistics

After completion, the tool reports:

```
beliefs_processed: 1234
triples_generated: 5678
domain_nodes_created: 89
entity_references: 456
```

## Notes

- **Read-only**: The tool never modifies the v1 database
- **Batching**: Sends triples in batches of 100 to avoid overwhelming the v2 API
- **No LLM parsing**: Uses simple heuristics, not natural language understanding
- **Graph structure**: Emerges from shared domain nodes and entity references
- **Idempotent**: Safe to re-run (v2 should handle duplicate prevention)

## Troubleshooting

### Connection refused to v1 database

Check that the PostgreSQL container is running:
```bash
docker ps | grep valence-pg
```

If using the valence-pg container on port 5433:
```bash
--v1-url "postgresql://postgres:password@localhost:5433/valence"
```

### v2 API returns errors

- Ensure valence-engine is running: `curl http://localhost:8421/health`
- Check v2 logs for validation errors
- Use `--dry-run` to inspect generated triples

### Missing entities

The tool only migrates beliefs with `status = 'active'`. Archived beliefs are skipped.

## Future Enhancements

- LLM-based content parsing to extract better subject-predicate-object triples
- Confidence dimension mapping to v2 edge weights
- Temporal validity handling
- Tension detection and migration
- Resume capability for large migrations
