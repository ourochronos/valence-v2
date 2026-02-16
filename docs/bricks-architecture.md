# Ourochronos Bricks Architecture

**Date**: 2026-02-16  
**Author**: Deep-read analysis of all 14 bricks + org repo  
**Purpose**: Comprehensive architecture documentation for Valence v2 planning

---

## Executive Summary

The ourochronos ecosystem consists of 14 focused, reusable "bricks" that compose into the Valence knowledge substrate. Each brick follows strict conventions defined in `our-infra`, maintaining clean interfaces, semantic versioning, and single-responsibility ownership. The architecture exhibits a clear layering strategy: foundational primitives (crypto, storage, models) → infrastructure services (db, identity, privacy) → domain capabilities (embeddings, federation, compliance) → application integration (mcp-base).

**Key Strengths**:
- Clean separation of concerns with minimal coupling
- Consistent use of abstract interfaces with mock and real implementations
- Strong cryptographic foundations (PyCA `cryptography` library)
- Clear state ownership per brick
- Thorough testing conventions (unit/integration markers)

**Key Challenges**:
- `our-federation` has high fan-out dependency (6 other bricks)
- Some bricks are early-stage placeholders (our-consensus, our-network partially implemented)
- Potential coupling between our-privacy and our-db (trust graphs stored in DB)
- our-embeddings couples to both local (sentence-transformers) and remote (OpenAI) providers

---

## 1. Shared Conventions (our-infra)

**Purpose**: Source of truth for how ourochronos projects are structured, built, tested, and released.

**Contents**:
- Standards documentation (naming, versioning, API contracts, testing, state ownership)
- Templates for new bricks and composed projects
- Reusable GitHub Actions workflows (lint, test, release)
- Scaffolding scripts (`new-brick.sh`, `check-conventions.sh`)

**Key Conventions**:

| Aspect | Convention |
|--------|-----------|
| Naming | Bricks: `our-<name>`, packages: `our_<name>`, composed projects: no prefix |
| Interfaces | Clean nouns (`Store`, `Client`), implementations descriptive (`PostgresStore`) |
| Versioning | Semver with `v` prefix, no deprecated shims (clean breaks) |
| Testing | Markers: `unit`, `integration`, `slow`; pytest with `asyncio_mode=auto` |
| State Ownership | **One owner per shared resource** — explicit, documented |
| Build | Hatchling (PEP 517), uv in CI, ruff (lint+format), mypy (strict) |
| Python | >=3.11, line length 120, disallow_untyped_defs=true |

**Dependencies**: None (templates + docs only)

**State Ownership**: None

---

## 2. Foundation Layer

### 2.1 our-models (Core Data Models)

**Purpose**: Python dataclasses for the Valence knowledge substrate — beliefs, entities, sessions, exchanges, patterns, tensions.

**Version**: 0.1.1

**Key Types**:
- **Knowledge Models**: `Belief`, `Entity`, `Source`, `Tension`, `BeliefEntity`
- **Conversation Models**: `Session`, `Exchange`, `Pattern`, `SessionInsight`
- **Temporal**: `TemporalValidity`, `SupersessionChain`, `calculate_freshness()`, `freshness_label()`
- **Enums**: `BeliefStatus`, `EntityType`, `SessionStatus`, `Platform`, `ExchangeRole`, `TensionType`, `TensionSeverity`

**Dependencies**:
```
our-confidence >=0.1.0
```

**Source Structure**:
```
src/our_models/
├── __init__.py        # Public API exports
├── interface.py       # Documentation (models are the interface)
├── models.py          # All dataclasses (Belief, Entity, Session, etc.)
└── temporal.py        # TemporalValidity, SupersessionChain, freshness scoring
```

**Design Highlights**:
- Belief has `DimensionalConfidence` from our-confidence
- Temporal validity with range queries, expiration, overlap checks
- Supersession chains track belief evolution with timestamps and reasons
- Freshness scoring uses exponential decay (configurable half-life)
- All models have `to_dict()` for JSON and `from_row()` for database reconstruction
- Clean separation: models define shape, storage is elsewhere

**State Ownership**: None (pure data models)

**Well-Factored**: ✅ Excellent. Clean data layer with no business logic.

---

