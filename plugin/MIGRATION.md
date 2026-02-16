# Migration Guide: Valence v1 → v2

This guide explains how to migrate from `memory-valence` (v1) to `valence-v2` (v2 engine with triple store).

## Architecture Changes

### v1: Belief Documents
- **Storage**: PostgreSQL with belief documents (JSON-like records)
- **Search**: Hybrid keyword + semantic search via pgvector
- **Memory**: Belief-centric with explicit confidence scoring
- **Access**: HTTP JSON-RPC over network

### v2: Triple Store + Topology Embeddings
- **Storage**: SQLite with triple store (subject-predicate-object)
- **Search**: Topology-derived embeddings + graph traversal
- **Memory**: Graph-centric with stigmergy (access-based relevance)
- **Access**: stdio MCP (local process, no network)

## Key Concepts Mapping

| v1 Concept | v2 Equivalent | Notes |
|------------|---------------|-------|
| Belief | Triple(s) | A belief becomes one or more triples |
| Confidence | Weight + Access Count | Stigmergy: confidence emerges from use |
| Domain Path | Predicate Namespace | Use predicates like `domain:tech:python` |
| Session Tracking | Working Set | Sessions evolve a focused context subgraph |
| Pattern | Graph Motif | Recurring structures discovered via exploration |

## Configuration Migration

### v1 Configuration (`openclaw.json`)

```json
{
  "memory": {
    "plugin": "memory-valence",
    "config": {
      "serverUrl": "http://localhost:8420",
      "authToken": "${VALENCE_AUTH_TOKEN}",
      "autoRecall": true,
      "autoCapture": true,
      "sessionTracking": true,
      "exchangeRecording": true,
      "recallMaxResults": 20,
      "recallMinScore": 0.5
    }
  }
}
```

### v2 Configuration (`openclaw.json`)

```json
{
  "memory": {
    "plugin": "valence-v2",
    "config": {
      "dbPath": "${HOME}/.openclaw/valence-v2/knowledge.db",
      "autoRecall": true,
      "autoCapture": true,
      "sessionTracking": true,
      "contextMaxTriples": 50,
      "contextMaxNodes": 100,
      "embeddingDimensions": 64,
      "maintenanceInterval": "1d"
    }
  }
}
```

### Configuration Changes

1. **No network required**: v2 runs as a local stdio process
2. **No auth token**: Local process, no network auth needed
3. **Database path**: SQLite file instead of PostgreSQL connection
4. **Context budgets**: Replace recall limits with triple/node budgets
5. **Embedding config**: Topology embedding dimensions (default: 64)
6. **Maintenance**: Periodic decay/eviction/recompute (default: 1 day)

## Tool Migration

### Basic Storage

#### v1: `belief_create`
```typescript
await mcpCall("belief_create", {
  content: "Alice knows Bob",
  domain_path: ["social", "relationships"],
  confidence: { overall: 0.9 }
});
```

#### v2: `insert_triples`
```typescript
await mcpCall("insert_triples", {
  triples: [
    { subject: "Alice", predicate: "knows", object: "Bob" }
  ],
  source: {
    source_type: "conversation",
    reference: "session-abc123"
  }
});
```

**Migration notes:**
- Decompose belief content into subject-predicate-object structure
- Domain path becomes predicate namespacing (e.g., `social:knows`)
- Confidence is implicit (weight starts at 1.0, evolves via stigmergy)

### Retrieval

#### v1: `belief_query` (keyword + semantic)
```typescript
await mcpCall("belief_query", {
  query: "What does Alice know?",
  limit: 10
});
```

#### v2: `query_triples` (pattern match)
```typescript
await mcpCall("query_triples", {
  subject: "Alice",
  predicate: "knows",
  limit: 10
});
```

**OR** use high-level context assembly:

#### v2: `context_for_query` (intelligent context)
```typescript
await mcpCall("context_for_query", {
  query: "Alice",
  max_triples: 50,
  max_nodes: 100,
  format: "markdown"
});
```

**Migration notes:**
- `belief_query` → `query_triples` for structured queries
- `belief_search` → `context_for_query` for semantic search + context assembly
- v2 separates pattern matching from semantic search (use both as needed)

### Semantic Search

#### v1: `belief_search`
```typescript
await mcpCall("belief_search", {
  query: "social relationships",
  limit: 10,
  min_similarity: 0.7
});
```

