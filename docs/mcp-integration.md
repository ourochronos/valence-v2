# MCP Server Integration

**Status**: ✅ Implemented (Wave 8)  
**Commit**: `fbdbf09`

## Overview

Valence v2 engine now exposes an MCP (Model Context Protocol) server interface for integration with OpenClaw and other MCP clients. This provides a streamlined, stdio-based interface with 7 core tools (down from 58 in v1).

## Server Modes

The `valence-engine` binary supports three operational modes:

```bash
# HTTP server only (default)
valence-engine --mode http --port 8421

# MCP stdio server only
valence-engine --mode mcp

# Both HTTP + MCP concurrently
valence-engine --mode both --port 8421
```

### Mode Details

**`--mode http`** (default)
- Runs HTTP REST API on specified port
- Suitable for web clients and direct API access
- See `docs/api-design.md` for endpoint documentation

**`--mode mcp`**
- Runs MCP server on stdio (stdin/stdout)
- JSON-RPC protocol over standard streams
- Designed for AI assistant integration (Claude, OpenClaw)
- No network port required

**`--mode both`**
- Runs both HTTP and MCP servers concurrently
- HTTP listens on specified port
- MCP operates on stdio
- Shares same ValenceEngine instance (atomic operations)

## MCP Tools

### 1. `insert_triples`

Insert one or more triples with optional source provenance.

**Input Schema**:
```json
{
  "triples": [
    {
      "subject": "string",
      "predicate": "string",
      "object": "string"
    }
  ],
  "source": {
    "type": "Conversation|Observation|Inference|UserInput|Document|Decomposition",
    "reference": "optional-string"
  }
}
```

**Output Schema**:
```json
{
  "triple_ids": ["uuid", "uuid"],
  "source_id": "optional-uuid"
}
```

**Use Case**: Store knowledge claims, facts, or relationships.

---

### 2. `query_triples`

Pattern-match query with wildcards (omit parameters for wildcards).

**Input Schema**:
```json
{
  "subject": "optional-string",
  "predicate": "optional-string",
  "object": "optional-string",
  "limit": "optional-number",
  "include_sources": "optional-boolean"
}
```

**Output Schema**:
```json
{
  "triples": [
    {
      "id": "uuid",
      "subject": {"id": "uuid", "value": "string"},
      "predicate": "string",
      "object": {"id": "uuid", "value": "string"},
      "weight": "number",
      "created_at": "timestamp",
      "last_accessed": "timestamp",
      "access_count": "number",
      "sources": "optional-array"
    }
  ]
}
```

**Use Case**: Retrieve stored knowledge by structure.

---

### 3. `search`

Semantic search for nodes similar to query using topology-derived embeddings.

**Input Schema**:
```json
{
  "query_node": "string",
  "k": "number (default: 10)",
  "include_confidence": "boolean (default: false)"
}
```

**Output Schema**:
```json
{
  "results": [
    {
      "node_id": "uuid",
      "value": "string",
      "similarity": "number (0-1)",
      "confidence": "optional-number"
    }
  ]
}
```

**Use Case**: Find conceptually related knowledge without exact matches.

---

### 4. `neighbors`

Get k-hop neighborhood (subgraph) around a node.

**Input Schema**:
```json
{
  "node": "string (value or UUID)",
  "depth": "number (default: 1)",
  "limit": "optional-number"
}
```

**Output Schema**:
```json
{
  "triples": [/* same as query_triples */],
  "node_count": "number",
  "triple_count": "number"
}
```

**Use Case**: Explore context and relationships around a concept.

---

### 5. `sources`

Get provenance sources for a specific triple.

**Input Schema**:
```json
{
  "triple_id": "uuid"
}
```

**Output Schema**:
```json
{
  "sources": [
    {
      "id": "uuid",
      "type": "SourceType",
      "reference": "optional-string",
      "created_at": "timestamp"
    }
  ]
}
```

**Use Case**: Understand where knowledge came from, assess trustworthiness.

---

### 6. `stats`

Get current engine statistics.

