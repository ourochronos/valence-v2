# Valence Engine v2 - Implementation Requirements

**Extracted from**: valence-engine design docs (36 concept docs + OPEN.md)  
**Date**: 2026-02-16  
**Purpose**: Concrete implementation requirements for building valence-engine v2

---

## Table of Contents

1. [Data Model Requirements](#1-data-model-requirements)
2. [Query Requirements](#2-query-requirements)
3. [Computation Requirements](#3-computation-requirements)
4. [API Requirements](#4-api-requirements)
5. [Performance Requirements](#5-performance-requirements)
6. [Security Requirements](#6-security-requirements)
7. [Federation Requirements](#7-federation-requirements)
8. [Open Implementation Questions](#8-open-implementation-questions)

---

## 1. Data Model Requirements

### 1.1 Triple-Based Storage (Core)

**Status**: Settled (Confidence: 0.95)

The atomic unit of knowledge is the **triple**: `(subject, relationship, object)`.

#### Required Schema

**Triples Table** (Layer 1):
```sql
triples (
  id UUID PRIMARY KEY,
  subject_id UUID NOT NULL,
  relationship_id UUID NOT NULL,
  object_id UUID NOT NULL,
  content_hash BYTEA NOT NULL,  -- SHA-256 of (subject, relationship, object)
  created_at TIMESTAMPTZ NOT NULL,
  UNIQUE(content_hash)  -- Automatic deduplication
)

-- Three covering indexes for any query pattern
CREATE INDEX idx_triples_spo ON triples(subject_id, relationship_id, object_id);
CREATE INDEX idx_triples_pos ON triples(relationship_id, object_id, subject_id);
CREATE INDEX idx_triples_osp ON triples(object_id, subject_id, relationship_id);
```

**Sources Table** (Layer 2 - Provenance):
```sql
sources (
  id UUID PRIMARY KEY,
  triple_id UUID REFERENCES triples(id),
  origin_session UUID,  -- Where did this observation come from
  observed_at TIMESTAMPTZ NOT NULL,
  observer VARCHAR,  -- agent name or user ID
  context TEXT,  -- Optional: conversation context
  initial_confidence FLOAT DEFAULT 0.5,
  last_accessed TIMESTAMPTZ,
  access_count INTEGER DEFAULT 0,
  FOREIGN KEY (triple_id) REFERENCES triples(id) ON DELETE CASCADE
)

CREATE INDEX idx_sources_triple ON sources(triple_id);
CREATE INDEX idx_sources_session ON sources(origin_session);
CREATE INDEX idx_sources_accessed ON sources(last_accessed);
```

**Entities Table** (for node metadata):
```sql
entities (
  id UUID PRIMARY KEY,
  name VARCHAR NOT NULL,
  type VARCHAR,  -- Emergent type (person, concept, tool, etc.)
  created_at TIMESTAMPTZ,
  last_accessed TIMESTAMPTZ,
  access_count INTEGER DEFAULT 0,
  embedding VECTOR(384),  -- Optional: cached embedding
  UNIQUE(name)
)

CREATE INDEX idx_entities_name ON entities(name);
CREATE INDEX idx_entities_type ON entities(type);
```

#### Key Properties

1. **Content-addressed deduplication**: Same triple from different sources = one triple, multiple sources
2. **Immutable triples**: Once created, never modified (supersession handled via new triple + provenance chain)
3. **Timeless facts, temporal sources**: Triples are timeless; time-boundedness is in sources
4. **Provenance preservation**: Every observation is a separate source record
5. **Self-describing ontology**: The ontology describes itself using triples:
   - `(is_a, is_a, relationship_type)`
   - `(confidence_dimension, is_a, attribute_type)`

### 1.2 Layer 3: Summaries (Rendered, NOT Stored)

**Status**: Settled (Confidence: 0.95)

Summaries are **generated on-demand at query time** by the LLM, never stored.

- **Input**: Clusters of triples from retrieval
- **Output**: Natural language summary
- **Benefits**: No storage cost, no staleness, always reflects current graph state
- **Rendering**: LLM at read boundary composes triples → natural language

**Open Question Q37**: Should we cache summaries for expensive queries, or always render fresh?

### 1.3 Progressive Layers (L0-L4)

**Status**: Settled (Confidence: 0.95)

Knowledge flows through lifecycle layers:

- **L0**: Raw ingestion (text + timestamp + source + trigram hash). Cheap, bulk storage.
- **L1**: First retrieval triggers promotion → embedding + entity extraction → belief creation
- **L2**: Repeated retrieval/corroboration → graph connections, confidence updates, supersession
- **L3**: Established knowledge (high confidence, well-connected, auto-recall eligible)
- **L4**: Synthesized insights (cross-belief synthesis, agent-generated or inferred patterns)

**Promotion triggers**:
- L0 → L1: First retrieval
- L1 → L2: Multiple retrievals OR corroboration from another source
- L2 → L3: High confidence + well-connected + corroborated (≥3 independent sources)
- L3 → L4: Agent synthesis OR engine pattern detection

**Storage**:
```sql
l0_raw_data (
  id UUID PRIMARY KEY,
  content TEXT,  -- Raw text
  fingerprint VARCHAR,  -- Trigram hash for cheap dedup
  source VARCHAR,
  ingested_at TIMESTAMPTZ,
  promoted BOOLEAN DEFAULT FALSE  -- Has this been promoted to L1?
)

CREATE INDEX idx_l0_fingerprint ON l0_raw_data(fingerprint);
CREATE INDEX idx_l0_promoted ON l0_raw_data(promoted);
```

### 1.4 Bounded Memory (LRU with Hard Boundary)

**Status**: Exploring (Confidence: 0.80)

The entire dataset is an LRU cache with a hard boundary (e.g., 10K nodes).

**Eviction score dimensions**:
1. **Recency**: When was this node last retrieved?
2. **Retrieval frequency**: How often has it been pulled?
3. **Connection count**: How well-linked in the graph?
4. **Explicit pins**: User/agent can pin to protect from eviction

```sql
-- Add to entities table
ALTER TABLE entities ADD COLUMN eviction_score FLOAT DEFAULT 0.0;
ALTER TABLE entities ADD COLUMN pinned BOOLEAN DEFAULT FALSE;

-- Eviction score formula
eviction_score = 
  w_recency * days_since_access +
  w_frequency * (1 / access_count) +
  w_connections * (1 / connection_count) -
  (pinned ? 1000 : 0)  -- Pinned nodes have artificially low score
```

**When boundary is hit**: Evict lowest-scoring nodes until below threshold.

**Open Questions**:
- **Q15**: What's the right boundary size? 10K? Adaptive? Per-domain budgets?
- **Q17**: How much protection does link count give?
- **Q19**: Federated beliefs in local LRU or separate tier?
- **Q20**: Cold start symmetry breaking?

### 1.5 Supersession Chains

**Status**: Settled (Confidence: 0.90)

Beliefs are superseded, not deleted. The chain is preserved via sources.

```sql
supersessions (
  id UUID PRIMARY KEY,
  old_triple_id UUID REFERENCES triples(id),
  new_triple_id UUID REFERENCES triples(id),
  reason TEXT,
  superseded_at TIMESTAMPTZ,
  superseded_by VARCHAR  -- Agent or user
)

CREATE INDEX idx_supersessions_old ON supersessions(old_triple_id);
CREATE INDEX idx_supersessions_new ON supersessions(new_triple_id);
```

**Retrieval behavior**: Superseded triples are deprioritized but not hidden (can be queried explicitly for history).

### 1.6 Tensions (Contradictions)

**Status**: Exploring (Confidence: 0.75)

When two triples contradict, create a tension edge.

```sql
tensions (
  id UUID PRIMARY KEY,
  triple_a_id UUID REFERENCES triples(id),
  triple_b_id UUID REFERENCES triples(id),
  tension_type VARCHAR,  -- 'contradiction', 'inconsistency', 'ambiguity'
  detected_at TIMESTAMPTZ,
  resolved BOOLEAN DEFAULT FALSE,
  resolution TEXT
)

CREATE INDEX idx_tensions_unresolved ON tensions(resolved) WHERE NOT resolved;
```

**Retrieval behavior**: Surface tensions alongside results (not hidden).

### 1.7 Clustering / Merge Model

**Status**: Exploring (Confidence: 0.80)

Co-retrieval creates clusters. Clusters are new nodes with members.

```sql
clusters (
  id UUID PRIMARY KEY,
  representative_triple_id UUID REFERENCES triples(id),  -- Most-retrieved member
  created_at TIMESTAMPTZ,
  last_accessed TIMESTAMPTZ,
  access_count INTEGER
)

cluster_members (
  cluster_id UUID REFERENCES clusters(id),
  triple_id UUID REFERENCES triples(id),
  join_score FLOAT  -- Co-retrieval count that led to clustering
)

CREATE INDEX idx_cluster_members_triple ON cluster_members(triple_id);
```

**Behavior**: Cluster inherits dimensional profile of members. Individual members compete for LRU residency; low-retrieval members evict naturally.

**Open Question Q16**: Representative = most-retrieved or most-connected?

---

## 2. Query Requirements

### 2.1 Multi-Dimensional Fusion (Core)

**Status**: Settled (Confidence: 0.90)

A single query searches across **multiple dimensions simultaneously** and fuses results in one pass.

**Dimensions**:
1. **Semantic**: Vector similarity (embedding cosine distance)
2. **Graph**: Traversal distance and relationship type from active entities
3. **Confidence**: Computed dynamically from topology (NOT stored metadata)
   - Source reliability (independent, well-connected sources)
   - Corroboration (independent reasoning paths to triple)
   - Temporal freshness (recency-weighted edge decay)
   - Internal consistency (no contradicting triples in local neighborhood)
   - Domain applicability (centrality within query-relevant subgraph)
4. **Temporal**: Superseded beliefs deprioritized; recent observations boosted
5. **Corroboration**: Number of independent sources/paths (computed from source topology)
6. **Tension**: Contradicted beliefs surfaced WITH contradiction, not hidden

**Scoring function**:
```
score = w_semantic * semantic_sim(query, triple)
      + w_graph * graph_proximity(active_entities, triple)
      + w_confidence * compute_confidence(triple, query_context)  // dynamic!
      + w_recency * recency_decay(sources)
      + w_corroboration * count_independent_paths(triple)
      - w_tension * tension_penalty(triple)
```

**Key**: `compute_confidence()` is a computation over graph topology, NOT a lookup.

**Open Question Q43**: How to tune blend between vector (broad) and graph (precise)?

### 2.2 Hybrid Graph-Vector Retrieval

**Status**: Settled (Confidence: 0.95)

Retrieval uses both vector space and graph space:

1. **Vector search**: Broad, fast, finds neighborhood (e.g., top-50 candidates via HNSW)
2. **Graph traversal**: Precise, structured, walks from candidates via explicit edges
3. **Confidence ranking**: Dynamic scoring from topology
4. **Return**: Assembled context with provenance

**Analogy**: Vectors get you to the right neighborhood. Graph gets you to the right house.

### 2.3 Budget-Bounded Operations

**Status**: Settled (Confidence: 0.90)

Every operation has a **budget** (time, hops, results, tokens). When exhausted, return what you have.

**Budget types**:
- **Time budget**: Max milliseconds (e.g., 50-100ms for context assembly)
- **Hop budget**: Max graph depth (e.g., 3 hops from query nodes)
- **Result budget**: Max nodes in result set (e.g., top-20)
- **Token budget**: Max LLM tokens for warm operations

**Strategies**:
1. **Early termination**: Stop when marginal relevance declines
2. **Tiered retrieval**: Vector-only (fast) → + graph walk (medium) → + full confidence (slow)
3. **Materialized neighborhoods**: Pre-compute k-hop neighborhoods for hot nodes
4. **Bloom filters on paths**: Fast "can I reach B from A?" checks
5. **Adaptive budgets**: Learn optimal budgets per domain/user

**Open Questions**:
- **Q63**: Default budget values per domain?
- **Q64**: Communicate partial results to user?
- **Q65**: Auto-tune budgets from feedback?

### 2.4 Context Assembly

**Status**: Settled (Confidence: 0.95)

Each LLM call receives **freshly assembled context**, not accumulated history.

**Components**:
1. **Immediate thread**: Last 2-3 messages (conversational continuity)
2. **Relevant knowledge**: Beliefs from graph via multi-dim fusion
3. **Active entities**: Focused subgraph of entities live in this conversation
4. **Tensions**: Contradictions between query and existing beliefs
5. **Confidence profile**: Signal uncertainty to LLM

**No overflow**: Context is always relevant, never grows unbounded.

**Open Question Q4**: 2-3 messages fixed, or adaptive based on conversation density?

### 2.5 Working Set (Session State)

**Status**: Exploring (Confidence: 0.70)

Compact representation of active conceptual threads.

**Contains**:
- Active topics (with recency weighting)
- Open questions / unresolved threads
- Decisions made this session
- Entities currently in focus
- Conversation "shape"

**Maintenance**: Between turns, update threads (resolved → drop, active → strengthen, new → add, dormant → weaken).

**Open Questions**:
- **Q5**: Text summary, structured object, or subgraph?
- **Q6**: Who maintains it — engine, LLM, or both?

---

## 3. Computation Requirements

### 3.1 Topology-Derived Embeddings (Critical)

**Status**: Settled (Confidence: 0.95)

Embeddings generated from **graph topology alone**, without external language model.

**Three proven approaches**:

1. **Spectral Embedding**: Eigenvectors of graph Laplacian
   ```
   L = D - A  (Degree - Adjacency)
   Compute eigenvectors → use first k as embeddings
   ```
   - Deterministic, fast (sparse matrix ops), encodes spectral properties
   - Compute cost: O(n²) sparse, ~1-2s for 10K nodes on CPU

2. **Random Walks** (DeepWalk/Node2Vec): Sample graph, compress walk statistics
   ```
   For each node:
     Generate random walks
     Treat as "sentences"
     Train skip-gram on walks
     Embedding = skip-gram vector
   ```
   - Stochastic but converges, ~500ms for 10K nodes
   - Captures local neighborhood similarity

3. **Message Passing** (GNN without learned weights): Iterative neighborhood aggregation
   ```
   Init: random vectors
   For k iterations:
     Aggregate neighbor vectors (mean)
     Update node vector
   Embeddings = final vectors
   ```
   - Deterministic with fixed aggregation, ~200ms for 10K nodes (k=5)
   - Captures k-hop structure

**Bootstrap strategy** (4-phase transition):
1. **Cold start (0-500 triples)**: Use external LLM embeddings (fast, high-quality)
2. **Hybrid (500-2000)**: Blend LLM + topology (shifting weights)
3. **Topology-primary (2000+)**: Mostly topology, LLM fallback for new nodes
4. **Mature (5000+)**: Pure topology, zero API cost

**Open Questions**:
- **Q38**: Which method for which graphs?
- **Q39**: Recompute all on insert, incremental, or batch?
- **Q40**: Minimum density threshold for "good enough"?
- **Q41**: Blend weights during hybrid phase?

### 3.2 Dynamic Confidence Computation

**Status**: Settled (Confidence: 0.90)

Confidence is **NOT stored metadata** — it's computed from graph topology at query time.

**Dimensions computed dynamically**:
1. **Source reliability**: Count independent, well-connected source nodes
2. **Corroboration**: Count independent reasoning paths to triple
3. **Temporal freshness**: Recency-weighted decay from source timestamps
4. **Internal consistency**: Absence of contradicting triples in local neighborhood
5. **Domain applicability**: Centrality within query-relevant subgraph (PageRank-like)

**Why dynamic**: Same triple can have different confidence in different query contexts.

**Example**:
```
Triple: (Rust, good_for, systems_programming)

Query 1: "What about Rust?" 
  → High domain applicability (central to Rust subgraph)
  → Overall confidence: 0.92

Query 2: "What web frameworks?" 
  → Low domain applicability (peripheral to web subgraph)
  → Overall confidence: 0.45
```

**Open Questions**:
- **Q53**: Cache dimension scores or always compute fresh?
- **Q54**: Minimum graph structure for reliable confidence?
- **Q55**: How to blend dimensions into overall score?
- **Q56**: Algorithm for detecting contradicting triples?

### 3.3 Decay Model

**Status**: Exploring (Confidence: 0.70)

Information relevance decays based on **structural properties, not just time**.

**Decay types**:
1. **Conversation decay**: Last 2-3 messages full fidelity → older absorbed into working set → ancient = knowledge in graph only
2. **Belief decay**: Active (frequent retrieval) = stable; dormant (no retrieval) = slow decay; superseded = rapid deprioritization
3. **Entity decay**: Frequent reference = strong; rare reference = fade toward L0
4. **Thread decay**: Active = full weight; dormant = weakened; resolved = compressed; abandoned = fades

**Mechanism**: Decay is deprioritization in fusion query, NOT deletion.

**Decay + eviction**: Heavily decayed + low retrieval frequency + few connections → eviction candidate.

**Open Questions**:
- Decay rate (linear, exponential, stepped)?
- Protection for important but infrequent knowledge (birthdays)?

### 3.4 Lazy Compute

**Status**: Settled (Confidence: 0.90)

Compute is spent **proportional to demonstrated value**, not ingestion volume.

**Rules**:
1. **Ingestion is cheap**: Store raw text + timestamp + trigram hash. No embeddings, no entity extraction.
2. **First retrieval triggers processing**: When query touches L0 → THEN compute embedding, extract entities.
3. **Repeated use triggers refinement**: Graph connections, confidence, supersession only on proven-relevant data.
4. **Expensive ops are demand-driven**: BERT embedding, graph construction, tension detection triggered by retrieval.

**Why**: 100:1 ratio of ingested to useful data. Process only the 1% that matters.

### 3.5 Clustering / Merge via Co-Retrieval

**Status**: Exploring (Confidence: 0.80)

Clustering is **deterministic bookkeeping**, not inference.

**Mechanism**:
1. **Co-retrieval counting**: Two nodes pulled together repeatedly → structural link
2. **Cluster formation**: Nodes with high co-retrieval → cluster node
3. **Representative selection**: Most-retrieved member represents cluster
4. **Individual eviction**: Low-retrieval members compete for LRU, evict naturally
5. **Dimensional inheritance**: Cluster inherits union of member dimensional profiles

**Optional enrichment**: Warm engine can generate synthesis summary for cluster, but system doesn't depend on it.

### 3.6 PageRank / Centrality for Confidence

**Status**: Implied (Confidence: 0.75)

Domain applicability dimension uses PageRank-like centrality within query-relevant subgraph.

**Algorithm**: Standard PageRank with query nodes as seeds:
```
d = 0.85  # damping factor
For each iteration:
  For each node:
    rank[node] = (1-d) + d * sum(rank[neighbor] / out_degree[neighbor])
```

**Query-specific**: Run PageRank on subgraph relevant to query, not entire graph.

---

## 4. API Requirements

### 4.1 Core Operations (Cold Engine)

**Status**: Settled (Confidence: 0.95)

Deterministic operations that work **without inference**:

1. **Insert**: Add triple to graph, compute hash, assign ID
   ```
   insert(subject, relationship, object, source_metadata) -> triple_id
   ```

2. **Link**: Create edge between triples/entities
   ```
   link(entity_a, relationship, entity_b) -> edge_id
   ```

3. **Retrieve**: Multi-dimensional fusion query
   ```
   retrieve(query, budget, weights) -> Vec<Triple>
   ```

4. **Decay**: Time-based confidence reduction, access-based freshness
   ```
   decay(age_threshold, unused_threshold) -> updated_count
   ```

5. **Evict**: LRU-based removal when boundary hit
   ```
   evict(target_count) -> evicted_ids
   ```

6. **Cluster**: Group co-retrieved triples
   ```
   cluster(co_retrieval_threshold) -> cluster_id
   ```

### 4.2 Warm Engine Operations (Inference-Enriched)

**Status**: Settled (Confidence: 0.85)

Optional operations that use LLM reasoning:

1. **Synthesize**: Generate L4 insight from L3 belief cluster
   ```
   synthesize(cluster_id) -> insight_text
   ```

2. **Label**: Assign semantic label to cluster
   ```
   label(cluster_id) -> label_name
   ```

3. **Name Dimensions**: Human-readable labels for structural axes
   ```
   name_dimension(dimension_vector) -> dimension_name
   ```

4. **Curate**: Suggest supersessions, detect tensions, merge candidates
   ```
   curate() -> Vec<CurationSuggestion>
   ```

5. **Recognize Intent**: Understand why query was made
   ```
   recognize_intent(query) -> Intent
   ```

6. **Optimize Context**: Tune which knowledge surfaces for this query
   ```
   optimize_context(query, candidates) -> optimized_candidates
   ```

### 4.3 Tool Mediation

**Status**: Exploring (Confidence: 0.80)

Engine mediates all tool interactions:

1. **Wrap calls with intent**:
   ```
   call_tool_with_intent(tool, params, intent, reasoning, context) -> result
   ```

2. **Extract knowledge from results**:
   ```
   extract_beliefs(tool_result) -> Vec<Triple>
   ```

3. **Preemptive enrichment**:
   ```
   enrich_context_with_artifacts(current_entities) -> enriched_context
   ```

4. **Hygiene triggers**:
   ```
   trigger_hygiene(tool_result) -> Vec<HygieneAction>
   ```

### 4.4 Curation Tools (LLM-Facing)

**Status**: Exploring (Confidence: 0.75)

Explicit LLM tools for substrate curation:

1. **Correct**: Fix belief content or confidence
   ```
   correct(belief_id, new_content OR new_confidence, reason)
   ```

2. **Enrich**: Add context, implications, metadata
   ```
   enrich(belief_id, additional_context)
   ```

3. **Link**: Create explicit connection
   ```
   link_explicit(entity_a, relationship, entity_b)
   ```

4. **Pin**: Protect from decay/eviction
   ```
   pin(belief_id, reason)
   ```

5. **Surface**: Request engine attention on issues
   ```
   surface(attention_type: 'tensions' | 'thin_areas' | 'open_loops')
   ```

### 4.5 Protocol Layer (Deployment Interfaces)

**Status**: Exploring (Confidence: 0.75)

Multiple protocol options for embedding:

1. **MCP (Model Context Protocol)**: Primary interface for agent integration
2. **HTTP REST API**: For web/service deployments
3. **stdio**: For subprocess/sidecar patterns
4. **Native library**: Via napi-rs (Node.js) or PyO3 (Python)
5. **WASM**: For browser-based deployment

---

## 5. Performance Requirements

### 5.1 Context Assembly Latency

**Status**: Settled (Confidence: 0.90)

**Target**: 50-100ms for context assembly (from query to context ready for LLM).

**Breakdown**:
- Vector search (HNSW): 10-20ms for top-k from 10K nodes
- Graph traversal (3 hops): 10-30ms with adjacency lists
- Confidence computation: 10-30ms (topology analysis on subgraph)
- Assembly/serialization: 5-10ms

**Total**: 35-90ms typical, 50-100ms budget with margin.

**Open Question Q13**: Is 50-100ms acceptable, or target lower?

### 5.2 Query Throughput

**Status**: Exploring (Confidence: 0.70)

**Target**: 100+ queries/second on normal hardware (8GB RAM, 4-core CPU).

**Scaling**:
- Single Rust sidecar: 100-200 QPS
- Multiple sidecars per Postgres: 500+ QPS
- Federation across multiple engines: 1000+ QPS

**Open Question Q61**: Can we run multiple Rust sidecars per Postgres for scale?

### 5.3 Graph Size Support

**Status**: Exploring (Confidence: 0.80)

**Target**: Efficiently handle 10K-100K triples in-memory.

**Memory budget**:
- 10K triples: ~100MB in-memory graph + embeddings
- 100K triples: ~1GB in-memory graph + embeddings
- 1M triples: ~10GB (requires disk-backed strategies)

**Bounded memory**: Hard boundary at user-configurable limit (default: 10K nodes).

**Open Question Q62**: Memory budget — 1GB? 8GB? User-configurable?

### 5.4 Startup Time

**Status**: Exploring (Confidence: 0.75)

**Target**: <5 seconds from cold start to ready.

**Breakdown**:
- Load triples from Postgres: 1-2s (bulk SELECT)
- Build adjacency lists: 0.5-1s
- Compute/load embeddings: 1-2s (or load from cache)
- Build HNSW index: 0.5-1s

**Total**: 3-6s typical.

**Open Question Q60**: Cache embeddings in Postgres to speed startup?

### 5.5 Embedding Computation

**Status**: Settled (Confidence: 0.90)

**Target**: Embedding computation must be fast enough for lazy compute.

**Topology embeddings**:
- Spectral: 1-2s for 10K nodes
- Random walks: 500ms for 10K nodes
- Message passing: 200ms for 10K nodes (k=5)

**External embeddings (bootstrap phase)**:
- Quantized BERT via ONNX: 10ms per embedding on CPU
- API call (Ada): 100-500ms (network latency)

### 5.6 Incremental Updates

**Status**: Exploring (Confidence: 0.75)

**Target**: New triple insertion should be fast (sub-millisecond graph update).

**Breakdown**:
- Insert into Postgres: 1-5ms (transactional)
- Update in-memory adjacency lists: <1ms
- Mark embeddings dirty: <1ms
- Recompute embeddings: Lazy (deferred to next retrieval or batch job)

**Embedding update strategy**: Batch recompute (every N inserts or every T seconds) rather than per-insert.

**Open Question Q39**: Recompute all, incremental, or batch?

---

## 6. Security Requirements

### 6.1 Privacy by Architecture

**Status**: Settled (Confidence: 0.90)

Privacy is **structural**, not bolted on.

**Layers**:
- **L0 never shares**: Raw ingestion (emails, messages, documents) is private by definition
- **L1-L2 requires explicit intent**: Share beliefs, not raw data
- **L3-L4 federation candidates**: High-confidence, established knowledge

**Provenance firewall**: Share belief content + confidence, but NOT raw provenance (which conversations produced it).

### 6.2 Data Sovereignty

**Status**: Settled (Confidence: 0.90)

**Principles**:
1. **Engine is always local**: Even in federation, engine runs on user's device
2. **No centralization**: No cloud server that sees everything
3. **User control**: Explicit consent required for any sharing
4. **Revocable**: Can remove trust signal after sharing (can't delete data on remote node, but can revoke endorsement)

### 6.3 Encryption

**Status**: Exploring (Confidence: 0.75)

**Requirements**:
1. **At-rest encryption**: Postgres database encrypted by default (AES-256)
2. **In-transit encryption**: Federation uses DID key exchange + TLS
3. **Key management**: User-controlled keys, stored locally (not in cloud)

**Open Question**: Integration with OS keychain (macOS Keychain, Windows Credential Manager)?

### 6.4 Consent-Gated Sharing

**Status**: Exploring (Confidence: 0.70)

**Sharing intents** (from Valence):
- **know_me**: Private 1:1 sharing
- **work_with_me**: Bounded group sharing
- **learn_from_me**: Cascading (can be reshared)
- **use_this**: Public

**Metadata on triples**:
```sql
ALTER TABLE triples ADD COLUMN sharing_policy VARCHAR DEFAULT 'private';
ALTER TABLE triples ADD COLUMN trust_requirements JSONB;
```

**Enforcement**: Engine checks sharing policy before sending triple to federation layer.

**Open Questions**:
- **Q11**: Transitive provenance in federation?
- **Q12**: Belief revocation after sharing?

### 6.5 GDPR Compliance

**Status**: Exploring (Confidence: 0.70)

**Right to deletion**:
- **Local**: Delete triple + all sources + all provenance
- **Federated**: Send deletion request to nodes that received belief (best-effort)

**Right to access**: Export all triples + sources + metadata as JSON

**Right to portability**: Export in standard format (JSON-LD triples?)

---

## 7. Federation Requirements

### 7.1 Network Architecture

**Status**: Settled (Confidence: 0.90)

**Separation**: Engine (local, embeddable) + Network (protocol for connection).

- **Engine**: Portable knowledge substrate, runs anywhere (sidecar, embedded, WASM, edge)
- **Network**: Protocol for how engines connect, share, corroborate

**Every agent/person gets their own engine instance.** Network enables selective sharing.

### 7.2 What Flows Through Network

**Status**: Settled (Confidence: 0.90)

**Everything decomposes to triples and flows the same way**:

1. **Beliefs**: `(Chris, prefers, composable_architectures)`
2. **Skills**: `(write_file_skill, requires, file_path_parameter)`
3. **Trust scores**: `(Node_A, trusts, Node_B, confidence=0.85)`
4. **Reputation**: `(Node_A, corroboration_rate, 0.73)`
5. **Models**: `(boundary_model_v3, trained_on, 5000_pairs)`
6. **Training data**: `(training_pair_1047, input, "...")`
7. **Compute requests**: `(Node_A, requests, topology_embedding_computation)`

**What does NOT flow**:
- Raw L0 data (emails, transcripts, tool outputs)
- Graph structure (full adjacency lists)
- Working set / session state
- Embeddings (model-dependent, recomputed locally)

### 7.3 Corroboration Protocol

**Status**: Exploring (Confidence: 0.70)

**Flow**:
1. Node A shares belief X
2. Node B checks for similar beliefs (semantic similarity ≥ threshold)
3. **If found**: Corroboration event → both beliefs get confidence boost
4. **If contradicted**: Cross-node tension → both nodes informed
5. **If novel**: Node B stores as "belief from A" at lower initial confidence

### 7.4 Trust Propagation

**Status**: Exploring (Confidence: 0.70)

**Mechanism**:
- Nodes build reputation through accurate sharing
- Corroborated beliefs → boost sharer's reputation
- Contradicted beliefs (proven wrong) → lower reputation
- Reputation affects how future shared beliefs are weighted

**Trust decay**: Friend-of-friend trust decays with distance (2-hop trust < 1-hop trust).

**Reputation metadata**:
```
(Node_A, shared, 147_beliefs)
(Node_A, corroboration_rate, 0.73)  # 73% corroborated
(Node_A, contradiction_rate, 0.05)  # 5% contradicted
```

### 7.5 DIDs (Decentralized Identity)

**Status**: Exploring (Confidence: 0.75)

**Requirements**:
- Every engine has a DID
- Every user/agent has a DID
- DID authentication for all federation operations
- DID-based consent control

**DID methods**: Start with `did:key` (simplest), potentially add `did:web`, `did:peer`.

### 7.6 Discovery

**Status**: Exploring (Confidence: 0.65)

**Mechanisms**:
1. **Manual pairing**: Exchange DIDs directly (secure, no infrastructure)
2. **DHT-based discovery**: Publish capabilities to distributed hash table
3. **Relay servers**: Optional infrastructure for NAT traversal

**Open Question Q75**: Fully p2p or some infrastructure?

### 7.7 Verification & Tamper-Evident Logs

**Status**: Exploring (Confidence: 0.70)

**Requirements**:
1. **Cryptographic proof of provenance**: Sign shared triples with DID key
2. **Tamper-evident logs**: Append-only log of shared beliefs (Merkle tree?)
3. **Verification**: Recipients verify signatures before accepting beliefs

**Open Question**: ZK proofs for compute delegation (Q81)?

### 7.8 Network Flows: Self-Training Loop

**Status**: Exploring (Confidence: 0.75)

**Distributed training**:
1. Engine A trains boundary model on domain X (medical)
2. Engine A shares model metadata: `(boundary_model_medical_v1, accuracy, 0.96)`
3. Engine B requests access (consent required)
4. Engine A shares model weights OR training data
5. Engine B improves own medical domain performance
6. Engine B contributes back (new training data, refined model)

**Network effect**: 1000 engines training in 1000 domains = 1000 specialized models available (with consent).

**Open Questions**:
- **Q77**: Share weights, training data, or both?
- **Q78**: Anonymize training data before sharing?
- **Q80**: Paid access to models?

### 7.9 Spam & Attack Prevention

**Status**: Exploring (Confidence: 0.65)

**Mechanisms**:
1. **Reputation gating**: Only accept beliefs from nodes with reputation ≥ threshold
2. **Rate limiting**: Max beliefs per time window per node
3. **Stake**: Optional economic stake for network participation
4. **Blocklists**: User-controlled blocklists for bad actors

**Open Question Q74**: What combination is sufficient?

---

## 8. Open Implementation Questions

This section captures **unresolved questions** from the design docs that directly impact implementation decisions.

### 8.1 Architecture Decisions

**Q3**: Embedded model distribution?  
Ship ~80MB BERT weights in binary, or download on first use?

**Q25**: Minimum viable cold engine?  
What's the smallest feature set we can ship without warm features?

**Q26**: Warm operations user-configurable?  
Should users control when/how inference runs?

**Q59**: Postgres sync protocol frequency?  
Real-time (every insert), batched (every second), or pull-on-query?

**Q60**: Embedding cache in Postgres?  
Cache topology embeddings in table to speed startup?

**Q61**: Multiple Rust sidecars per Postgres?  
Can we scale with multiple compute instances against one DB?

**Q62**: Memory budget for in-memory graph?  
1GB? 8GB? User-configurable with graceful degradation?

### 8.2 Data Model Details

**Q33**: Triple granularity?  
Single assertion per triple, or complex objects? `(Chris, prefers, X)` vs `(Chris, prefers, {type: arch, property: composable})`?

**Q34**: Temporal validity at which layer?  
Do triples have temporal bounds, or only sources?

**Q35**: Migration from beliefs to triples?  
How to decompose current Valence's 800+ beliefs? Automated, manual, hybrid?

**Q36**: Triple serialization for LLM?  
Show raw triples, summaries, or adaptive?

**Q37**: Summary caching?  
Cache rendered summaries or always compute fresh?

**Q48**: Automatic deduplication threshold?  
Exact hash = definite dupe. What similarity threshold for fuzzy? 0.90? 0.95?

**Q49**: Source independence criteria?  
Different session = independent? Different day? How prevent gaming?

**Q50**: Confidence aggregation from sources?  
Mean? Weighted by source reliability? Median? Max?

**Q51**: Summary scope determination?  
Per-entity? Per-topic? Per-session? Emergent clusters?

### 8.3 Query & Retrieval

**Q4**: Optimal conversation history length?  
2-3 messages fixed, or adaptive based on density?

**Q5**: Working set representation?  
Text summary, structured object, or subgraph?

**Q6**: Who maintains working set?  
Engine, LLM, or collaborative?

**Q13**: Context assembly latency target?  
<50ms? <100ms? What's acceptable?

**Q43**: Vector-graph blend tuning?  
How balance vector (broad) vs graph (precise) in retrieval?

**Q44**: Should vectors override graph?  
If embeddings suggest similarity but no edge exists, create candidate edge?

**Q63**: Default budget values per domain?  
Different budgets for technical vs casual queries?

**Q64**: Communicating partial results?  
Tell users "stopped early" or silent?

**Q65**: Auto-tune budgets from feedback?  
Learn optimal budget per user/domain from engagement?

### 8.4 Computation & Performance

**Q38**: Which topology method for which graphs?  
Spectral for dense? Random walks for sparse? Auto-select?

**Q39**: Dynamic graph embedding updates?  
Recompute all on insert, incremental, or batch?

**Q40**: Minimum graph density threshold?  
How many triples before topology embeddings are "good enough"?

**Q41**: Blend weights for hybrid embeddings?  
Fixed, adaptive based on density, or learned?

**Q53**: Confidence computation caching?  
Cache dimension scores or always compute fresh?

**Q54**: Minimum graph structure for reliable confidence?  
How sparse before topology-based confidence becomes unreliable?

**Q55**: Confidence score blending across dimensions?  
Fixed weights, context-dependent, or learned from feedback?

**Q56**: Contradicting triples detection algorithm?  
Semantic similarity of inverses? Explicit contradiction edges? LLM-based?

**Q57**: Query-relevant subgraph boundary?  
k-hops from query nodes? Similarity threshold? PageRank with query seed?

### 8.5 Bounded Memory & Lifecycle

**Q15**: Right LRU boundary size?  
10K nodes? Adaptive? Per-domain budgets?

**Q16**: Cluster representative selection?  
Most-retrieved or most-connected? Does synthesis become representative?

**Q17**: Eviction of linked nodes?  
How much protection does link-count give? Linear, log, or threshold?

**Q19**: Federation + LRU interaction?  
Federated beliefs in local LRU or separate tier?

**Q20**: Cold start symmetry breaking?  
With no usage history, what breaks equal priority? Ingestion order? Source weight?

**Q22**: Prevent single-inference beliefs as truth?  
Start at LOW confidence, require ≥3 independent corroborations to escalate?

**Q23**: What counts as independent corroboration?  
Different sessions? Different days? How track independence?

**Q24**: Belief deduplication strategy?  
Exact hash, embedding similarity check, or periodic batch consolidation?

### 8.6 Self-Training & Loops

**Q67**: Minimum training data size?  
1000 examples? 5000? Domain-dependent?

**Q68**: Which base model for boundary?  
Phi-3, Llama 3.2 1B, Qwen 2.5 1.5B? Benchmark to decide?

**Q69**: LoRA vs full fine-tuning?  
LoRA faster but less flexible. Which for boundary models?

**Q70**: Sharing trained boundary models?  
Privacy implications — models may encode training data.

**Q71**: Separate models for decomp vs recomp?  
One model for both directions, or two specialized?

**Q83**: Loop maturity detection?  
Auto-detect when each loop is mature enough to transition?

**Q84**: Loop transition: automatic or manual?  
Trust system to auto-transition or user control?

### 8.7 Federation & Network

**Q11**: Transitive provenance in federation?  
How much of the provenance chain is visible when beliefs flow through network?

**Q12**: Belief revocation after sharing?  
Can you "unshare"? Revocation = remove trust signal, not delete data?

**Q72**: Engine-first or engine+network together?  
Ship embeddable engine first, add network later? Or build both?

**Q73**: Minimum viable federation protocol?  
DID exchange + triple sync + corroboration? Or simpler?

**Q74**: Spam/attack prevention?  
Reputation + stake? Rate limiting? Proof-of-work?

**Q75**: P2P infrastructure requirements?  
Fully p2p (DHT, hole-punching) or some infrastructure (relays)?

**Q76**: Reference network or protocol only?  
Operate a reference Valence network, or just publish protocol?

**Q77**: Model sharing: weights, data, or both?  
Weights = large. Data = requires training. Both?

**Q78**: Training data anonymization?  
Remove identifying entities before sharing?

**Q81**: Trustless compute delegation?  
ZK proofs for verifiable computation?

---

## Appendix A: Technology Stack Recommendations

Based on the design docs, here's the implied/recommended tech stack:

### Core Engine (Rust)
- **Language**: Rust (memory safety, performance, embeddable)
- **Graph library**: petgraph (mature, well-tested)
- **Vector search**: HNSW implementation (instant-distance or custom)
- **Embeddings**: 
  - Bootstrap: ONNX Runtime (`ort` crate) for quantized BERT
  - Mature: Topology-derived (pure Rust, no ML dependencies)
- **Storage backend**: PostgreSQL with Rust connector (`tokio-postgres`)
- **Serialization**: serde + bincode for efficient binary formats

### Database (PostgreSQL)
- **Version**: PostgreSQL 14+ (for better JSON support)
- **Extensions**:
  - pgvector (vector storage during bootstrap phase)
  - pg_trgm (trigram indexes for L0 cheap dedup)
- **Indexes**: 
  - B-tree on triple components (SPO/POS/OSP)
  - GiST/GIN for full-text search
  - Optional: pgvector IVFFLAT for bootstrap embeddings

### Embeddings (Bootstrap Phase)
- **Model**: all-MiniLM-L6-v2 (22M params, 80MB quantized, 10ms CPU)
- **Runtime**: ONNX Runtime via `ort` crate
- **Fallback**: API to OpenAI Ada or Cohere (during cold start if local fails)

### Protocol Layer
- **Primary**: Model Context Protocol (MCP) for agent integration
- **Secondary**: HTTP REST API (via axum or actix-web)
- **Bindings**: 
  - Node.js: napi-rs
  - Python: PyO3
  - WASM: wasm-bindgen

### Federation (Future)
- **Identity**: DIDs via did-key (simplest, no infrastructure)
- **Transport**: libp2p (mature p2p networking library in Rust)
- **Encryption**: TLS 1.3 + DID-based key exchange
- **Discovery**: Manual pairing initially, DHT later (Kademlia via libp2p)

### Deployment
- **Primary**: Rust binary as sidecar process
- **Alternative**: Embedded library (via napi-rs or PyO3)
- **Future**: Standalone binary with embedded Postgres (via libpq)

---

## Appendix B: Implementation Phases

Based on design consensus, here's a suggested build order:

### Phase 0: Foundation (Weeks 1-2)
1. PostgreSQL schema (triples, sources, entities)
2. Rust sidecar scaffold (petgraph integration)
3. Basic insert/retrieve (no embeddings yet, just graph)
4. Prove Postgres ↔ Rust sync works

### Phase 1: Cold Engine MVP (Weeks 3-6)
1. Triple decomposition (simple heuristics, no LLM yet)
2. Graph construction from triples
3. Spectral/random walk topology embeddings
4. HNSW index for vector search
5. Hybrid retrieval (vector + graph)
6. Basic fusion query (semantic + graph proximity)

**Milestone**: Retrieve knowledge from triples using topology embeddings.

### Phase 2: Warm Boundary (Weeks 7-10)
1. Integrate ONNX Runtime for bootstrap embeddings
2. LLM decomposition via MCP (Claude/GPT-4)
3. Context assembly with fusion
4. Store decomposition training pairs
5. Hybrid embeddings (LLM + topology, blended)

**Milestone**: LLM can store and retrieve knowledge using the engine.

### Phase 3: Lifecycle & Hygiene (Weeks 11-14)
1. L0 raw data storage
2. Lazy promotion (L0 → L1 on first retrieval)
3. Source tracking and provenance
4. Deduplication (content-hash + similarity)
5. Corroboration tracking and confidence updates
6. Supersession chains

**Milestone**: Engine handles redundant beliefs cleanly.

### Phase 4: Bounded Memory (Weeks 15-18)
1. LRU eviction scoring
2. Decay model (recency, frequency, connections)
3. Explicit pins
4. Hard boundary enforcement
5. Clustering / merge model
6. Hygiene triggers

**Milestone**: Engine self-maintains within fixed memory boundary.

### Phase 5: Self-Training (Weeks 19-24)
1. Collect training data from LLM decompositions
2. Fine-tune Phi-3 or Llama 3.2 1B on training pairs
3. Local model inference (decomposition)
4. Fallback logic (local → LLM when uncertain)
5. Quality monitoring and auto-tuning

**Milestone**: Engine trains its own boundary, reduces LLM costs.

### Phase 6: Federation (Months 6-9)
1. DID integration (did-key)
2. Manual node pairing
3. Triple sharing protocol
4. Corroboration detection
5. Trust scoring and reputation
6. Network flows (beliefs, skills, models)

**Milestone**: Two engines share knowledge and corroborate.

### Phase 7: Scale & Polish (Months 10-12)
1. Multi-sidecar support
2. Distributed budgets
3. DHT discovery
4. Production monitoring
5. Documentation and examples
6. Public protocol specification

**Milestone**: Production-ready, scalable, federable engine.

---

## Appendix C: References to Concept Docs

For each requirement above, here are the primary concept docs that informed it:

- **Triples**: `triples-atomic.md`, `three-layer-architecture.md`
- **Progressive Lifecycle**: `knowledge-lifecycle.md`, `progressive-summarization.md`
- **Bounded Memory**: `bounded-memory.md`, `decay-model.md`
- **Multi-Dim Fusion**: `multi-dim-fusion.md`, `emergent-dimensions.md`
- **Context Assembly**: `context-assembly.md`, `working-set.md`
- **Topology Embeddings**: `topology-embeddings.md`, `graph-vector-duality.md`, `knowledge-loop.md`
- **Confidence**: `emergent-dimensions.md`, `epistemics-native.md`
- **Cold/Warm Split**: `cold-warm-split.md`, `deterministic-core.md`
- **Budget-Bounded Ops**: `budget-bounded-ops.md`, `graceful-degradation.md`
- **Lazy Compute**: `lazy-compute.md`, `value-per-token.md`
- **Self-Training**: `self-training-boundary.md`, `self-closing-loops.md`
- **PostgreSQL + Rust**: `postgres-rust-architecture.md`, `rust-engine.md`
- **Privacy**: `privacy-sovereignty.md`, `intermediary.md`
- **Federation**: `federation.md`, `engine-network-product.md`, `network-flows.md`
- **Stigmergy**: `stigmergy.md`, `inference-training-loop.md`
- **Curation**: `curation.md`, `tool-mediation.md`
- **Merge Model**: `merge-model.md`
- **Open Questions**: `docs/questions/OPEN.md`

---

## Document Status

**Created**: 2026-02-16  
**Source**: 36 concept docs + OPEN.md from valence-engine design repo  
**Next Steps**: 
1. Review and validate requirements with design team
2. Prioritize open questions for resolution
3. Begin Phase 0 implementation (foundation)
4. Refine requirements based on implementation learnings

**Living Document**: This should be updated as design evolves and open questions get resolved.
