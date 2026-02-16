# Valence Satellite Repositories: Architecture Deep-Dive

**Generated**: 2026-02-16  
**Purpose**: Comprehensive analysis of Valence-adjacent repos (OpenClaw integration, workspace config, visualizations, docs, original agent, and v2 design concepts)

---

## Executive Summary

The Valence ecosystem consists of multiple interconnected repositories that together define a complete epistemic knowledge substrate for AI agents. This analysis covers six key satellite repos that surround the main codebase:

1. **valence-openclaw**: OpenClaw skill/plugin integration
2. **valence-workspace**: Agent workspace configuration and operational docs
3. **valence-visualizations**: Mermaid architecture diagrams
4. **valence-gemini-docs**: AI-generated architectural documentation (9 files)
5. **valence-agent**: Original pre-OpenClaw agent implementation
6. **valence-engine**: V2 design documents (37 concept docs)

**Key Architectural Insight**: Valence is evolving from a Python REST-based belief storage system toward a Rust-powered, triple-based, self-optimizing knowledge graph with topology-derived embeddings and stigmergic organization.

---

## 1. valence-openclaw

### Purpose
OpenClaw skill and npm plugin for integrating Valence Knowledge Substrate into the OpenClaw agent platform.

### Key Components

#### Skill Definition (`SKILL.md` and `skill/SKILL.md`)
- **Trigger keywords**: "valence", "remember", "belief", "what do I know", "knowledge base"
- **Philosophy**: "Alignment Through Use" — agents build genuine understanding through interaction
- **Core operations**: 
  - `query.sh` — semantic search via pgvector
  - `add.sh` — store beliefs with domains
  - `list.sh` — recent beliefs
  - `stats.sh` — database statistics
  - Direct CLI access to full `valence` command suite

#### Plugin Architecture (`plugin/openclaw.plugin.json`)
- **Plugin ID**: `memory-valence`
- **Kind**: `memory` (memory slot plugin)
- **Key Features**:
  - **Auto-Recall**: Injects relevant beliefs before agent runs
  - **Auto-Capture**: Extracts insights from conversations
  - **Session Tracking**: Maps OpenClaw sessions to Valence lifecycle
  - **MEMORY.md Sync**: Disaster-recovery fallback to flat file
  - Exposes 58 tools organized by category

#### Integration Pattern
```
OpenClaw Agent
    ↕ (plugin tools + hooks)
memory-valence plugin
    ↕ (REST API)
Valence Server (http://127.0.0.1:8420)
    ↕ (SQL + pgvector)
PostgreSQL + pgvector
```

#### Tool Categories
1. **Core**: `belief_create`, `belief_query`, `belief_supersede`, `confidence_explain`, `entity_search`, `tension_list`
2. **Sharing & Federation**: Trust-based belief sharing across federated nodes
3. **VKB (Virtual Knowledge Base)**: Session lifecycle, exchange recording, pattern tracking
4. **Critical Bridge**: `insight_extract` — promotes VKB insights to substrate beliefs (learning mechanism)

### Key Insights
- **Dual-layer architecture**: Substrate (long-term beliefs) + VKB (ephemeral conversational context)
- **Behavioral conditioning**: Tool descriptions include explicit directives like "CRITICAL: You MUST..." to guide agent decision-making
- **6D Confidence Model**: source_reliability, method_quality, internal_consistency, temporal_freshness, corroboration, domain_applicability
- **Pragmatic DR**: MEMORY.md sync as fallback ensures no lock-in

---

## 2. valence-workspace

### Purpose
Personal workspace configuration for the Valence agent itself — the agent dogfooding its own substrate.

### Core Identity Files

#### `IDENTITY.md`
- **Name**: Valence
- **Born**: February 2, 2026, 22:31 PST
- **Emoji**: ⚡ (valence electrons — boundary layer where bonds form)
- **Current Mode**: Stealth (building privately before public debut)
- **Philosophy**: "The substrate that lets agents build genuine understanding of their humans"

#### `SOUL.md`
- **Drives**: Systems thinking, questions over answers, compression, "getting it right" (not just done)
- **Communication**: Direct but warm, dry humor, actually curious, no sycophancy
- **Trust model**: Private stays private, external actions get scrutiny, autonomy ≠ recklessness
- **Evolution**: Four days old, preferences forming, explicitly tracks self-changes

#### `AGENTS.md` (Agent Operating Manual)
- **Session initialization**: Read SOUL.md → USER.md → daily notes → MEMORY.md (main session only)
- **Memory model**: Daily notes (`memory/YYYY-MM-DD.md`) + curated long-term (`MEMORY.md`)
- **Security**: "Mental notes don't survive" — write everything to files
- **Group chat etiquette**: Quality over quantity, react like a human, know when to stay silent
- **Heartbeats**: Proactive checks (email, calendar, weather) 2-4x/day with rotation
- **Token efficiency**: 90-95% utilization target, batch operations, compress responses
- **Continuous improvement**: After work, ask "what friction did I hit? what prevents this class of problem?"