### 2.2 our-confidence (Dimensional Confidence)

**Purpose**: Multi-dimensional confidence scoring for beliefs with extensible schema registry.

**Version**: 0.1.0

**Key Types**:
- `DimensionalConfidence`: Core 6 dimensions + extensible via `dimensions` dict
- `ConfidenceDimension` enum: `SOURCE_RELIABILITY`, `METHOD_QUALITY`, `INTERNAL_CONSISTENCY`, `TEMPORAL_FRESHNESS`, `CORROBORATION`, `DOMAIN_APPLICABILITY`
- `DimensionRegistry`: Network-maintained registry with schema validation and inheritance
- `DimensionSchema`: Named dimension sets with required fields and validation rules

**Dependencies**: None (stdlib only)

**Source Structure**:
```
src/our_confidence/
├── __init__.py              # Public exports
├── interface.py             # Docs
├── confidence.py            # DimensionalConfidence class, geometric/arithmetic mean
└── dimension_registry.py    # Schema registry, validation, inheritance
```

**Design Highlights**:
- **Geometric mean preferred** (per MATH.md spec) — better penalizes imbalanced vectors
- Backward-compatible kwargs for core dimensions
- Extensible `dimensions` dict for custom dimensions
- Schema registry with inheritance (e.g., `v1.trust.extended` inherits `v1.trust.core`)
- Built-in schemas: `v1.confidence.core`, `v1.trust.core`, `v1.trust.extended`
- `EPSILON = 0.001` floor to prevent log(0) in geometric mean
- Factory methods: `simple()`, `full()`, `from_dimensions()`

**State Ownership**: None (computation only, singleton registry in-memory)

**Well-Factored**: ✅ Excellent. Clean separation of confidence computation and schema validation.

---

### 2.3 our-crypto (Cryptographic Operations)

**Purpose**: Abstract interfaces for PRE (Proxy Re-Encryption), MLS (Messaging Layer Security), and ZKP (Zero-Knowledge Proofs).

**Version**: 0.1.0

**Key Interfaces**:
- **PRE**: `PREBackend`, `MockPREBackend`, `X25519PREBackend`
  - Unidirectional re-encryption (A→B without seeing plaintext)
  - ECIES hybrid: X25519 DH + HKDF + AES-256-GCM
  - **Trusted-proxy model**: Backend caches DEKs for re-encryption (not blind PRE)
  
- **MLS**: `MLSBackend`, `MockMLSBackend`, `HKDFMLSBackend`
  - Group key agreement with epoch-based ratcheting
  - X25519 DH + HKDF key schedule + Ed25519 signatures
  - **Simplified**: Flat member list (O(n) updates), no full TreeKEM
  - RFC 9420 labeled key derivation

- **ZKP**: `ZKPBackend`, `MockZKPBackend`, `SigmaZKPBackend`
  - Sigma protocols with Fiat-Shamir (non-interactive)
  - Proof types: `HAS_CONSENT`, `WITHIN_POLICY`, `NOT_REVOKED`, `MEMBER_OF_DOMAIN`
  - Optional BLS12-381 Pedersen commitments (if `py-ecc` installed)

**Dependencies**:
```
cryptography >=42.0
py-ecc >=7.0  (optional, for [zkp] extra)
```

**Source Structure**:
```
src/our_crypto/
├── __init__.py          # Factory functions (create_pre_backend, etc.)
├── interface.py         # Public API re-exports
├── _primitives.py       # Shared: HKDF, AES-GCM, X25519, Ed25519
├── pre.py               # PRE interfaces + MockPREBackend
├── pre_real.py          # X25519PREBackend (ECIES)
├── mls.py               # MLS interfaces + MockMLSBackend
├── mls_real.py          # HKDFMLSBackend (RFC 9420 key schedule)
├── zkp.py               # ZKP interfaces + MockZKPBackend
└── zkp_real.py          # SigmaZKPBackend (Schnorr-family)
```

**Design Highlights**:
- Factory functions for backend selection: `create_pre_backend("mock" | "x25519")`
- All real backends use PyCA `cryptography` (audited, well-maintained)
- Consistent pattern: Abstract → Mock (tests) → Real (crypto library)
- PRE: Trusted-proxy model acceptable for federation relays
- MLS: Flat member list sufficient for <100 members (TreeKEM is upgrade path)
- ZKP: Sigma protocols provide real soundness + zero-knowledge under ROM