**Input Schema**: None (empty object or omit)

**Output Schema**:
```json
{
  "triple_count": "number",
  "node_count": "number",
  "avg_weight": "number"
}
```

**Use Case**: Monitor knowledge base size and health.

---

### 7. `maintain`

Run maintenance operations (decay, eviction, embedding recomputation).

**Input Schema**:
```json
{
  "decay_factor": "optional-number (0-1)",
  "evict_threshold": "optional-number",
  "recompute_embeddings": "optional-boolean",
  "embedding_dimensions": "optional-number (default: 64)"
}
```

**Output Schema**:
```json
{
  "decay": {"affected_count": "number"},
  "evict": {"evicted_count": "number"},
  "recompute_embeddings": {"embedding_count": "number"}
}
```

**Use Case**: Periodic housekeeping to keep knowledge base healthy.

---

## OpenClaw Integration

### Plugin Manifest

Located at `plugin/openclaw.plugin.json`:

```json
{
  "id": "valence-v2",
  "name": "Valence v2 Knowledge Engine",
  "kind": "memory",
  "transport": "stdio",
  "command": ["valence-engine", "--mode", "mcp"],
  "tools": [/* ... */]
}
```

### Installation (OpenClaw)

1. **Build the engine**:
   ```bash
   cd ~/projects/valence-v2/engine
   cargo build --release
   ```

2. **Install binary** (optional, for system-wide access):
   ```bash
   sudo cp target/release/valence-engine /usr/local/bin/
   ```

3. **Register plugin with OpenClaw**:
   ```bash
   openclaw plugins install ~/projects/valence-v2/plugin/openclaw.plugin.json
   ```

4. **Verify installation**:
   ```bash
   openclaw plugins list
   ```

### Usage in Claude

Once registered, the tools are available in Claude conversations:

```
[You]: Remember that I prefer using Rust for systems programming.

[Claude uses insert_triples]:
{
  "triples": [
    {
      "subject": "Chris",
      "predicate": "prefers_language_for",
      "object": "Rust"
    },
    {
      "subject": "Rust",
      "predicate": "used_for",
      "object": "systems_programming"
    }
  ],
  "source": {
    "type": "Conversation",
    "reference": "session-2026-02-16-01"
  }
}

[You]: What programming language do I like?

[Claude uses query_triples]:
{
  "subject": "Chris",
  "predicate": "prefers_language_for"
}

[Claude]: Based on our previous conversation, you prefer Rust for systems programming.
```

## Technical Details

### Architecture

```
┌─────────────────────────────────────┐
│     OpenClaw / Claude Desktop       │
└─────────────────────────────────────┘
                  │
                  │ MCP (stdio)
                  ↓
┌─────────────────────────────────────┐
│    valence-engine --mode mcp        │
│  ┌───────────────────────────────┐  │
│  │      McpServer (rmcp)         │  │
│  │   - Tool routing              │  │
│  │   - JSON schema generation    │  │
│  │   - Stdio transport           │  │
│  └───────────────────────────────┘  │
│                  │                   │
│  ┌───────────────────────────────┐  │
│  │     ValenceEngine             │  │
│  │   - TripleStore (MemoryStore) │  │
│  │   - EmbeddingStore            │  │
│  │   - Lifecycle management      │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

### Implementation

- **MCP SDK**: Uses `rmcp` crate (official Rust MCP implementation)
- **Tool definitions**: Procedural macros (`#[tool]`, `#[tool_router]`, `#[tool_handler]`)
- **Schema generation**: Automatic via `schemars` (JsonSchema derives)
- **Transport**: Stdio (stdin/stdout) for compatibility with MCP clients
- **Consistency**: All tools map directly to HTTP API endpoints

### Testing

The MCP server shares the same ValenceEngine as the HTTP API, so all existing integration tests validate MCP behavior:

```bash
cd engine
cargo test  # All tests pass (25 tests)
```