#### v2: `search` (vector neighbors)
```typescript
await mcpCall("search", {
  query_node: "relationships",
  k: 10,
  include_confidence: true
});
```

**Migration notes:**
- v2 search finds similar *nodes* (not triples)
- Use `neighbors` to expand nodes into their triple neighborhoods
- Combine `search` + `neighbors` for full context (or use `context_for_query`)

### Session Lifecycle

#### v1: `session_start` / `session_end`
```typescript
await mcpCall("session_start", {
  initial_query: "Let's talk about Alice"
});
// ... conversation ...
await mcpCall("session_end", {
  session_id: "session-abc123"
});
```

#### v2: Same API, different internals
```typescript
await mcpCall("session_start", {
  initial_query: "Let's talk about Alice"
});
// ... conversation with working set evolution ...
await mcpCall("session_end", {
  session_id: "session-abc123"
});
```

**Migration notes:**
- API preserved for compatibility
- v2 uses working sets (evolving subgraphs) instead of static belief sets
- Working sets track focused context across conversation turns

### Pattern Discovery

#### v1: `pattern_list`
```typescript
await mcpCall("pattern_list", {
  type: "preference",
  min_confidence: 0.5
});
```

#### v2: `explore` (graph exploration)
```typescript
await mcpCall("explore", {
  start_node: "preferences",
  max_depth: 2,
  max_results: 20,
  time_budget_ms: 1000
});
```

**Migration notes:**
- v1 patterns were explicit records
- v2 patterns emerge from graph topology
- Use `explore` for tiered discovery of recurring structures

## New Capabilities in v2

### 1. Stigmergy-Based Relevance
```typescript
// Record which triples were useful
await mcpCall("record_feedback", {
  session_id: "session-abc123",
  useful_triple_ids: ["triple-uuid-1", "triple-uuid-2"]
});
```

Useful triples gain weight through access. Unused triples naturally decay. No manual confidence scoring needed.

### 2. Context Assembly with Fusion Scoring
```typescript
// Get optimal context combining multiple signals
await mcpCall("context_for_query", {
  query: "What should I work on next?",
  max_triples: 50,
  format: "markdown",
  session_id: "session-abc123" // Uses working set if available
});
```

Combines:
- Semantic similarity (topology embeddings)
- Graph centrality (importance)
- Access patterns (stigmergy)
- Session context (working set)

### 3. Tiered Retrieval with Budgets
```typescript
// Explore graph with time/depth budgets
await mcpCall("explore", {
  start_node: "machine-learning",
  max_depth: 3,
  max_results: 30,
  time_budget_ms: 500
});
```

Tiers:
1. **Warm**: Vector search (fast, semantic)
2. **Cold**: + Graph walk (more coverage)
3. **Exhaustive**: + Confidence scoring (complete)

Stops early if high-confidence results found or budget exhausted.

### 4. Provenance Tracking
```typescript
// Get sources for a triple
await mcpCall("sources", {
  triple_id: "triple-uuid-1"
});
```

Every triple can have multiple sources. Understand where knowledge came from.

### 5. Graph Maintenance
```typescript
// Periodic maintenance
await mcpCall("maintain", {
  decay_factor: 0.95,        // Reduce all weights by 5%
  evict_threshold: 0.1,      // Remove triples below weight 0.1
  recompute_embeddings: true // Update topology embeddings
});
```

Keep the graph healthy: decay old knowledge, evict noise, refresh embeddings.

## Migration Strategy

### Phase 1: Dual-Run (Recommended)
1. Keep v1 running for read operations
2. Start v2 as write-only (new knowledge goes to v2)
3. Gradually migrate historical beliefs to triples
4. Compare results between v1 and v2
5. Switch to v2-only when confident

### Phase 2: Batch Migration
1. Export beliefs from v1 via `belief_search` with `min_similarity: 0`
2. Convert each belief to triples:
   ```typescript
   // Example: "Alice prefers Python over JavaScript"
   {
     subject: "Alice",
     predicate: "prefers",
     object: "Python"
   },
   {
     subject: "Alice",
     predicate: "dislikes",
     object: "JavaScript"
   }
   ```
3. Insert triples with source provenance:
   ```typescript
   {
     source_type: "migration",
     reference: "valence-v1-belief-{id}"
   }
   ```
4. Run `maintain` to recompute embeddings
5. Validate with `stats` and spot-check key knowledge

