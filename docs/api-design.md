# Valence v2 API Design

**Date:** 2026-02-16  
**Status:** Draft  
**Target:** MCP + HTTP API for agent interaction

## Design Principles

1. **Radical Simplification:** v1 had 56 tools. v2 has 7 core operations.
2. **Triple-First:** Everything is a triple or derived from triples.
3. **Computed, Not Stored:** Confidence, trust, consensus emerge from graph topology.
4. **Clean Separation:** Engine is deterministic. LLM inference only at boundaries.
5. **HTTP + MCP:** HTTP for flexibility, MCP for Claude integration.

## Core Operations

### 1. insert_triples

**Purpose:** Insert one or more triples with optional source provenance.

**Parameters:**
```typescript
{
  triples: Array<{
    subject: string,      // Node value (engine handles find-or-create)
    predicate: string,    // Relationship type
    object: string        // Node value
  }>,
  source?: {
    type: "conversation" | "observation" | "inference" | "user_input" | "document" | "decomposition",
    reference?: string,   // Session ID, document URL, etc.
    metadata?: object     // Arbitrary JSON
  }
}
```

**Returns:**
```typescript
{
  triple_ids: string[],   // UUIDs of created triples
  source_id?: string      // UUID of source record if provided
}
```

**Example:**
```json
{
  "triples": [
    {
      "subject": "Chris",
      "predicate": "lives_in",
      "object": "San Francisco"
    },
    {
      "subject": "Chris",
      "predicate": "works_on",
      "object": "Valence"
    }
  ],
  "source": {
    "type": "conversation",
    "reference": "session-abc-123"
  }
}
```

**Response:**
```json
{
  "triple_ids": [
    "550e8400-e29b-41d4-a716-446655440000",
    "660e8400-e29b-41d4-a716-446655440001"
  ],
  "source_id": "770e8400-e29b-41d4-a716-446655440002"
}
```

**HTTP:** `POST /triples`

---

### 2. query

**Purpose:** Hybrid retrieval: find relevant triples by pattern or natural language.

**Parameters:**
```typescript
{
  // Option A: Pattern matching (wildcards via null)
  subject?: string | null,   // null = wildcard
  predicate?: string | null,
  object?: string | null,
  
  // Option B: Natural language (future: vector search)
  query?: string,
  
  // Filters
  limit?: number,            // Default: 100
  include_sources?: boolean  // Default: false
}
```

**Returns:**
```typescript
{
  triples: Array<{
    id: string,
    subject: { id: string, value: string },
    predicate: string,
    object: { id: string, value: string },
    weight: number,
    created_at: string,
    last_accessed: string,
    access_count: number,
    sources?: Array<{      // If include_sources=true
      id: string,
      type: string,
      reference?: string,
      created_at: string
    }>
  }>
}
```

**Example (pattern matching):**
```json
{
  "subject": "Chris",
  "predicate": null,
  "object": null,
  "limit": 10,
  "include_sources": true
}
```

**Response:**
```json
{
  "triples": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "subject": {
        "id": "abc-123",
        "value": "Chris"
      },
      "predicate": "lives_in",
      "object": {
        "id": "def-456",
        "value": "San Francisco"
      },
      "weight": 1.0,
      "created_at": "2026-02-16T08:45:00Z",
      "last_accessed": "2026-02-16T08:45:00Z",
      "access_count": 0,
      "sources": [
        {
          "id": "770e8400-e29b-41d4-a716-446655440002",
          "type": "conversation",
          "reference": "session-abc-123",
          "created_at": "2026-02-16T08:45:00Z"
        }
      ]
    }
  ]
}
```

**HTTP:** `GET /triples?subject=Chris&predicate=&object=&limit=10&include_sources=true`

---

### 3. neighbors

**Purpose:** Get the subgraph around a node (all triples within k hops).

**Parameters:**
```typescript
{
  node: string,        // Node value or ID
  depth: number,       // How many hops (default: 1)
  limit?: number       // Max triples to return (default: 1000)
}
```

**Returns:**
```typescript
{
  triples: Array<Triple>,  // Same structure as query response
  node_count: number,      // Distinct nodes in subgraph
  triple_count: number     // Total triples in subgraph
}
```

**Example:**
```json
{
  "node": "Chris",
  "depth": 2,
  "limit": 100
}
```

**HTTP:** `GET /nodes/{node_value}/neighbors?depth=2&limit=100`

**Alternative:** `GET /nodes/{node_id}/neighbors?depth=2` (by UUID)

---

### 4. sources

**Purpose:** Get provenance for triples.

**Parameters:**
```typescript
{
  triple_ids: string[]   // One or more triple UUIDs
}
```

**Returns:**
```typescript
{
  sources: Array<{
    id: string,
    triple_ids: string[],
    type: "conversation" | "observation" | "inference" | "user_input" | "document" | "decomposition",
    reference?: string,
    metadata?: object,
    created_at: string
  }>
}
```

**Example:**
```json
{
  "triple_ids": ["550e8400-e29b-41d4-a716-446655440000"]
}
```

**HTTP:** `GET /triples/{triple_id}/sources` (single)  
**Or:** `POST /sources/batch` with `{"triple_ids": [...]}` (multiple)

---

### 5. stats

**Purpose:** Engine statistics.

**Parameters:** None

**Returns:**
```typescript
{
  triple_count: number,
  node_count: number,
  source_count: number,
  avg_weight: number,
  storage_size?: number  // Optional: storage backend size in bytes
}
```

**Example Response:**
```json
{
  "triple_count": 12847,
  "node_count": 4521,
  "source_count": 892,
  "avg_weight": 0.87
}
```