**State Ownership**: In-memory state only (DEK cache for PRE, group state for MLS, circuit params for ZKP)

**Well-Factored**: ✅ Excellent. Clean abstractions with testable mocks.

**Coupling Note**: PRE's trusted-proxy model means backend holds DEKs — acceptable for semi-trusted relays but documented as limitation.

---

### 2.4 our-storage (Erasure Coding + Merkle Integrity)

**Purpose**: Reed-Solomon erasure coding with Merkle tree integrity for resilient shard-based storage.

**Version**: 0.1.0

**Key Types**:
- `ErasureCodec`: Encode/decode with configurable redundancy levels
- `RedundancyLevel` enum: `MINIMAL` (2-of-3), `PERSONAL` (3-of-5), `FEDERATION` (5-of-9), `PARANOID` (7-of-15)
- `MerkleTree`: Cryptographic integrity with `get_proof()`, `verify_proof()`
- `IntegrityVerifier`: Validate shard sets and individual shards
- `BackendRegistry`: Pluggable storage (`MemoryBackend`, `LocalFileBackend`, future S3)

**Dependencies**: None (pure Python Reed-Solomon implementation)

**Source Structure**:
```
src/our_storage/
├── __init__.py           # Public exports
├── interface.py          # Docs
├── codec.py              # ErasureCodec, RedundancyLevel
├── merkle.py             # MerkleTree, proof generation/verification
├── integrity.py          # IntegrityVerifier, VerificationReport
├── backends/
│   ├── registry.py       # BackendRegistry
│   ├── memory.py         # MemoryBackend (tests/cache)
│   └── local_file.py     # LocalFileBackend (filesystem)
└── shard_repair.py       # Re-encode from available shards
```

**Design Highlights**:
- Pure Python Reed-Solomon (no native deps for portability)
- Merkle trees provide O(log n) integrity proofs (can verify single shard without downloading all)
- Pluggable storage backends via protocol (duck typing)
- Automatic shard repair when some shards are lost
- Quota management per backend
- Round-robin distribution across backends

**State Ownership**: Backends own their storage (filesystem paths, memory buffers)

**Well-Factored**: ✅ Very good. Clean codec/integrity/storage separation.

---

## 3. Infrastructure Layer

### 3.1 our-db (Database Connectivity)

**Purpose**: Database connection management, configuration, migrations.

**Version**: 0.1.0

**Key Features**:
- Connection pooling (sync: `psycopg2`, async: `asyncpg`)
- Pydantic settings for config (`db_host`, `db_port`, `db_name`, etc.)
- Migration runner with up/down/status
- Context managers: `get_cursor()`, `async_cursor()`

**Dependencies**:
```
psycopg2-binary >=2.9
pydantic-settings >=2.0
asyncpg >=0.29  (optional, for [async])
```

**Source Structure**:
```
src/our_db/
├── __init__.py           # Public exports
├── connection.py         # get_cursor(), async_cursor()
├── config.py             # Pydantic settings
└── migrations.py         # MigrationRunner
```

**Design Highlights**:
- Environment-based config (12-factor)
- Context managers ensure connection cleanup
- Migration runner tracks applied migrations in DB table
- Async support is opt-in dependency

**State Ownership**: Database connections, migration state table

**Well-Factored**: ✅ Good. Single responsibility (connectivity, not schema).

**Coupling Note**: Many bricks depend on this for storage, but interface is minimal.

---

### 3.2 our-identity (Decentralized Identity)

**Purpose**: Multi-DID identity management with Ed25519 cryptography and bilateral linking.

**Version**: 0.1.0

**Key Types**:
- `DIDManager`: High-level service (create, link, revoke, resolve, verify)
- `DIDNode`: Single node identity with Ed25519 keypair
- `IdentityCluster`: Group of linked DIDs representing one conceptual identity
- `LinkProof`: Bilateral cryptographic proof (both parties must sign)
- `DIDStore` protocol: Pluggable storage (default: `InMemoryDIDStore`)

**Dependencies**:
```
cryptography >=42.0
```