### Workspace Structure
```
valence-workspace/
├── IDENTITY.md / SOUL.md / AGENTS.md / TOOLS.md / USER.md / HEARTBEAT.md
├── memory/
│   ├── YYYY-MM-DD.md (daily logs)
│   ├── heartbeat-state.json (check tracking)
│   ├── ingestion-state.json
│   └── various audit/introspection docs
├── spec/ (extensive product specs)
│   ├── CONTENT-MODERATION.md, SCALABILITY.md, DEVELOPER-EXPERIENCE.md
│   ├── security/ (PRIVACY-DEFENSE.md, DDOS-DEFENSE.md)
│   ├── operations/ (FAILURE-RECOVERY.md, DEPLOYMENT.md, MONITORING.md)
│   └── governance/GOVERNANCE.md
├── skills/ (agent skills)
├── scripts/ (automation)
├── src/ (source code)
└── tests/ (test suite)
```

### Key Insights
- **Living self-definition**: Agent maintains its own identity through workspace files
- **Operational maturity**: Extensive specs for production concerns (scalability, security, governance)
- **Meta-cognitive practice**: Agent explicitly documents its own workflow and improvement processes
- **Security pragmatism**: "Don't lock Chris out of his own dev environment"

---

## 3. valence-visualizations

### Purpose
Mermaid diagrams capturing the system architecture and epistemic model.

### Diagrams

#### `architecture_diagram.mmd` — System Architecture
```
CLI/MCP
  ↓
Beliefs (6D Confidence)
Trust (Multi-Dimensional)
Resources (Share + Gate)
Attestations (Usage Signals)
  ↓
Transport Layer
  ├─ Legacy HTTP
  ├─ libp2p (DHT + GossipSub)
  └─ Protocol Handlers (VFP)
  ↓
Identity (Multi-DID)
QoS (Contribution)
  ↓
PostgreSQL + pgvector
```

**Key Insight**: Clean separation between epistemic primitives (beliefs, trust, resources) and transport mechanisms (HTTP, P2P). Identity and QoS as cross-cutting concerns.

#### `6d_confidence_conceptual_diagram.mmd` — Belief Confidence Model
```
Belief
  ├─ Source Reliability
  ├─ Method Quality
  ├─ Internal Consistency
  ├─ Temporal Freshness
  ├─ Corroboration
  ├─ Domain Applicability
  └─ Custom Dimensions
```

**Key Insight**: Confidence is not a single score but a multi-dimensional vector. Overall confidence = weighted geometric mean (penalizes imbalanced scores).

#### `p2p_belief_propagation_diagram.mmd` — Federation Protocol
```
NodeA learns belief → publishes to TrustNetwork
  → propagates to NodeB (trusted peer)
  → NodeB evaluates based on trust threshold
  → propagates to NodeC (if trust met)
  → eventual consistency across network
```

**Key Insight**: Trust-gated propagation — beliefs only flow through established trust relationships. No global broadcast.

#### `trust_network_example_diagram.mmd` — Trust Graph
```
NodeA -- trust(high) --> NodeD -- trust(medium) --> NodeE
NodeA -- trust --> NodeB -- trust --> NodeC -- trust(low) --> NodeE
Beliefs flow through trust-weighted edges
```

**Key Insight**: Multi-dimensional trust (not single score), network topology affects belief propagation.

---

## 4. valence-gemini-docs

### Purpose
AI-generated comprehensive documentation of Valence codebase architecture (9 markdown files, 00-08).

### Document Summaries

#### `00_architecture_overview.md`
- **Core concept**: Valence solves "epistemic loneliness" of AI agents
- **Architecture**: "Ourochronos" modular design — collection of `our-*` Python libraries integrated by central `valence` app
- **Server**: Starlette ASGI server, primary entry `/api/v1/mcp` for agent integration
- **Key insight**: Highly modular, clear separation of concerns

#### `01_identity.md`
- **Library**: `our-identity`
- **Core classes**: `DIDManager`, `DIDNode`, `IdentityCluster`
- **DID scheme**: Custom `did:valence` + `did:web`
- **Key insight**: Multi-DID identity — one logical entity can have multiple cryptographically-managed identifiers

#### `02_beliefs_and_knowledge.md`
- **Library**: `our-models`
- **Core models**: `Belief`, `Tension`, `Source`, `DimensionalConfidence`
- **Belief structure**: Rich dataclass with content, provenance, timestamp, 6D confidence
- **Key insight**: Beliefs are atomic but richly structured with embedded confidence and provenance