**HTTP:** `GET /stats`

---

### 6. decay

**Purpose:** Trigger decay cycle (reduce weight of stale triples).

**Parameters:**
```typescript
{
  factor: number,      // Decay multiplier (0.0-1.0, e.g., 0.95)
  min_weight: number   // Don't decay below this (e.g., 0.1)
}
```

**Returns:**
```typescript
{
  affected_count: number   // Number of triples whose weight changed
}
```

**Example:**
```json
{
  "factor": 0.95,
  "min_weight": 0.1
}
```

**Response:**
```json
{
  "affected_count": 347
}
```

**HTTP:** `POST /maintenance/decay`

---

### 7. evict

**Purpose:** Remove low-weight triples (garbage collection).

**Parameters:**
```typescript
{
  threshold: number   // Remove triples with weight < threshold
}
```

**Returns:**
```typescript
{
  evicted_count: number   // Number of triples deleted
}
```

**Example:**
```json
{
  "threshold": 0.05
}
```

**Response:**
```json
{
  "evicted_count": 23
}
```

**HTTP:** `POST /maintenance/evict`

---

## Comparison to v1

### What Was Eliminated

| v1 Category | v1 Tool Count | v2 Approach | Reduction |
|-------------|---------------|-------------|-----------|
| **Belief CRUD** | 9 tools | → `insert_triples`, `query`, `sources` | 9 → 3 |
| **Entity Operations** | 2 tools | → Built into `query` (entities are just nodes) | 2 → 0 |
| **Tension/Conflict** | 2 tools | → Dynamic detection via `query` (conflicting triples) | 2 → 0 |
| **Trust & Verification** | 9 tools | → Future: computed from graph topology | 9 → 0 |
| **Incentives & Reputation** | 12 tools | → Future: reputation as triples + computed scores | 12 → 0 |
| **Consensus** | 9 tools | → Future: trust layers from corroboration graph | 9 → 0 |
| **Backup/Resilience** | 4 tools | → Infrastructure (not in core API) | 4 → 0 |
| **Session/VKB** | 9 tools | → Sessions as triples (future wave) | 9 → 0 |
| **Maintenance** | 0 tools | → `decay`, `evict` | 0 → 2 |
| **TOTAL** | **56 tools** | → **7 operations** | **87.5% reduction** |

### Why This Works

1. **Belief CRUD → Triple CRUD:** `belief_create` becomes `insert_triples`. `belief_query` becomes `query`. `belief_get` is just `query` with an ID filter.

2. **Entities Are Nodes:** No separate entity table. "Chris" is a node, "San Francisco" is a node. `entity_search` becomes `query` for nodes.

3. **Computed, Not Stored:** 
   - **Confidence:** Derived from corroboration graph topology (future)
   - **Trust:** PageRank on verification edges (future)
   - **Consensus:** L1-L4 layers from corroboration count (future)
   - **Tensions:** Detect conflicting triples dynamically

4. **Maintenance Made Explicit:** v1 had no decay/eviction API. v2 exposes these for observability.

5. **VKB as Triples:** Sessions, exchanges, patterns will be modeled as triples (future wave).

### What We're Not Building Yet

**Deferred to future waves:**
- Vector search (for natural language `query`)
- Topology-derived embeddings (node2vec, GNN)
- Verification/dispute workflows (triples exist, game theory comes later)
- Reputation scoring (triples exist, computation comes later)
- Consensus layers (corroboration triples exist, trust layer computation comes later)
- Federation (share triples between nodes)

**Current scope (Wave 4):**
- Pattern matching on triples (subject/predicate/object wildcards)
- Provenance tracking (sources linked to triples)
- Basic maintenance (decay, eviction)

---

## MCP Integration

Valence v2 will expose these 7 operations as MCP tools:

```json
{
  "tools": [
    {
      "name": "valence_insert_triples",
      "description": "Insert one or more triples with optional source provenance",
      "inputSchema": { ... }
    },
    {
      "name": "valence_query",
      "description": "Find triples by pattern matching (subject/predicate/object wildcards)",
      "inputSchema": { ... }
    },
    {
      "name": "valence_neighbors",
      "description": "Get subgraph around a node (k-hop neighborhood)",
      "inputSchema": { ... }
    },
    {
      "name": "valence_sources",
      "description": "Get provenance for triples",
      "inputSchema": { ... }
    },
    {
      "name": "valence_stats",
      "description": "Get engine statistics (triple count, node count, etc.)",
      "inputSchema": { ... }
    },
    {
      "name": "valence_decay",
      "description": "Trigger decay cycle to reduce weight of stale triples",
      "inputSchema": { ... }
    },
    {
      "name": "valence_evict",
      "description": "Remove low-weight triples (garbage collection)",
      "inputSchema": { ... }
    }
  ]
}
```

The HTTP API and MCP tools share the same JSON schema for requests/responses.

---

## Error Handling

All endpoints return standard HTTP status codes:

- `200 OK` — Success
- `400 Bad Request` — Invalid parameters
- `404 Not Found` — Triple/node not found (for GET by ID)
- `500 Internal Server Error` — Storage backend failure

Error responses:
```json
{
  "error": "Invalid triple pattern",
  "details": "Predicate cannot be empty string; use null for wildcard"
}
```

---

## Next Steps

1. **Implement HTTP server** (`engine/src/api/mod.rs`) with axum
2. **Write integration tests** (start server, make HTTP requests)
3. **Add Cargo dependencies:** axum, tokio, serde_json
4. **Future waves:**
   - Vector search endpoint
   - MCP server implementation
   - Streaming responses for large subgraphs
   - WebSocket support for live updates