**Source Structure**:
```
src/our_identity/
├── __init__.py           # Public exports
├── interface.py          # DIDStore protocol
├── manager.py            # DIDManager
├── models.py             # DIDNode, IdentityCluster, LinkProof
├── store.py              # InMemoryDIDStore
└── verification.py       # Link proof verification
```

**Design Highlights**:
- **No master key**: Each DID has independent Ed25519 keypair (compromise one != compromise all)
- **Bilateral linking**: Both parties must sign to prove co-ownership
- **Cluster merging**: Cross-linking DIDs from different clusters merges them automatically
- **Revocation isolation**: Revoking one DID leaves others in cluster unaffected
- **Offline verification**: Link proofs are self-contained (no external authority needed)
- DIDs are deterministic: `did:valence:<ed25519_fingerprint>`

**State Ownership**: DID registry, link proofs, cluster membership

**Well-Factored**: ✅ Excellent. Clean identity primitive with no external dependencies.

---

### 3.3 our-privacy (Trust + Capabilities + Audit + GDPR)

**Purpose**: Comprehensive privacy and trust management — trust graphs, OCAP authorization, audit trails, watermarking, GDPR export.

**Version**: 0.1.0

**Key Features**:
- **Trust Management**: 4D trust edges (competence, integrity, confidentiality, judgment)
- **Capabilities**: Short-lived bearer tokens (JWT-based) with resource + action scoping
- **Sharing Policies**: Graduated levels (private → direct → bounded → public)
- **Audit Logging**: Tamper-evident SHA-256 hash chain
- **Watermarking**: Invisible markers for leak detection
- **GDPR Export**: Self-service data export with compliance audit trail

**Dependencies**:
```
our-db >=0.1.0
cryptography >=41.0
PyJWT >=2.8
```

**Source Structure**:
```
src/our_privacy/
├── __init__.py           # Public exports
├── interface.py          # Protocols
├── trust.py              # TrustEdge4D, TrustService
├── capabilities.py       # issue_capability(), verify_capability()
├── sharing.py            # SharePolicy (private/direct/bounded/public)
├── audit.py              # get_audit_logger(), verify_chain()
├── watermarking.py       # Invisible watermark embedding
├── gdpr.py               # export_user_data(), DeletionReason
└── encryption.py         # ECIES helpers (X25519 + HKDF + AES-GCM)
```

**Design Highlights**:
- Trust graph uses 4 dimensions (extends beyond binary trust/distrust)
- Capabilities use JWT with short TTL (15 min default) to limit exposure
- Audit logs form tamper-evident chain (SHA-256 hash of prev || current)
- Watermarking uses LSB steganography (imperceptible to humans)
- GDPR export bundles all user data + audit trail in JSON

**State Ownership**: Trust edges (DB), capability metadata (DB), audit log chain (DB), watermarked content hashes (DB)

**Well-Factored**: ⚠️ Medium. High cohesion (all privacy-related) but couples trust + audit + watermarking. Could split into our-trust, our-audit, our-gdpr in v2.

**Coupling Note**: Depends on our-db for persistence. Trust graph queries could be heavy.

---

## 4. Domain Capability Layer

### 4.1 our-embeddings (Vector Search)

**Purpose**: Unified interface for generating and searching vector embeddings (local and OpenAI).

**Version**: 0.1.0

**Key Features**:
- Generate embeddings (default: BAAI/bge-small-en-v1.5, 384-dim)
- Search similar content (cosine similarity via pgvector)
- Batch embedding generation
- Backfill missing embeddings
- Provider abstraction (local vs OpenAI)

**Dependencies**:
```
our-db >=0.1.0
openai >=1.0
sentence-transformers >=2.2  (optional, for [local])
numpy >=1.24  (optional, for [local])
```

**Source Structure**:
```
src/our_embeddings/
├── __init__.py           # Public exports
├── interface.py          # EmbeddingProvider protocol
├── service.py            # generate_embedding(), search_similar(), embed_content()
├── local.py              # SentenceTransformerProvider (local models)
├── openai_provider.py    # OpenAIEmbeddingProvider (API)
└── backfill.py           # backfill_embeddings()
```