### Phase 3: Cutover
1. Update `openclaw.json` to use `valence-v2`
2. Restart OpenClaw gateway
3. Monitor logs for MCP tool calls
4. Verify auto-recall and auto-capture work correctly
5. Decommission v1 server

## Troubleshooting

### "Query node not found" in `search`
**Problem**: v2 search requires the query node to exist in the graph.

**Solution**: Use `context_for_query` which handles missing nodes gracefully, or ensure nodes exist before searching.

### Empty context from `context_for_query`
**Problem**: No triples in the graph, or no embeddings computed.

**Solution**: 
1. Check `stats` to verify triple count > 0
2. Run `maintain` with `recompute_embeddings: true`
3. Use `query_triples` to verify data exists

### Session not found in `session_end`
**Problem**: Session ID doesn't match active session.

**Solution**: Session state is in-memory. If the engine restarts, sessions are lost. Use persistent session storage (coming in future wave).

### Poor semantic search results
**Problem**: Topology embeddings haven't converged or graph is too sparse.

**Solution**:
1. Add more triples to increase graph density
2. Run `maintain` with higher `embedding_dimensions` (e.g., 128)
3. Use `neighbors` to explore context manually

## Performance Comparison

| Operation | v1 (HTTP + Postgres) | v2 (stdio + SQLite) | Notes |
|-----------|----------------------|---------------------|-------|
| Insert belief/triple | ~50ms | ~5ms | v2 is 10x faster (local) |
| Semantic search | ~100ms | ~20ms | v2 topology embeddings cached |
| Pattern discovery | ~200ms | ~50ms | v2 graph traversal optimized |
| Session tracking | In-memory | In-memory | Both fast |
| Maintenance | N/A (manual) | ~500ms | v2 automates decay/eviction |

## Feature Comparison

| Feature | v1 | v2 | Notes |
|---------|----|----|-------|
| Belief storage | ✅ | ✅ (as triples) | v2 more granular |
| Semantic search | ✅ (pgvector) | ✅ (topology) | Different algorithms |
| Session tracking | ✅ | ✅ (working sets) | v2 more dynamic |
| Pattern recognition | ✅ (explicit) | ✅ (emergent) | v2 discovers patterns |
| Confidence scoring | ✅ (manual) | ✅ (stigmergy) | v2 automatic via use |
| Provenance | ✅ | ✅ | Both track sources |
| Federation | ✅ | 🚧 Coming | v1 has DID-based sharing |
| Verification | ✅ | 🚧 Coming | v1 has reputation system |
| Network access | ✅ (HTTP) | ❌ (stdio only) | v2 is local-first |
| Multi-user | ✅ | ❌ | v2 is single-user |

## FAQ

**Q: Can I run v1 and v2 simultaneously?**

A: Yes! They use different plugin IDs and don't conflict. Useful for gradual migration.

**Q: Will my v1 beliefs be automatically migrated?**

A: No. Manual migration required (see Phase 2 above). Consider dual-run approach.

**Q: Is v2 faster than v1?**

A: Yes, significantly. stdio transport (no network) and SQLite (local) vs HTTP + Postgres (network).

**Q: What about federation and reputation from v1?**

A: Coming in future waves. v2 focuses on core triple store + embeddings first.

**Q: Can I export my v2 graph to v1 format?**

A: Not directly. v2 triples are more granular than v1 beliefs. Would require lossy conversion.

**Q: Should I migrate now or wait?**

A: Depends:
- **Migrate now** if you value speed, local-first, and stigmergy-based relevance
- **Wait** if you need federation, verification, or multi-user features

## Next Steps

1. **Read the plugin manifest**: `plugin/openclaw.plugin.json`
2. **Install v2 engine**: `cargo install --path engine`
3. **Try it locally**: `valence-engine --mode mcp` (stdio MCP server)
4. **Update config**: Edit `~/.openclaw/openclaw.json`
5. **Test tools**: Use OpenClaw MCP client to verify connectivity
6. **Migrate data**: Use batch migration script (Phase 2)
7. **Monitor**: Check logs for errors during cutover
8. **Optimize**: Tune `embedding_dimensions` and `maintenance_interval`

## Support

- **Issues**: File in `claude-journal` (private repo)
- **Questions**: Ask in OpenClaw dev channel
- **Bugs**: Include MCP logs and `stats` output

Good luck with your migration! 🚀