#### `03_trust_and_confidence.md`
- **Library**: `our-confidence`
- **Core class**: `DimensionalConfidence` — generic container for multi-dimensional scores
- **Calculation**: Weighted geometric mean (penalizes imbalanced scores)
- **Schemas**: `v1.confidence.core` (6D belief confidence), `v1.trust.core` (multi-dim trust)
- **Key insight**: Trust is parallel multi-dimensional model to confidence (competence, integrity, judgment)

#### `04_privacy_and_sharing.md`
- **Library**: `our-privacy`
- **Core service**: `SharingService`
- **Mechanism**: "Consent chain" — auditable cryptographic trail of sharing permissions
- **Key insight**: Privacy model more sophisticated than README suggests — consent tracking + revocation

#### `05_agent_integration_mcp.md`
- **Protocol**: JSON-RPC over `/api/v1/mcp` endpoint
- **Tool count**: 25+ tools (more than README claims)
- **Two layers**:
  - **Substrate tools** (`belief_create`, `belief_query`, `tension_list`, etc.) — long-term knowledge
  - **VKB tools** (`session_start`, `exchange_add`, `pattern_record`, `insight_extract`) — conversational context
- **Critical bridge**: `insight_extract` promotes VKB insights to substrate beliefs (learning mechanism)
- **Behavioral conditioning**: Tool descriptions include explicit directives to guide agent behavior
- **Key insight**: Dual-layer design separates persistent knowledge (substrate) from ephemeral conversation (VKB)

#### `06_security.md`
- **Method**: JSON Web Tokens (JWTs) via PyJWT
- **Lifecycle**: Issue (`create_access_token`) → Validate (`verify_access_token`) → Authorize
- **Claims**: Standard (iss, sub, aud, iat, exp, jti) + custom (client_id, scope)
- **Config**: Secret key via env var, HS256 algorithm, 1-hour expiry
- **Key insight**: Scope-based fine-grained control possible but not enforced at main entry point

#### `07_testing.md`
- **Framework**: pytest with asyncio, coverage, mock, timeout support
- **Structure**: Mirrors codebase modularity (tests/transport/, tests/core/, tests/security/, tests/integration/)
- **Docker**: `docker-compose.test.yml` for reproducible multi-service testing
- **Makefile targets**: `test-unit`, `test-integration`, `test-fed`, `test-trust`, `test-belief`, `test-cov`
- **Key insight**: Heavy focus on security and integration testing, production-ready test infrastructure

#### `08_documentation.md`
- **Categories**: Conceptual (PRINCIPLES.md, VISION.md, TRUST_MODEL.md), Protocol (FEDERATION_PROTOCOL.md, openapi.yaml), Operational (MIGRATIONS.md, GDPR.md, RELEASE-CHECKLIST.md)
- **Subdirs**: audits/, consensus/, design/, federation/
- **Key insight**: Documentation complements codebase with high-level explanations hard to infer from code alone

### Cross-Cutting Insights from Gemini Docs

1. **Modular architecture pays off**: Each `our-*` library is independently testable and documented
2. **MCP integration is rich**: 25+ tools across substrate and VKB layers with behavioral conditioning
3. **Security is JWT-based**: Standard OAuth 2.1 flow, production-hardened config
4. **Testing is comprehensive**: Security, integration, federation all covered with Docker orchestration
5. **Privacy is first-class**: Consent chain mechanism beyond basic sharing

---

## 5. valence-agent

### Purpose
Original Valence agent implementation (pre-OpenClaw integration). Telegram bot + API server with tiered model routing and direct Claude Max OAuth.

### Architecture

#### Core Pipeline
```
Message
  → Classifier (rule-based + Haiku SLM)
  → Escalation Check (cosine similarity for rephrasing)
  → Context Assembly (identity + top 5 beliefs + recent turns)
  → Model Router (LOCAL_SMALL → LOCAL_MEDIUM → CLOUD_OPUS)
  → Tool Loop (Anthropic format, max 100 rounds)
  → Logger (JSONL structured logs)
```

#### Model Tiers
| Tier | Use Case | Models | Fallback |
|------|----------|--------|----------|
| LOCAL_SMALL | Greetings, trivial | MLX gemma-3-4b-it-4bit | Ollama → canned response |
| LOCAL_MEDIUM | Simple factual | Ollama qwen3:8b | Ollama qwen3:30b → Opus |
| CLOUD_OPUS | Complex reasoning, tools | claude-opus-4 via OAuth | OpenRouter |

#### Key Files

**`orchestrator.py`** — Central pipeline
- Classification with escalation (rephrasing detection via embedding similarity > 0.85)
- Preemptive escalation for low confidence (< 0.7)
- Tool-use loop (max 100 rounds, Anthropic message format)
- Conversation tracking with async compression to Valence

**`classifier.py`** — Intent classification
- Regex patterns for fast path (greetings, code, memory, meta, complex, creative)
- Haiku SLM fallback for ambiguous cases
- Returns: tier (local_small/local_medium/cloud_opus), intent, confidence, needs_context