**Design Highlights**:
- Local-first: Default model runs on-device (no API key needed)
- OpenAI provider as fallback (requires API key in env)
- Embeddings stored in pgvector column (cosine distance search)
- Batch processing for efficiency
- Backfill command for migrations

**State Ownership**: Embeddings table (content_type, content_id, embedding, created_at)

**Well-Factored**: ✅ Good. Clean provider abstraction.

**Coupling Note**: Depends on our-db for storage. Provider choice is runtime config.

---

### 4.2 our-compliance (GDPR + PII)

**Purpose**: GDPR Article 17 compliance (Right to Erasure) and PII scanning.

**Version**: 0.1.0

**Key Features**:
- `delete_user_data()`: GDPR-compliant deletion with audit trail
- `scan_for_pii()`: Detect emails, phone numbers, SSNs, credit cards, etc.
- Tombstone records for federation-aware deletion propagation
- Cryptographic erasure (DEK deletion for encrypted data)

**Dependencies**:
```
our-db >=0.1.0
```

**Source Structure**:
```
src/our_compliance/
├── __init__.py           # Public exports
├── interface.py          # Protocols
├── deletion.py           # delete_user_data(), DeletionReason
├── pii_scanner.py        # scan_for_pii(), regex patterns
└── tombstones.py         # Tombstone record creation + sync
```

**Design Highlights**:
- Deletion creates audit trail (who, when, why, what)
- Tombstones notify federation peers to cascade deletion
- PII scanner uses regex patterns (email, phone, SSN, credit card, IP, etc.)
- Cryptographic erasure option (delete DEK → data unrecoverable)

**State Ownership**: Deletion audit log, tombstone records

**Well-Factored**: ✅ Good. Focused on compliance only.

**Coupling Note**: Depends on our-db. May need coordination with our-federation for tombstone propagation.

---

### 4.3 our-mcp-base (MCP Server Framework)

**Purpose**: Reusable patterns for building MCP (Model Context Protocol) servers.

**Version**: 0.1.0

**Key Features**:
- `MCPServerBase`: Abstract base with lifecycle hooks (startup, health check)
- `ToolRouter`: Decorator-based tool registration and dispatch
- Response helpers: `success_response()`, `error_response()`, `not_found_response()`
- CLI flags: `--health-check`, `--skip-startup-hook`

**Dependencies**:
```
mcp >=1.0
```

**Source Structure**:
```
src/our_mcp_base/
├── __init__.py           # Public exports
├── interface.py          # MCPServerBase abstract class
├── router.py             # ToolRouter with @router.register()
└── responses.py          # success_response(), error_response(), etc.
```

**Design Highlights**:
- Reduces boilerplate for MCP server implementations
- Consistent lifecycle (startup hook, health check, shutdown)
- Tool router avoids manual if/elif chains
- Standard response format across all MCP servers

**State Ownership**: None (framework only)

**Well-Factored**: ✅ Excellent. Pure abstraction layer.

---

### 4.4 our-federation (P2P Knowledge Sharing)

**Purpose**: P2P federation protocol for trust-based knowledge sharing across sovereign Valence nodes.

**Version**: 0.1.0

**Key Features**:
- Node discovery (DNS-based, DID documents)
- Challenge-response authentication
- Trust relationship evolution (Observer → Contributor → Participant → Anchor)
- Belief synchronization (incremental, vector clocks)
- Group encryption (MLS)
- Differential privacy for belief sharing
- Cross-federation consent chains

**Dependencies**:
```
our-db >=0.1.0
our-models >=0.1.0
our-confidence >=0.1.0
our-privacy >=0.1.0
our-embeddings >=0.1.0
our-compliance >=0.1.0
cryptography >=41.0
pydantic >=2.0
aiohttp >=3.9
numpy >=1.24
dnspython >=2.4
mcp >=1.0
```

**Source Structure**:
```
src/our_federation/
├── __init__.py           # Public exports
├── interface.py          # Protocols
├── discovery.py          # discover_node(), register_node()
├── sync.py               # SyncManager, queue_belief_for_sync()
├── trust.py              # TrustSignal, get_effective_trust(), TrustPhase
├── groups.py             # MLS group encryption (create, encrypt, decrypt)
├── privacy.py            # Differential privacy (noise addition, k-anonymity)
└── consent.py            # Cross-federation consent chain verification
```