Specific test coverage:
- ✅ Triple insertion and querying
- ✅ Semantic search with embeddings
- ✅ Neighborhood traversal
- ✅ Source provenance tracking
- ✅ Statistics aggregation
- ✅ Maintenance operations (decay/evict/recompute)

### Error Handling

Tools return `Result<Json<T>, String>`:
- **Success**: `Ok(Json(response))` - Serialized JSON response
- **Validation errors**: `Err("Invalid input: ...")` - User-facing error message
- **Internal errors**: `Err(e.to_string())` - System error propagated as string

MCP protocol wraps these in standard JSON-RPC error responses.

## Comparison with v1

### Tool Count

| Aspect | v1 | v2 |
|--------|----|----|
| Total tools | 58 | 7 |
| Substrate tools | 42 | 7 (unified) |
| VKB tools | 16 | — (future) |

### Design Philosophy

**v1**: Comprehensive feature set
- 6-dimensional confidence
- Multi-dimensional trust networks
- Verification protocol with staking
- Consensus mechanism (L1→L4)
- Federation protocol
- Incentive system (calibration, bounties)

**v2**: Deterministic core
- Single weight score (for now)
- Topology-derived embeddings
- Provenance tracking
- Graph-based operations
- Clean separation of concerns
- Foundation for incremental feature addition

### Migration Path

V2 tools map to v1 equivalents:

| v2 Tool | v1 Equivalent(s) |
|---------|------------------|
| `insert_triples` | `belief_create` |
| `query_triples` | `belief_query` |
| `search` | `belief_search` |
| `neighbors` | (new - graph traversal) |
| `sources` | (partially `belief_sources`) |
| `stats` | (new - aggregated stats) |
| `maintain` | `maintenance/decay`, `maintenance/evict`, `maintenance/recompute-embeddings` |

## Future Enhancements

Planned additions (not yet implemented):

1. **Multi-dimensional confidence** (v1 parity)
   - Source reliability, method quality, temporal freshness, etc.
   - Tool: `confidence_explain`

2. **Trust network** (v1 parity)
   - DID-based identity
   - Trust scoring (competence, integrity, confidentiality)
   - Tool: `trust_query`, `trust_update`

3. **Verification protocol** (v1 parity)
   - Stake-based quality control
   - Evidence submission
   - Tool: `verification_submit`, `verification_challenge`

4. **Conversation tracking** (v1 VKB parity)
   - Session management
   - Pattern recognition
   - Tool: `session_start`, `session_end`, `pattern_record`

5. **Federation** (v1 parity)
   - P2P belief sharing
   - Privacy-preserving aggregation
   - Tool: `federation_share`, `federation_query`

All future tools will follow the same pattern: JSON schema, deterministic operation, maps to HTTP endpoint.

## Troubleshooting

### MCP server not starting

**Symptom**: `valence-engine --mode mcp` exits immediately

**Check**:
1. Ensure stdio is not redirected: `valence-engine --mode mcp < /dev/null` should fail
2. Verify build: `cargo build --release` shows no errors
3. Check dependencies: `cargo tree | grep rmcp` shows version 0.15.0

### OpenClaw plugin not recognized

**Symptom**: `openclaw plugins list` doesn't show `valence-v2`

**Check**:
1. Plugin JSON is valid: `cat plugin/openclaw.plugin.json | jq .`
2. Binary is in PATH: `which valence-engine`
3. OpenClaw version supports stdio transport

### Tools return errors

**Symptom**: MCP tools return `{"success": false, "error": "..."}`

**Check**:
1. Engine has data: `valence-engine --mode http` then `GET /stats`
2. Embeddings computed: Run `maintain` with `recompute_embeddings: true`
3. Check tool parameters match schema (use `--help` or inspect plugin JSON)

## References

- [MCP Specification](https://modelcontextprotocol.io/specification)
- [rmcp Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [OpenClaw Documentation](https://openclaw.dev/docs)
- [Valence v1 Architecture](./v1-architecture.md)
- [Valence v2 API Design](./api-design.md)

---

**Last Updated**: 2026-02-16  
**Author**: Wave 8 implementation (MCP integration)