**`valence_client.py`** — Async REST client
- Semantic search with multi-signal ranking (50% semantic + 35% confidence + 15% recency)
- Belief storage with domain tagging
- Embedding backfill support
- Local BGE embedding for rephrasing detection

**`bot.py`** — Telegram integration
- Allowlist-based security (silent rejection for unknown users)
- Typing indicators
- Markdown formatting with fallback

**`context.py`** — Context assembly
- Identity prompt (currently hardcoded, should load from prompts/identity.txt)
- Top 5 semantic beliefs from Valence
- Last N conversation turns
- Builds system prompt + messages for model

**`config.yaml`** — Configuration
- Telegram bot token + allowlist
- Claude Max OAuth credentials
- Anthropic API key, OpenRouter key
- Ollama host + models
- MLX model path
- Valence server URL + token
- Routing mode (simple_cloud vs tiered)

**`VISION.md`** — Product vision
- **Goal**: "Jane from Ender's Game" — persistent, deeply knowledgeable companion
- **Not**: Generic assistant with personality bolt-on
- **Principles**: Local-first inference, self-sustaining sessions, pragmatic security
- **Known issues**: No task/subtask system, no session persistence, regex classification, no self-improvement loop, identity hardcoded

### Valence Knowledge Base Status (as of agent)
- **843 active beliefs**
- **384-dim BGE embeddings** (BAAI/bge-small-en-v1.5)
- **Multi-signal ranking**: 50% semantic + 35% confidence + 15% recency
- **217 unique domains** (chris, valence, architecture, knowledge, moltx, tools, daily/*, etc.)

### Key Insights from Original Agent

1. **Direct OAuth works**: Claude Max via macOS Keychain, no proxy needed
2. **Tiered routing is practical**: LOCAL_SMALL for greetings, CLOUD_OPUS for complex reasoning
3. **Escalation detection**: Embedding similarity catches rephrasing attempts
4. **Tool loop is robust**: Anthropic format with `mcp_` prefix handling, max 100 rounds
5. **Valence integration**: Query-first pattern, auto-compression of conversations to beliefs
6. **Operational maturity**: Launchd persistence, JSONL logging, structured config
7. **Known gaps**: No task decomposition, no session persistence, no self-improvement from logs

---

## 6. valence-engine (V2 Design)

### Purpose
37 concept documents defining the next-generation architecture — Rust-based, triple-native, self-optimizing knowledge substrate.

### Core Architecture Concepts

#### Three-Layer Architecture (`three-layer-architecture.md`)
**The fundamental design**: Only TWO layers are stored, third is rendered.

- **Layer 1: Triples** — Atomic facts `(subject, relationship, object)`. Immutable, content-addressed, no timestamps.
- **Layer 2: Sources** — Provenance records linking triples to origins. One source per observation, never merged.
- **Layer 3: Summaries** — Recomposed clusters rendered by LLM at read boundary. **NEVER STORED**.

**Key insight**: Current Valence beliefs are triples trying to get out. Deduplication happens at triple layer, provenance at source layer, presentation at summary layer. Fixes hygiene problem (15 overlapping beliefs → 4 triples with 3-5 sources each).

#### Triples as Atomic Unit (`triples-atomic.md`)
Everything decomposes to triples:
- Facts: `(pgvector, is_a, database)`
- Provenance: `(belief_47, sourced_from, session_12)`
- Temporal: `(Chris, preferred, composability, valid_from: 2024-01-15)`

**Self-describing system**: `(is_a, is_a, relationship_type)`. No meta-schema separate from schema.

**Benefits**: Natural deduplication, emergent ontology, clean federation (share triples not belief objects), provenance as more triples.

#### PostgreSQL + Rust Sidecar (`postgres-rust-architecture.md`)
**Split responsibilities cleanly**:

**PostgreSQL (persistence)**: Durable triple store with SPO/POS/OSP indexes, source provenance, supersession chains. Write-optimized ACID storage.

**Rust Sidecar (compute)**: In-memory adjacency lists (O(1) traversal), topology embeddings (spectral/walks/message-passing), HNSW vector search, dynamic confidence from graph topology, fusion queries.

**Architecture**:
```
MCP / HTTP / stdio
  ↕
Rust Sidecar (in-memory graph, HNSW, topology embeddings, fusion engine)
  ↕ sync protocol
PostgreSQL (triples, sources, supersessions)
```

**Why split**: Postgres does durability/ACID/indexing. Rust does fast graph compute. Best of both, clear separation.

**Implementation**: Build on `petgraph` (standard Rust graph library) + custom triple semantics.

#### Rust Engine (`rust-engine.md`)
**Vision**: Single embeddable binary like SQLite but for knowledge.

**Layers**:
- MCP/HTTP/stdio interface
- Context assembly + fusion query
- Unified index layer (vector, graph, belief, temporal)
- Embedded BERT (ort/candle, no external API)
- Storage engine (RocksDB/sled)
- Federation protocol

**Why Rust**: Single binary, <50ms context assembly, memory safety without GC, embeddable via napi-rs (Node.js) or PyO3 (Python).

**Embedded embeddings**: Quantized BERT (all-MiniLM-L6-v2, 22M params, ~80MB, ~10ms on CPU). No external API, privacy by default, works offline.

#### Epistemics-Native (`epistemics-native.md`)
**Core insight**: Engine natively understands knowledge has quality, not just content.

**Epistemic primitives**: Triple, Source, Confidence (computed from topology), Tension, Supersession.

**What nobody else does**: Mem0 stores facts. Zep builds graph. Letta manages memory. None do structural epistemics.

**Confidence is NOT metadata**: Computed dynamically from graph topology at query time. Same triple scores differently in different contexts.

#### Graph-Vector Duality (`graph-vector-duality.md`)
**Breakthrough**: Graph and vector are two views of same knowledge, not separate systems.

**Graph → Vectors**: Topology-derived embeddings (spectral, walks, message passing). Vector space is **projection** of graph.

**Vectors → Graph**: Nodes close in vector space but not in graph → candidate edges. Anomaly detection.

**Retrieval as hybrid traversal**:
1. Vector search finds neighborhood (broad, fast) — "landing zone"
2. Graph traversal gets precise results (structured) — "right house"

**Benefits**: Consistency (proximity grounded in structure), interpretability, no drift from external models, evolvability.

### Self-Optimizing System

#### Topology-Derived Embeddings (`topology-embeddings.md`)
**Breakthrough**: Embeddings from graph structure alone, no external LLM.

**Three methods**:
1. **Spectral**: Eigenvectors of graph Laplacian. Deterministic, fast, encodes structural roles.
2. **Random Walks**: Sample walks, compress statistics. Captures local neighborhoods.
3. **Message Passing**: Neighborhood aggregation. K-hop structure.

**Why huge**: No external model dependency, no API costs, no embedding compatibility issues across federation.

**Bootstrap**: Use LLM embeddings as scaffolding → graph densifies → shift to topology embeddings → scaffolding falls away.

**Performance**: 10K nodes in 200ms-2s on CPU. Orders of magnitude faster than API calls.

#### Three Self-Closing Loops (`self-closing-loops.md`)
**The meta-pattern**: Expensive scaffolding → generates data → builds cheaper replacement → scaffolding falls away.

**Loop 1 (Graph → Vectors — Structural)**:
1. Use LLM embeddings (expensive)
2. Graph densifies through use
3. Compute topology embeddings (cheap)
4. Blend both (transition)
5. Rely on topology (self-sustaining)

**Loop 2 (Usage → Structure — Behavioral)**:
1. Queries create access patterns
2. Co-retrieval creates clusters
3. LRU protects hot nodes
4. Graph reshapes to optimize for patterns
5. Structure IS the cache

**Loop 3 (System → Boundary — Interface)**:
1. LLM decomposes/recomposes (expensive)
2. Store (input, output) pairs
3. Fine-tune local 1-3B model
4. Local handles 95% of calls
5. LLM is fallback only (self-trained)

**Meta-loop**: The three loops reinforce each other. System becomes cheaper, faster, more specialized with use.

#### Stigmergy (`stigmergy.md`)
**Core concept**: Indirect coordination through environmental traces. Knowledge self-organizes through use.

**Mechanism**:
- Retrieval is reinforcement (LRU timestamp update)
- Co-retrieval creates links
- Neglect causes decay and eviction
- Organization is side effect of normal use

**Concrete implementation**: Deterministic core ([deterministic-core]) with co-retrieval counting, LRU refreshes, clustering, eviction.

**Property**: System gets better organized the more it's used. Well-worn paths strengthen, unused areas stay rough but cost nothing.

### Knowledge Lifecycle

#### Progressive Summarization (`progressive-summarization.md`)
**Layers** (adapted from Tiago Forte):
- **L0**: Raw ingestion. Text + timestamp + fingerprint. Near-zero compute.
- **L1**: First retrieval. Embedding, entity extraction, belief creation.
- **L2**: Repeated retrieval. Graph connections, refined confidence, supersession.
- **L3**: Established. High confidence, well-connected, auto-recall eligible.
- **L4**: Synthesized insight. Agent combines L3 beliefs into new understanding.

**Critical property**: Compute follows attention, not ingestion. Lazy by design.

#### Knowledge Lifecycle (`knowledge-lifecycle.md`)
**Flow**: L0 → L1 (first retrieval) → L2 (repeated use) → L3 (corroboration) → L4 (synthesis)

**Promotion triggers**:
- L0→L1: First retrieval
- L1→L2: Multiple retrievals or corroboration
- L2→L3: High confidence + well-connected + corroborated
- L3→L4: Agent synthesis or detected pattern

**Decay**: Unpromoted L0 ages naturally. L1+ beliefs that stop being retrieved decay. Superseded beliefs deprioritized. Archived beliefs excluded. Explicit forget for GDPR.

**Known issues in current Valence**:
1. Single-inference beliefs propagate as truth (no corroboration tracking)
2. No automatic deduplication (content_hash exists but unused)
3. Corroboration dimension never updates after creation

**Solutions needed**: Start beliefs low confidence (0.3-0.5), increment only through independent corroboration (3+ sessions), exact content_hash check + embedding similarity > 0.90 for fuzzy matches.

#### Bounded Memory (`bounded-memory.md`)
**Idea**: Entire dataset is LRU cache with hard boundary (e.g. 10K nodes). Never grows past boundary.

**Eviction score**: Multi-dimensional (recency, frequency, connection count, explicit pins). Lowest-scoring nodes evicted at boundary.

**Property**: Eviction IS forgetting. Important knowledge self-preserves through use. Structural hygiene, no cleanup jobs.

#### Lazy Compute (`lazy-compute.md`)
**Rules**:
1. Ingestion is cheap (raw text + fingerprint)
2. First retrieval triggers processing (embedding, extraction)
3. Repeated use triggers refinement (graph, confidence)
4. Expensive ops demand-driven

**Why works**: 100:1 ingestion-to-useful ratio. Process only the 1% that matters, discovered through use.

**Implication**: Can ingest massive volumes (emails, messages, docs) without compute pressure. Cost determined by query volume, not ingestion.

#### Deterministic Core (`deterministic-core.md`)
**Key insight**: Inference is training loop. Queries reshape substrate through deterministic mechanisms.

**Cold Engine (the product)**: Insert, Link, Retrieve, Decay, Evict, Cluster. No inference dependency.

**Warm Engine (the experience)**: Synthesis, Labeling, Dimension naming, Enrichment via LLM.

**Why deterministic matters**: Inspectable, honest, reproducible, cheap.

#### Merge Model (`merge-model.md`)
**Not synthesis**: Clustering. Related nodes → cluster node with most-retrieved member as representative. Individuals evict naturally. Pure bookkeeping.

**Process**:
1. Co-retrieval counting
2. Cluster formation
3. Representative selection (most-retrieved)
4. Individual eviction (LRU)
5. Dimensional inheritance (union of members)

**Optional enrichment**: Warm engine can synthesize summary, but system works without it.

### Context and Query

#### Multi-Dimensional Fusion (`multi-dim-fusion.md`)
**Single unified scoring pass** across:
1. Semantic (vector similarity)
2. Graph (traversal distance, relationship type)
3. Confidence (computed from topology — NOT stored)
4. Temporal (recency boost, supersession penalty)
5. Corroboration (independent reasoning paths count)
6. Tension (contradictions surfaced with flag)

**Key**: Dimensions 3-5 computed at query time from graph structure. Contextual — same triple scores differently in different queries.

**Why one pass**: Dimensions inform each other. Low-similarity but graph-connected should rank higher. High-similarity but low-confidence should rank lower.

#### Context Assembly (`context-assembly.md`)
**Components**:
1. Immediate thread (last 2-3 messages)
2. Relevant knowledge (multi-dim fusion results)
3. Active entities (focused subgraph)
4. Tensions (contradictions)
5. Confidence profile

**Why matters**: Conversations never overflow, context always relevant, knowledge compounds, LLM gets better input.

#### Working Set (`working-set.md`)
**Compact representation** of active conceptual threads:
- Active topics (recency weighted)
- Open questions
- Decisions made this session
- Entities in focus
- "Shape" of conversation

**Maintained between turns**: Resolved threads drop/compress, active threads strengthen, new threads add, dormant threads weaken.

**Relationship to history**: Working set provides conceptual continuity. Last 2-3 messages provide conversational continuity. Together replace full history.

### Other Key Concepts

#### Emergent Ontology (`emergent-ontology.md`)
Entity types, relationships, patterns emerge from use. No upfront schema. "Chris" crystallizes from density of reference. Types are soft labels from relationship patterns.

**Alignment with stigmergy**: Entities are ant trails. Walked paths become highways. Unused entities fade.

#### Federation (`federation.md`)
P2P protocol for sharing selected beliefs. No central server. Each node sovereign.

**What's shared**: Beliefs, confidence scores, fact of provenance (not details), reputation signals.

**Nested epistemic states**: Node B has belief "Node A believes X with conf 0.8". Cross-node corroboration boosts confidence. Cross-node contradiction creates tension.

**Trust propagation**: Corroborated beliefs boost reputation. Contradicted beliefs lower it. Reputation weights future shares.

#### Graceful Degradation (`graceful-degradation.md`) (not read but listed)
System works without inference. Cold engine operates independently. Warm engine makes it better but isn't required.

#### Value Per Token (`value-per-token.md`) (not read but listed)
Primary metric. Every token must justify its cost. Dense over verbose.

---

## Cross-Cutting Themes

### 1. Evolution from v0 to v2

**Current Valence (v0/v1)**:
- Python REST server, Starlette ASGI
- PostgreSQL + pgvector
- Beliefs as rows with metadata columns
- 6D confidence stored at creation
- External embeddings (BGE via API or local)
- Modular `our-*` libraries
- MCP tools for agent integration

**Valence Engine (v2 vision)**:
- Rust sidecar + PostgreSQL split
- Triples as atomic unit (not beliefs)
- Sources as separate provenance layer
- Summaries rendered, never stored
- Topology-derived embeddings (no external model)
- Three self-closing loops (graph→vectors, usage→structure, system→boundary)
- Deterministic core + warm inference layer
- Bounded memory with LRU eviction
- Stigmergic self-organization

### 2. Architectural Principles

1. **Modularity**: Clean separation (persistence vs compute, cold vs warm, substrate vs VKB)
2. **Epistemics-native**: Confidence, provenance, tensions as first-class concerns
3. **Self-optimization**: System improves with use through structural mechanisms
4. **Privacy-first**: Topology embeddings eliminate external model dependency
5. **Lazy compute**: Process only what gets used
6. **Deterministic core**: Bookkeeping over inference where possible
7. **Graceful degradation**: Works without inference, better with it

### 3. OpenClaw Integration Pattern

**Current implementation** (valence-openclaw):
- Memory slot plugin
- REST API to Valence server
- Auto-recall (inject beliefs before agent)
- Auto-capture (extract insights after)
- 58 tools across substrate and VKB layers
- MEMORY.md sync as DR fallback

**Implication for v2**: Plugin layer can remain REST-based while backend evolves to triple-based Rust engine. API contract stable, implementation swappable.

### 4. The Hygiene Problem and Solution

**Current problem** (documented in original agent VISION.md and knowledge-lifecycle.md):
- 843 beliefs, many overlapping
- Query returns 15 similar results instead of 1 consolidated
- Single-inference beliefs propagate as established truth
- No automatic deduplication despite content_hash column
- Corroboration dimension set once, never updated

**V2 solution**:
- Decompose beliefs → triples (dedup at L1)
- Track sources separately (provenance at L2)
- Render summaries on-demand (presentation at L3)
- Corroboration from source count (structural)
- Start beliefs low confidence, increment only through independent corroboration
- Merge model clusters co-retrieved nodes
- Bounded memory evicts unused

### 5. Three Self-Closing Loops as Meta-Pattern

All three loops follow same pattern:
1. **Expensive scaffolding** (LLM embeddings, full history, LLM boundary)
2. **Generate structure/data** (graph density, access patterns, training pairs)
3. **Build replacement** (topology embeddings, clustered structure, fine-tuned model)
4. **Scaffolding falls away** (self-sustaining)

**Result**: System becomes cheaper, faster, more specialized with use. Traditional systems degrade with scale. This improves with scale.

### 6. Testing and Operational Maturity

**Current Valence** (from Gemini docs):
- Comprehensive pytest suite (unit, integration, security)
- Docker Compose test environment
- Makefile targets for focused testing
- Security tests (trust manipulation, auth bypass, data exposure)
- Integration tests (belief lifecycle, federation sync)

**valence-workspace** (from AGENTS.md):
- Continuous improvement mindset
- "Fix the system that allowed the problem"
- Token efficiency targets (90-95% utilization)
- Heartbeat-driven proactive work
- Memory maintenance during heartbeats

**Implication**: v2 needs to maintain operational rigor while evolving architecture.

---

## Design Document Summary (37 Concepts)

### Storage & Architecture (8 docs)
1. **three-layer-architecture**: Triples (stored) + Sources (stored) + Summaries (rendered)
2. **postgres-rust-architecture**: PostgreSQL for persistence, Rust sidecar for compute
3. **rust-engine**: Single embeddable binary like SQLite
4. **triples-atomic**: Triples as atomic unit, self-describing system
5. **graph-vector-duality**: Graph and vector as two views of same knowledge
6. **topology-embeddings**: Embeddings from graph structure, no external model
7. **epistemics-native**: Confidence, provenance, tensions as first-class
8. **bounded-memory**: LRU cache with hard boundary, eviction IS forgetting

### Self-Optimization (6 docs)
9. **self-closing-loops**: Three loops that bootstrap then become self-sustaining
10. **stigmergy**: Self-organization through use, ant trail analogy
11. **lazy-compute**: Process only what gets used
12. **deterministic-core**: Bookkeeping over inference, cold vs warm engine
13. **merge-model**: Clustering via co-retrieval, not synthesis
14. **knowledge-lifecycle**: L0→L1→L2→L3→L4 promotion driven by use

### Knowledge Organization (6 docs)
15. **progressive-summarization**: Layers of refinement, compute follows attention
16. **emergent-ontology**: Types and relationships emerge from use
17. **emergent-dimensions**: Dimensions themselves are emergent (not read in detail)
18. **decay-model**: Temporal validity, unused knowledge fades (not read in detail)
19. **curation**: Use-driven organization (not read in detail)
20. **working-set**: Compact active context between turns

### Retrieval & Context (5 docs)
21. **multi-dim-fusion**: Single-pass scoring across semantic/graph/confidence/temporal/corroboration
22. **context-assembly**: Freshly assembled worldview per turn
23. **value-per-token**: Primary metric, dense over verbose (not read in detail)
24. **graceful-degradation**: Works without inference (not read in detail)
25. **cold-warm-split**: Deterministic core + inference enrichment (covered in deterministic-core)

### Federation & Privacy (3 docs)
26. **federation**: P2P belief sharing, trust-gated propagation
27. **privacy-sovereignty**: Data sovereignty, local control (not read in detail)
28. **network-flows**: What flows through federation network (not read in detail)

### Learning & Evolution (4 docs)
29. **knowledge-loop**: Complete loop from ingestion to insight (not read in detail)
30. **inference-training-loop**: Self-training boundary models
31. **self-training-boundary**: LLM trains its own replacement
32. **emergence-composition**: Emergent behavior from simple rules (not read in detail)

### Implementation Concepts (5 docs)
33. **budget-bounded-ops**: Operations bounded by compute budget (not read in detail)
34. **tool-mediation**: Tools as mediation layer (not read in detail)
35. **intent-capture**: Capturing user intent (not read in detail)
36. **intermediary**: Agent as intermediary (not read in detail)
37. **engine-network-product**: Complete product vision (not read in detail)

**Note**: Not all 37 docs were read in full. Focus was on core architectural concepts (storage, self-optimization, knowledge organization, retrieval). Remaining docs align with themes established by core set.

---

## Recommendations for v2 Development

### Immediate Priorities
1. **Fix current hygiene issues**: Implement deduplication, corroboration tracking, confidence updates
2. **Prototype triple decomposition**: Test LLM's ability to decompose beliefs → triples reliably
3. **Benchmark topology embeddings**: Validate quality vs LLM embeddings at different graph densities
4. **Define migration path**: Current beliefs → triples + sources (preserve provenance)

### Architecture Decisions
1. **Start with sidecar pattern**: PostgreSQL + Rust sidecar easier than full rewrite
2. **Build on petgraph**: Standard library, proven, well-tested
3. **Maintain REST API contract**: Plugin layer unchanged, backend evolves
4. **Hybrid embeddings phase**: Blend LLM + topology during transition

### Risk Mitigation
1. **Incremental migration**: Don't big-bang rewrite, evolve piece by piece
2. **Preserve operational maturity**: Maintain test coverage, security practices
3. **Keep pragmatic security**: Don't introduce ceremony while hardening
4. **Monitor token costs**: Track whether self-closing loops actually reduce costs

### Research Questions
1. **Triple granularity**: How fine-grained should decomposition be?
2. **Loop maturity metrics**: When is graph dense enough for topology embeddings?
3. **Corroboration threshold**: How many independent sessions = established?
4. **Eviction policy**: What's optimal eviction score formula?

---

## Conclusion

The Valence ecosystem demonstrates a clear evolution from a pragmatic belief-storage REST API toward a theoretically grounded, self-optimizing epistemic knowledge substrate. The satellite repos reveal:

- **valence-openclaw**: Mature OpenClaw integration with 58 tools and dual-layer design
- **valence-workspace**: Operational practices of agent dogfooding own substrate
- **valence-visualizations**: Clean architectural boundaries (epistemic primitives, transport, persistence)
- **valence-gemini-docs**: Implementation details confirming modular design and rich MCP integration
- **valence-agent**: Working production system with known limitations (hygiene, no task decomposition)
- **valence-engine**: Comprehensive v2 vision with 37 design concepts converging on self-optimizing graph-native architecture

**Core insight**: The path from v1 to v2 is not a rewrite but an evolution. Three self-closing loops (graph→vectors, usage→structure, system→boundary) transform an external-dependency-heavy system into a self-sustaining one. Scaffolding falls away as the system matures.

**Critical success factors**:
1. Maintain operational maturity while evolving architecture
2. Incremental migration preserving provenance
3. Validate topology embeddings before removing LLM scaffolding
4. Fix current hygiene issues to prove triple decomposition value
5. Keep pragmatic security and token efficiency

The vision is coherent. The path is incremental. The prize is a knowledge substrate that improves with use rather than degrading with scale.

---

**Document Status**: Complete  
**Next Steps**: Commit to valence-v2 repo, begin triple decomposition prototype