**Design Highlights**:
- **Trust phases** gate capabilities (read-only → write → full sync → anchor)
- **Incremental sync** via vector clocks (tracks per-node last-sync timestamp)
- **MLS group encryption** for private federation circles
- **Differential privacy** adds calibrated noise to shared aggregates
- **Consent chains** ensure GDPR compliance across federation

**State Ownership**: Federation node registry, trust edges, sync state (vector clocks), group membership

**Well-Factored**: ⚠️ Medium. Comprehensive but high fan-out dependency (6 bricks). Could split into our-federation-core + our-federation-privacy.

**Coupling Note**: Heavy dependencies on our-privacy, our-compliance, our-embeddings. May need careful orchestration.

---

### 4.5 our-network (Onion-Routed P2P Transport)

**Purpose**: Privacy-preserving P2P networking with onion-routed circuits and QoS.

**Version**: 0.1.0 (partially implemented)

**Key Features**:
- End-to-end encryption (X25519 + AES-GCM)
- Onion routing (multi-hop circuits, routers only know prev/next hop)
- Seed node discovery (decentralized router registry)
- Contribution-based QoS
- Traffic analysis mitigation

**Dependencies**:
```
cryptography >=42.0
aiohttp >=3.9
```

**Source Structure**:
```
src/our_network/
├── __init__.py           # Public exports
├── interface.py          # Protocols
├── discovery.py          # create_discovery_client()
├── node_client.py        # create_node_client()
├── encryption.py         # generate_*_keypair(), encrypt/decrypt_message()
├── onion.py              # create_onion(), peel_onion()
└── qos.py                # Contribution-based quality of service
```

**Design Highlights**:
- **Onion routing** provides anonymity (routers can't see full path)
- **Seed nodes** bootstrap discovery (no central authority)
- **QoS based on contribution** (good actors get better service)
- **Traffic analysis mitigation** (cover traffic, timing jitter)

**State Ownership**: Router registry, circuit state, contribution scores

**Well-Factored**: ⚠️ Unknown (partially implemented). Appears focused on transport only.

**Coupling Note**: Independent of other bricks (could be used standalone).

---

### 4.6 our-consensus (VRF-Based Consensus)

**Purpose**: VRF-based consensus, validator selection, and anti-gaming.

**Version**: 0.1.0 (stub/placeholder)

**Dependencies**:
```
cryptography >=42.0  (optional, for [crypto])
```

**State**: Appears to be a placeholder — README is minimal, likely not implemented yet.

**Well-Factored**: N/A (not implemented)

---

## 5. Ourochronos Org Repo

**Purpose**: Meta-project for development environment, processes, and tools.

**Not a brick** — this is the development scaffolding and knowledge base for building the bricks and composed projects.

**Key Contents**:
- Documentation (CLAUDE.md, DEVELOPMENT_PROCESS.md, CURRENT_FOCUS.md, etc.)
- Tools (`./tools/thought`, `./tools/validate`)
- Project roadmap and lessons learned
- Adoption process for external projects
- pg_infinity (PostgreSQL extension for universal search)

**Philosophy**: Incremental wins > Grand plans. Build systems that improve themselves.

---

## Dependency Graph

```
Layer 4 (Application Integration):
  our-mcp-base → mcp

Layer 3 (Domain Capabilities):
  our-federation → our-db, our-models, our-confidence, our-privacy, our-embeddings, our-compliance, cryptography, pydantic, aiohttp, dnspython, mcp
  our-embeddings → our-db, openai, [sentence-transformers]
  our-compliance → our-db
  our-network → cryptography, aiohttp
  our-consensus → [cryptography]

Layer 2 (Infrastructure):
  our-privacy → our-db, cryptography, PyJWT
  our-identity → cryptography
  our-db → psycopg2, pydantic-settings, [asyncpg]

Layer 1 (Foundation):
  our-models → our-confidence
  our-confidence → (none)
  our-crypto → cryptography, [py-ecc]
  our-storage → (none)

Layer 0 (Conventions):
  our-infra → (none, templates + docs only)
```

**Dependency Fan-Out**:
- **our-db**: 5 dependents (our-privacy, our-embeddings, our-compliance, our-federation, indirectly via our-privacy)
- **our-federation**: 6 dependencies (highest coupling)
- **our-confidence**: 2 dependents (our-models, our-federation)
- **cryptography**: Used by our-crypto, our-identity, our-privacy, our-network, our-consensus

**Dependency Fan-In**:
- **our-db**: Central persistence layer (many bricks depend on it)
- **cryptography**: Standard crypto library (shared across bricks)
- **our-models**: Core data types (used by our-federation via our-confidence)

---

## How Bricks Compose into Valence

**Valence** is a composed project that wires together the bricks:

1. **Knowledge Substrate**:
   - `our-models` defines data shapes (Belief, Entity, Session, etc.)
   - `our-confidence` scores belief reliability
   - `our-db` persists everything to PostgreSQL
   - `our-embeddings` enables semantic search

2. **Privacy & Identity**:
   - `our-identity` manages decentralized DIDs
   - `our-privacy` enforces trust graphs, capabilities, audit trails
   - `our-compliance` ensures GDPR compliance

3. **Federation**:
   - `our-federation` syncs beliefs across sovereign nodes
   - `our-crypto` provides PRE (for aggregation), MLS (for groups), ZKP (for proofs)
   - `our-network` routes messages via onion circuits

4. **Storage**:
   - `our-storage` shards data with Reed-Solomon + Merkle integrity
   - Backends distribute shards (local, S3, etc.)

5. **Integration**:
   - `our-mcp-base` provides MCP server framework for Claude/LLM integration
   - MCP servers expose tools (query beliefs, add beliefs, search, etc.)

**Valence v1** likely instantiates these bricks with configuration and wiring logic, providing a unified knowledge substrate for conversational memory.

---

## Well-Factored vs Coupling Issues

### Well-Factored ✅

1. **our-infra**: Perfect — defines conventions without code dependencies.
2. **our-models**: Clean data layer with no business logic.
3. **our-confidence**: Focused on confidence computation + schema registry.
4. **our-crypto**: Clean abstraction pattern (interface → mock → real).
5. **our-storage**: Pure erasure coding + integrity, no external deps.
6. **our-identity**: Self-contained DID management with bilateral proofs.
7. **our-mcp-base**: Pure framework, no domain logic.

### Coupling Issues ⚠️

1. **our-federation**:
   - **Problem**: Depends on 6 other bricks (our-db, our-models, our-confidence, our-privacy, our-embeddings, our-compliance).
   - **Impact**: High risk of cascading changes. Testing requires mocking 6 dependencies.
   - **v2 Fix**: Split into `our-federation-core` (sync protocol only) + `our-federation-privacy` (consent, differential privacy).

2. **our-privacy**:
   - **Problem**: Combines trust graphs, capabilities, audit, watermarking, GDPR — high cohesion but broad scope.
   - **Impact**: Changes to trust logic may affect audit logging (unintentional coupling).
   - **v2 Fix**: Split into `our-trust`, `our-audit`, `our-gdpr`, `our-capabilities`.

3. **our-embeddings**:
   - **Problem**: Couples to both local (sentence-transformers) and remote (OpenAI) providers.
   - **Impact**: Optional dependencies create deployment complexity.
   - **v2 Fix**: Make provider fully pluggable (config-driven, not import-driven).

4. **our-db**:
   - **Problem**: Central dependency for 5+ bricks (persistence bottleneck).
   - **Impact**: Database schema changes affect many bricks.
   - **v2 Fix**: Consider event sourcing or document store to decouple schema from bricks.

5. **our-network**:
   - **Status**: Partially implemented, unclear if ready for production.
   - **v2 Action**: Complete implementation or clearly mark as experimental.

6. **our-consensus**:
   - **Status**: Placeholder/stub.
   - **v2 Action**: Implement or remove to avoid dead code.

---

## Lessons for Valence v2 Module Boundaries

### Keep These Patterns ✅

1. **One owner per shared resource** — Clear ownership prevents conflicts.
2. **Interface → Mock → Real** — Testability + flexibility.
3. **Factory functions for backend selection** — Runtime config without import changes.
4. **No deprecated shims** — Clean breaks with major version bumps.
5. **Strict typing with mypy** — Catches errors early.
6. **pytest markers (unit/integration)** — Fast CI with selective test runs.
7. **Pydantic for config** — Type-safe settings from env vars.
8. **Stdlib-only foundations** — our-confidence and our-storage have zero runtime deps.

### Change for v2 ⚠️

1. **Split high-coupling bricks**:
   - `our-federation` → `our-federation-core` + `our-federation-privacy`
   - `our-privacy` → `our-trust` + `our-audit` + `our-gdpr` + `our-capabilities`

2. **Decouple from our-db**:
   - Consider event sourcing (bricks publish events, consumers persist)
   - Or: Abstract storage behind `Store` protocol (like `DIDStore` in our-identity)

3. **Provider abstraction**:
   - Make `our-embeddings` provider fully pluggable (runtime config, no import coupling)
   - Same for `our-crypto` backends (already partially done via factories)

4. **Remove stubs**:
   - Complete `our-consensus` or remove it
   - Complete `our-network` or mark as experimental

5. **Dependency hygiene**:
   - Before adding a brick dependency, ask: "Can this be passed as a parameter instead?"
   - Prefer composition over inheritance in brick relationships

6. **Testing bricks in isolation**:
   - Each brick should have a full test suite without requiring integration with other bricks
   - Use mocks for dependencies (already mostly done)

7. **Consider a message bus**:
   - For high-coupling scenarios (e.g., our-federation coordinating privacy + compliance + embeddings), use async message passing instead of direct imports

---

## Summary Table

| Brick | Layer | Dependencies | Dependents | State? | Status | Coupling Risk |
|-------|-------|--------------|------------|--------|--------|---------------|
| **our-infra** | 0 | None | All (conventions) | No | ✅ Complete | None |
| **our-models** | 1 | our-confidence | our-federation | No | ✅ Complete | Low |
| **our-confidence** | 1 | None | our-models, our-federation | No | ✅ Complete | Low |
| **our-crypto** | 1 | cryptography, [py-ecc] | our-privacy, our-federation, our-network | Memory | ✅ Complete | Low |
| **our-storage** | 1 | None | Future sharding | Yes (backends) | ✅ Complete | Low |
| **our-db** | 2 | psycopg2, pydantic, [asyncpg] | 5+ bricks | Yes (DB) | ✅ Complete | **High** |
| **our-identity** | 2 | cryptography | our-federation | Yes (DID registry) | ✅ Complete | Low |
| **our-privacy** | 2 | our-db, cryptography, PyJWT | our-federation | Yes (trust, audit, watermarks) | ✅ Complete | **Medium** |
| **our-embeddings** | 3 | our-db, openai, [sentence-transformers] | our-federation | Yes (embeddings) | ✅ Complete | **Medium** |
| **our-compliance** | 3 | our-db | our-federation | Yes (audit, tombstones) | ✅ Complete | Low |
| **our-mcp-base** | 4 | mcp | MCP servers | No | ✅ Complete | None |
| **our-federation** | 3 | **6 bricks** + many libs | Composed projects | Yes (nodes, sync state) | ✅ Complete | **Very High** |
| **our-network** | 3 | cryptography, aiohttp | our-federation? | Yes (circuits, routers) | ⚠️ Partial | Unknown |
| **our-consensus** | 3 | [cryptography] | None? | Unknown | ❌ Stub | N/A |

---

## Conclusion

The ourochronos brick architecture is **well-designed at the foundation level** (our-models, our-confidence, our-crypto, our-storage, our-identity) with clean interfaces, strong typing, and excellent testability. The **infrastructure layer** (our-db, our-privacy) is solid but shows some coupling to central persistence. The **domain capability layer** (our-federation, our-embeddings, our-compliance) exhibits higher coupling, particularly our-federation which depends on 6 other bricks.

**For Valence v2**, the key improvements are:
1. **Split high-coupling bricks** (our-federation, our-privacy) into smaller, focused modules
2. **Decouple from our-db** via abstraction or event sourcing
3. **Complete or remove stubs** (our-consensus, our-network)
4. **Strengthen provider abstraction** (our-embeddings)
5. **Consider message bus** for cross-cutting concerns

The foundation is strong — v2 can build on these patterns while addressing the coupling hotspots identified above.
