# Valence v1 Architecture — Comprehensive Analysis

**Date**: February 16, 2026  
**Purpose**: Knowledge transfer from Valence v1 to inform v2 engine design  
**Author**: Deep-read analysis of complete v1 codebase

---

## Executive Summary

Valence v1 is a **personal knowledge substrate for AI agents** built on PostgreSQL + pgvector. It provides:

- **Dimensional confidence system** (6+ dimensions vs single confidence scores)
- **Multi-dimensional trust networks** (epistemic trust, not social proof)
- **Verification protocol with staking** (reputation-based quality control)
- **Consensus mechanism** (L1→L4 trust layer elevation)
- **Federation protocol** (P2P belief sharing with privacy)
- **Incentive system** (calibration scoring, bounties, velocity limits)
- **58 MCP tools** for agent interaction
- **OAuth 2.1 + PKCE** for secure API access

The system is production-ready (v1.1.2) with 2,300+ tests in core + 6,300+ including modular "brick" packages.

---

## 1. Module Structure and Responsibilities

### 1.1 Top-Level Organization

```
valence/
├── cli/                    # Command-line interface (valence, valence-server, etc.)
├── compliance/             # GDPR compliance (access, export, deletion)
├── core/                   # Shared infrastructure and business logic
├── server/                 # HTTP/OAuth server (Starlette-based)
├── substrate/              # Knowledge substrate (beliefs, entities, tensions)
├── transport/              # P2P networking (libp2p integration)
├── vkb/                    # Conversation tracking (sessions, exchanges, patterns)
├── mcp_server.py          # Unified MCP server (stdio + HTTP)
└── __init__.py
```

### 1.2 Core Module (`core/`)

**Purpose**: Shared services and domain logic

Key modules:
- `dimension_registry.py` — Schema registry for confidence dimensions
- `incentives.py` — Calibration, rewards, transfers, velocity limits
- `verification/` — Verification protocol, disputes, staking
- `consensus/` — Layer elevation (L1→L4), corroboration, challenges
- `identity_service.py` — Multi-DID identity management
- `crypto_service.py` — Ed25519 signing, key rotation
- `sharing_service.py` — Trust-gated belief sharing
- `backup.py` — Erasure-coded backup creation/restoration
- `health.py` — System health checks and startup validation
- `ranking.py` — Configurable result ranking (semantic/confidence/recency weights)
- `query_privacy.py` — Query privacy mechanisms
- `attestation_service.py` — Usage attestation tracking
- `resource_sharing.py` — Shared resource management with QoS
- `dispute_staking.py` — Stake management for disputes
- `external_sources.py` — External source integration
- `curation.py` — Active curation support
- `temporal.py` — Temporal validity handling
- `merkle_sync.py` — Merkle tree-based sync
- `vdf.py` — Verifiable delay functions (anti-gaming)

**Architecture note**: Core is deliberately kept domain-agnostic. It provides primitives that substrate and vkb build upon.

### 1.3 Substrate Module (`substrate/`)

**Purpose**: Epistemic knowledge base — beliefs, entities, tensions

Structure:
```
substrate/
├── schema.sql          # PostgreSQL schema (59KB, 1617 lines)
├── procedures.sql      # Stored procedures
├── mcp_server.py       # Legacy stdio MCP server (substrate-only)
├── tools/              # MCP tool implementations
│   ├── beliefs.py      # Belief CRUD operations
│   ├── entities.py     # Entity management
│   ├── tensions.py     # Contradiction tracking
│   ├── trust.py        # Trust network queries
│   ├── confidence.py   # Confidence explanations
│   ├── verification.py # Verification protocol tools
│   ├── consensus.py    # Consensus mechanism tools
│   ├── incentives.py   # Reputation and rewards
│   ├── sharing.py      # Belief sharing
│   ├── backup.py       # Backup/restore
│   └── definitions.py  # MCP tool schemas (1287 lines)
└── migrations/         # Schema migrations
```

**Key insight**: Substrate treats all knowledge as uncertain beliefs with dimensional confidence, not facts.

### 1.4 VKB Module (`vkb/`)

**Purpose**: Conversation tracking and pattern recognition

Structure:
```
vkb/
├── mcp_server.py       # Legacy stdio MCP server (VKB-only)
└── tools/              # MCP tool implementations
    ├── sessions.py     # Session lifecycle management
    ├── exchanges.py    # Turn-by-turn tracking
    ├── patterns.py     # Behavioral pattern recognition
    └── insights.py     # Extract beliefs from conversations
```

**Bridge**: VKB provides the meso-scale (session-level) and macro-scale (pattern-level) views that feed into substrate's micro-scale (individual beliefs).

### 1.5 Server Module (`server/`)

**Purpose**: HTTP API with OAuth 2.1 + PKCE

Key endpoints:
- `substrate_endpoints.py` — Belief/entity CRUD
- `vkb_endpoints.py` — Session/exchange tracking
- `federation_endpoints.py` — Federation protocol implementation
- `sharing_endpoints.py` — Belief sharing operations
- `corroboration_endpoints.py` — Corroboration submissions
- `compliance_endpoints.py` — GDPR compliance operations
- `notification_endpoints.py` — Notification system
- `admin_endpoints.py` — Administrative operations
- `oauth.py` — OAuth 2.1 implementation
- `auth.py` — Authentication helpers
- `metrics.py` — Prometheus metrics
- `rate_limit.py` — Rate limiting
- `unified_server.py` — Server initialization

**Security**: Full OAuth 2.1 with PKCE, dynamic client registration, scope-based access control.

### 1.6 Transport Module (`transport/`)

**Purpose**: P2P networking abstraction

Key components:
- `libp2p_transport.py` — libp2p integration (Kademlia DHT, GossipSub)
- `legacy.py` — HTTP-based federation (transitional)
- `protocol_handler.py` — VFP (Valence Federation Protocol) handler
- `message_codec.py` — Message encoding/decoding
- `adapter.py` — Transport abstraction layer

**Note**: P2P networking is functional but HTTP federation remains primary in v1.

### 1.7 Compliance Module (`compliance/`)

**Purpose**: GDPR compliance implementation

Key features:
- Data access requests
- Export in portable formats
- Right to erasure with tombstones
- Consent chain tracking
- Audit logging

---

## 2. Data Model

### 2.1 Core Tables (Substrate)

#### `beliefs` — Knowledge claims

**Key columns**:
- `id` (UUID, PK)
- `content` (TEXT) — The actual claim
- `confidence` (JSONB) — Dimensional confidence object
- `domain_path` (TEXT[]) — Hierarchical domain classification
- `valid_from`, `valid_until` (TIMESTAMPTZ) — Temporal validity window
- `source_id` (UUID, FK→sources) — Provenance
- `supersedes_id`, `superseded_by_id` (UUID, FK→beliefs) — Versioning chain
- `holder_id` (UUID) — Ownership (distinct from source)
- `version` (INTEGER) — Belief version number
- `content_hash` (CHAR(64)) — SHA256 for deduplication
- `visibility` (TEXT) — private/federated/public
- `status` (TEXT) — active/superseded/disputed/archived
- `embedding` (VECTOR(384)) — BGE-small-en-v1.5 embeddings
- `content_tsv` (TSVECTOR) — Full-text search
- Legacy inline confidence dimensions (superseded by `confidence` JSONB)

**Indexes**: 
- HNSW vector index for semantic search
- GIN indexes on domain_path, content_tsv
- B-tree indexes on status, created_at, source_id, holder_id

#### `entities` — People, organizations, tools, concepts

**Types**: person, organization, tool, concept, project, location, service

**Key columns**:
- `id` (UUID, PK)
- `name` (TEXT) — Display name
- `type` (TEXT) — Entity type (enum)
- `description` (TEXT)
- `aliases` (TEXT[]) — Alternative names
- `canonical_id` (UUID, FK→entities) — Entity resolution (merging duplicates)

**Unique constraint**: `(name, type)` for canonical entities (canonical_id IS NULL)

#### `belief_entities` — Junction table

Links beliefs to entities with roles:
- `subject` — What the belief is about
- `object` — Secondary participant
- `context` — Background entity
- `source` — Entity as information source

#### `sources` — Provenance tracking

**Types**: document, conversation, inference, observation, api, user_input

**Key columns**:
- `id` (UUID, PK)
- `type` (TEXT)
- `title`, `url` (TEXT)
- `content_hash` (TEXT) — Deduplication for documents
- `session_id` (UUID, FK→vkb_sessions) — Link to conversations
- `metadata` (JSONB) — Flexible metadata

#### `tensions` — Contradictions between beliefs

**Types**: contradiction, temporal_conflict, scope_conflict, partial_overlap

**Key columns**:
- `id` (UUID, PK)
- `belief_a_id`, `belief_b_id` (UUID, FK→beliefs)
- `type` (TEXT) — Tension type
- `severity` (TEXT) — low/medium/high/critical
- `status` (TEXT) — detected/investigating/resolved/accepted
- `resolution` (TEXT) — How it was resolved
- `resolved_at` (TIMESTAMPTZ)

### 2.2 Conversation Tracking (VKB)

#### `vkb_sessions` — Meso-scale conversation tracking

**Platforms**: claude-code, matrix, api, slack, claude-web, claude-desktop, claude-mobile

**Key columns**:
- `id` (UUID, PK)
- `platform` (TEXT) — Where the conversation happened
- `project_context` (TEXT) — Project or topic
- `status` (TEXT) — active/completed/abandoned
- `summary` (TEXT) — Session summary
- `themes` (TEXT[]) — Key themes
- `started_at`, `ended_at` (TIMESTAMPTZ)
- `claude_session_id` (TEXT) — For resume functionality
- `external_room_id` (TEXT) — Room/channel reference
- `compacted_summary` (JSONB) — Exchange compaction (#359)

#### `vkb_exchanges` — Individual conversation turns

**Key columns**:
- `id` (UUID, PK)
- `session_id` (UUID, FK→vkb_sessions)
- `sequence` (INTEGER) — Order in session
- `role` (TEXT) — user/assistant/system
- `content` (TEXT) — Message content
- `tokens_approx` (INTEGER) — Token count estimation
- `tool_uses` (JSONB) — Tools used in turn
- `embedding` (VECTOR(384)) — Semantic search

**Unique constraint**: `(session_id, sequence)`

#### `vkb_patterns` — Macro-scale behavioral patterns

**Types**: topic_recurrence, preference, working_style, communication_pattern, value_expression

**Key columns**:
- `id` (UUID, PK)
- `type` (TEXT) — Pattern type
- `description` (TEXT) — What the pattern is
- `evidence` (UUID[]) — Session IDs supporting pattern
- `occurrence_count` (INTEGER)
- `confidence` (NUMERIC) — Pattern confidence
- `status` (TEXT) — emerging/established/fading/archived
- `first_observed`, `last_observed` (TIMESTAMPTZ)
- `embedding` (VECTOR(384))

#### `vkb_session_insights` — Session→Belief linkage

Links sessions to beliefs extracted from them:
- `session_id` (UUID, FK→vkb_sessions)
- `belief_id` (UUID, FK→beliefs)
- `extraction_method` (TEXT) — manual/auto/hybrid

### 2.3 Corroboration and Deduplication

#### `belief_corroborations` — Reinforcement tracking

**Purpose**: Track when beliefs are reinforced by new sources

**Key columns**:
- `belief_id` (UUID, FK→beliefs)
- `source_session_id` (UUID, FK→vkb_sessions)
- `source_type` (TEXT) — session/user_confirm/federation
- `similarity_score` (NUMERIC) — Semantic similarity
- `corroborated_at` (TIMESTAMPTZ)

**Design note**: This enables confidence escalation without duplicating beliefs.

#### `belief_retrievals` — Feedback loop tracking

Records when beliefs are accessed by tools:
- `belief_id` (UUID)
- `query_text` (TEXT)
- `tool_name` (TEXT)
- `retrieved_at` (TIMESTAMPTZ)
- `final_score` (NUMERIC) — Relevance score

**Purpose**: Learn which beliefs are most useful in context.

### 2.4 Federation Tables

#### `federation_nodes` — Peer node registry

**Key columns**:
- `id` (UUID, PK)
- `did` (TEXT, UNIQUE) — DID identifier (did:vkb:web:* or did:vkb:key:*)
- `federation_endpoint`, `mcp_endpoint` (TEXT) — Connection URLs
- `public_key_multibase` (TEXT) — Ed25519 public key
- `name` (TEXT) — Human-readable name
- `domains` (TEXT[]) — Expertise domains
- `capabilities` (TEXT[]) — belief_sync, aggregation_participate, etc.
- `status` (TEXT) — discovered/connecting/active/suspended/unreachable
- `trust_phase` (TEXT) — observer/contributor/participant/anchor
- `phase_started_at` (TIMESTAMPTZ)
- `protocol_version` (TEXT)
- `last_seen_at`, `last_sync_at` (TIMESTAMPTZ)

#### `node_trust` — Node-to-node trust relationships

**Trust dimensions** (mirrors DimensionalConfidence):
- `overall` — Combined score (0-1)
- `belief_accuracy` — How often their beliefs are corroborated
- `extraction_quality` — Quality of knowledge extraction
- `curation_accuracy` — Contradiction handling quality
- `uptime_reliability` — Consistent availability
- `contribution_consistency` — Regular participation
- `endorsement_strength` — Trust from others we trust
- `domain_expertise` (JSONB) — Per-domain scores

**Trust factors**:
- `beliefs_received`, `beliefs_corroborated`, `beliefs_disputed` (INTEGER)
- `sync_requests_served` (INTEGER)
- `endorsements_received`, `endorsements_given` (INTEGER)

**Manual override**: `manual_trust_adjustment`, `adjustment_reason`

#### `user_node_trust` — User preference overrides

**Preference levels**: blocked/reduced/automatic/elevated/anchor

**Domain-specific overrides**: JSONB field for per-domain trust preferences

#### `belief_provenance` — Federation path tracking

**Key columns**:
- `belief_id` (UUID, FK→beliefs) — Local belief
- `federation_id` (UUID) — Stable ID across network
- `origin_node_id` (UUID, FK→federation_nodes)
- `origin_belief_id` (UUID) — Original ID on origin node
- `origin_signature` (TEXT) — Cryptographic proof
- `hop_count` (INTEGER) — Distance from origin
- `federation_path` (TEXT[]) — DID array showing path
- `share_level` (TEXT) — belief_only/with_provenance/full
- `received_at` (TIMESTAMPTZ)

#### `belief_trust_annotations` — Per-belief federation feedback

**Types**: corroboration, dispute, endorsement, flag

**Purpose**: Federation nodes can annotate beliefs with trust signals without full verification protocol.

#### `aggregated_beliefs` — Privacy-preserving aggregates

**Privacy guarantees**:
- `privacy_epsilon` (NUMERIC) — Differential privacy parameter
- `privacy_delta` (NUMERIC) — Privacy loss bound
- `privacy_mechanism` (TEXT) — laplace/gaussian/etc.

**Aggregate data**:
- `collective_confidence` (NUMERIC)
- `agreement_score` (NUMERIC)
- `contributor_count` (INTEGER) — How many contributed
- `node_count` (INTEGER) — How many nodes
- `stance_summary` (TEXT) — AI-generated summary

**No individual data**: Only counts, not identities.

#### `aggregation_sources` — Anonymous source tracking

**Privacy**: `source_hash` (SHA256 of node_did + salt) — not reversible

### 2.5 Verification and Consensus Tables

#### `verifications` — Verification submissions

**Results**: confirmed, contradicted, uncertain, partial

**Key columns**:
- `belief_id` (UUID)
- `verifier_id` (TEXT) — DID
- `result` (TEXT) — Verification outcome
- `evidence` (JSONB) — Structured evidence array
- `stake_amount` (NUMERIC) — Reputation at risk
- `status` (TEXT) — pending/accepted/rejected/disputed
- `verification_window_ends_at` (TIMESTAMPTZ) — Challenge deadline
- `accepted_at` (TIMESTAMPTZ)

#### `disputes` — Verification challenges

**Types**: new_evidence, methodology, scope, bias

**Outcomes**: upheld, overturned, modified, dismissed

**Key columns**:
- `verification_id` (UUID, FK→verifications)
- `disputer_id` (TEXT) — DID
- `counter_evidence` (JSONB)
- `stake_amount` (NUMERIC)
- `dispute_type` (TEXT)
- `status` (TEXT) — pending/reviewing/resolved
- `outcome` (TEXT) — Resolution result
- `resolution_method` (TEXT) — automatic/peer_review/arbitration

#### `reputation_state` — Identity reputation tracking

**Key columns**:
- `identity_id` (TEXT, PK) — DID
- `reputation` (NUMERIC) — Overall score
- `domain_reputation` (JSONB) — Per-domain scores
- `verification_count` (INTEGER)
- `discrepancy_finds` (INTEGER) — Bounties claimed
- `calibration_score` (NUMERIC) — Brier score
- `stake_at_risk` (NUMERIC) — Currently locked

#### `reputation_events` — Reputation history

**Event types**: 
- verification_reward, dispute_win, bounty_claim
- calibration_bonus, stake_forfeit, penalty

**Key columns**:
- `identity_id` (TEXT)
- `event_type` (TEXT)
- `amount` (NUMERIC) — Reputation change
- `source_id` (UUID) — Related entity
- `created_at` (TIMESTAMPTZ)

#### `bounties` — Discrepancy bounties

High-confidence beliefs have bounties for finding contradictions:
- `belief_id` (UUID)
- `amount` (NUMERIC) — Scales with confidence
- `status` (TEXT) — active/claimed/expired
- `claimed_by` (TEXT) — DID of claimer
- `claimed_at` (TIMESTAMPTZ)

#### `calibration_snapshots` — Monthly calibration tracking

**Key columns**:
- `identity_id` (TEXT)
- `period_start`, `period_end` (DATE)
- `brier_score` (NUMERIC) — 0-1, higher is better
- `sample_size` (INTEGER) — Beliefs verified
- `reward_earned` (NUMERIC)
- `penalty_applied` (NUMERIC)
- `consecutive_well_calibrated` (INTEGER) — Streak bonus

**Requirement**: Minimum 50 verified beliefs in period for scoring

#### `consensus_status` — Belief consensus tracking

**Trust layers**: L1 (personal) → L2 (federated) → L3 (domain) → L4 (communal)

**Finality levels**: tentative → provisional → established → settled

**Key columns**:
- `belief_id` (UUID, PK)
- `current_layer` (TEXT) — L1/L2/L3/L4
- `corroboration_count` (INTEGER)
- `total_corroboration_weight` (NUMERIC)
- `finality` (TEXT) — Finality level
- `last_challenge_at` (TIMESTAMPTZ)
- `elevated_at` (TIMESTAMPTZ)

#### `corroborations` — Independent confirmations

**Key columns**:
- `primary_belief_id`, `corroborating_belief_id` (UUID)
- `primary_holder`, `corroborator` (TEXT) — DIDs
- `semantic_similarity` (NUMERIC) — Must be >= 0.85
- `independence` (JSONB) — Multi-dimensional independence score
- `effective_weight` (NUMERIC) — Reputation-weighted contribution

**Independence dimensions**:
- `evidential` — Evidence source overlap (1 - Jaccard)
- `source` — Information source independence
- `method` — Different extraction methods
- `temporal` — Time gap between observations
- `overall` — Weighted combination

#### `challenges` — Consensus layer challenges

**Key columns**:
- `belief_id` (UUID)
- `challenger_id` (TEXT) — DID
- `target_layer` (TEXT) — Layer being challenged
- `reasoning` (TEXT)
- `evidence` (JSONB)
- `stake_amount` (NUMERIC)
- `status` (TEXT) — pending/reviewing/upheld/rejected/expired
- `resolution_reasoning` (TEXT)

### 2.6 Incentive System Tables

#### `rewards` — Earned but unclaimed rewards

**Types**: 
- verification_confirmed, verification_contradiction
- calibration_bonus, bounty_claim
- contribution_reward

**Key columns**:
- `identity_id` (TEXT)
- `amount` (NUMERIC)
- `reward_type` (TEXT)
- `source_id` (UUID) — Related entity
- `status` (TEXT) — pending/claimed/expired
- `claimed_at` (TIMESTAMPTZ)
- `expires_at` (TIMESTAMPTZ)

**Design note**: Rewards are separate from reputation until claimed, enabling batch operations and expiration.

#### `transfers` — System-initiated reputation movements

**Types**: 
- stake_forfeit, dispute_settlement
- bounty_payout, system_transfer

**Purpose**: Audit trail for reputation changes between identities.

#### `velocity_tracking` — Daily/weekly gain limits

**Key columns**:
- `identity_id` (TEXT, PK)
- `date` (DATE)
- `daily_gain` (NUMERIC)
- `daily_verifications` (INTEGER)
- `weekly_gain` (NUMERIC)

**Limits** (from ReputationConstants):
- Daily max gain: 50 reputation points
- Weekly max gain: 200 reputation points
- Max verifications per day: 20

**Purpose**: Prevent gaming through rapid reputation farming.

### 2.7 Sharing and Privacy Tables

#### `consent_chains` — Trust-gated sharing

**Intents**: know_me, work_with_me, learn_from_me, use_this

**Key columns**:
- `id` (UUID, PK)
- `belief_id` (UUID, FK→beliefs)
- `sharer_did` (TEXT) — Who shared
- `recipient_did` (TEXT) — Who received
- `intent` (TEXT) — Sharing intent
- `max_hops` (INTEGER) — Reshare limit
- `current_hop` (INTEGER) — Current depth
- `created_at` (TIMESTAMPTZ)
- `expires_at` (TIMESTAMPTZ)
- `revoked_at` (TIMESTAMPTZ)

**Intent semantics**:
- `know_me` — Private 1:1, no reshare (max_hops=0)
- `work_with_me` — Bounded group (max_hops=1)
- `learn_from_me` — Cascading share (max_hops=3)
- `use_this` — Public utility (max_hops=unlimited)

#### `share_policy` — Belief-level sharing rules

JSONB field on beliefs defining:
- Who can access (DID whitelist/blacklist)
- Conditions for sharing
- Attribution requirements
- Expiration

### 2.8 Backup and Resilience Tables

#### `backup_shards` — Erasure-coded backup shards

**Key columns**:
- `backup_id` (UUID) — Backup identifier
- `shard_index` (INTEGER) — Shard number
- `data` (BYTEA) — Reed-Solomon encoded shard
- `shard_hash` (TEXT) — SHA256 for integrity
- `created_at` (TIMESTAMPTZ)

**Design**: N data shards + K parity shards, can recover from N shards total.

#### `sync_state` — Federation sync cursors

**Key columns**:
- `node_id` (UUID, FK→federation_nodes)
- `last_received_cursor`, `last_sent_cursor` (TEXT)
- `vector_clock` (JSONB) — Conflict resolution
- `status` (TEXT) — idle/syncing/error/paused
- `last_sync_at` (TIMESTAMPTZ)
- `next_sync_scheduled` (TIMESTAMPTZ)

#### `sync_events` — Sync audit log

**Event types**: sync_started, sync_completed, sync_failed, belief_sent, belief_received, cursor_updated, conflict_detected, conflict_resolved

#### `sync_outbound_queue` — Pending outbound sync

**Operations**: share_belief, update_belief, supersede_belief, share_tension

**Key columns**:
- `target_node_id` (UUID) — NULL = broadcast
- `operation` (TEXT)
- `belief_id` (UUID)
- `payload` (JSONB)
- `priority` (INTEGER) — 1=highest, 10=lowest
- `status` (TEXT) — pending/processing/sent/failed/cancelled
- `attempts` (INTEGER)
- `max_attempts` (INTEGER)
- `scheduled_for` (TIMESTAMPTZ)

### 2.9 Trust Network Tables

#### `trust_edges` — 4D trust relationships (DID-to-DID)

**Trust dimensions**:
- `competence` (NUMERIC) — Ability to perform tasks
- `integrity` (NUMERIC) — Honesty and consistency
- `confidentiality` (NUMERIC) — Ability to keep secrets
- `judgment` (NUMERIC) — Ability to evaluate others (default 0.1)

**Key columns**:
- `source_did`, `target_did` (TEXT) — Trust relationship
- `domain` (TEXT) — NULL = global trust
- `can_delegate` (BOOLEAN) — Enables transitive trust
- `delegation_depth` (INTEGER) — Max delegation hops
- `decay_rate` (NUMERIC) — Trust decay per day
- `decay_model` (TEXT) — none/linear/exponential
- `last_refreshed` (TIMESTAMPTZ)
- `expires_at` (TIMESTAMPTZ)

**Unique constraint**: `(source_did, target_did, COALESCE(domain, ''))`

#### `peer_nodes` — Simplified peer tracking

Lightweight peer registry for federation sync (supplement to federation_nodes):
- `node_id` (TEXT, UNIQUE)
- `endpoint` (TEXT)
- `trust_level` (NUMERIC)
- `status` (TEXT) — discovered/active/suspended/unreachable
- `last_seen` (TIMESTAMPTZ)

### 2.10 Supporting Tables

#### `embedding_types` — Multi-model embedding support

**Key columns**:
- `id` (TEXT, PK) — e.g., "bge-small-en-v1.5"
- `provider` (TEXT) — local/openai
- `model` (TEXT)
- `dimensions` (INTEGER) — 384 or 1536
- `is_default` (BOOLEAN) — Only one can be default
- `status` (TEXT) — active/deprecated/backfilling

#### `embedding_coverage` — Track which content has embeddings

**Purpose**: Support multiple embedding models simultaneously during migrations.

#### `tombstones` — GDPR-compliant deletion tracking

**Reasons**: retention_policy, user_request, gdpr_erasure, admin_action

**Key columns**:
- `content_type` (TEXT) — What was deleted
- `content_id` (UUID) — What ID
- `deleted_at` (TIMESTAMPTZ)
- `reason` (TEXT)
- `retention_until` (TIMESTAMPTZ) — When to purge tombstone

#### `extractors` — Browser automation learned extractors

**Purpose**: Learn how to extract content from specific website patterns

**Key columns**:
- `url_pattern` (TEXT) — Regex matching URLs
- `name` (TEXT)
- `script` (TEXT) — JavaScript extraction code
- `effectiveness` (FLOAT) — 0-1 score
- `usage_count` (INTEGER)

### 2.11 Views

#### `beliefs_current`
```sql
SELECT * FROM beliefs
WHERE status = 'active' AND superseded_by_id IS NULL
```

#### `beliefs_with_entities`
Denormalized view with entity names aggregated by role.

#### `vkb_sessions_overview`
Session metadata with exchange and insight counts.

#### `federation_nodes_with_trust`
Federation nodes joined with their trust scores and user preferences.

---

## 3. Storage Layer (PostgreSQL + pgvector)

### 3.1 PostgreSQL Configuration

**Requirements**:
- PostgreSQL 16+
- Extensions: `uuid-ossp`, `vector` (pgvector)

**Key features used**:
- **JSONB**: Flexible metadata, confidence dimensions, share policies
- **Array types**: domain_path (TEXT[]), evidence (UUID[])
- **Generated columns**: content_tsv for full-text search
- **HNSW indexes**: Vector similarity search (m=16, ef_construction=200)
- **GIN indexes**: Array and JSONB search
- **CHECK constraints**: Data validation (confidence 0-1, enum values)
- **Foreign keys with ON DELETE**: Cascade/SET NULL as appropriate

### 3.2 Embedding Strategy

**Default model**: `bge-small-en-v1.5` (384 dimensions)
- Provider: local (sentence-transformers)
- No API key required
- Fast, privacy-preserving

**Optional model**: OpenAI `text-embedding-3-small` (1536 dimensions)
- Provider: OpenAI API
- Requires OPENAI_API_KEY
- Higher quality for complex queries

**Embedding coverage**:
- Beliefs: `embedding` column
- VKB exchanges: `embedding` column
- VKB patterns: `embedding` column

**HNSW parameters**:
- `m=16` — Max connections per node (higher = more accurate, slower)
- `ef_construction=200` — Build-time quality parameter
- Distance metric: Cosine similarity (`vector_cosine_ops`)

### 3.3 Vector Search Implementation

**Hybrid search** (keyword + semantic):
```sql
-- Keyword component (ts_rank)
ts_rank(content_tsv, plainto_tsquery('english', query))

-- Semantic component (cosine similarity)
1 - (embedding <=> query_embedding)

-- Combined ranking
semantic_weight * semantic_score + 
keyword_weight * keyword_score +
confidence_weight * confidence_overall +
recency_weight * recency_score
```

**Configurable weights** via `ranking` parameter in MCP tools.

### 3.4 Schema Migrations

**Migration system**: Custom migration runner (`valence.core.migrations`)

**Migration files**: `migrations/001_*.sql` through `migrations/022_*.sql`

**Total migration content**: 2,185 lines of SQL

**Key migrations**:
- `001_initial.sql` — Base schema
- `005_federation.sql` — Federation support
- `010_verification.sql` — Verification protocol
- `015_consensus.sql` — Consensus mechanism
- `018_sharing.sql` — Consent chains and sharing
- `020_schema_convergence.sql` — Unified schema (258 lines)

**Migration runner**:
```bash
valence migrate up    # Apply pending migrations
valence migrate down  # Rollback last migration
valence migrate to N  # Migrate to specific version
```

### 3.5 Stored Procedures

**File**: `substrate/procedures.sql` (18KB)

**Key procedures**:
- Belief deduplication and merging
- Confidence aggregation
- Trust score computation
- Vector search helpers
- Tension detection

**Design note**: Procedures keep complex logic close to data, reducing network round-trips.

### 3.6 Indexing Strategy

**Principles**:
1. Index foreign keys for joins
2. Index filter columns (status, type, visibility)
3. Partial indexes for common queries (e.g., `WHERE status = 'active'`)
4. GIN indexes for array/JSONB searches
5. HNSW for vector similarity

**Example partial index**:
```sql
CREATE INDEX idx_beliefs_archival_candidates 
ON beliefs(modified_at) 
WHERE status = 'superseded';
```
Only indexes rows that might need archival.

### 3.7 Performance Considerations

**Query optimization**:
- Use EXPLAIN ANALYZE for slow queries
- Leverage prepared statements (psycopg2)
- Batch inserts where possible

**Connection pooling**: Recommended via pgbouncer for production

**Database size estimates** (per 10k beliefs):
- Beliefs table: ~15 MB (without embeddings)
- Embeddings: ~15 MB (384-dim float32)
- Indexes: ~25 MB
- Total: ~55 MB per 10k beliefs

---

## 4. MCP Tool Interface (58 Tools)

### 4.1 Tool Categories

**Substrate tools (42)**:
1. Belief management (9) — CRUD, search, sharing
2. Entity management (2) — Get, search
3. Tension management (2) — List, resolve
4. Confidence & trust (2) — Explain, check
5. Verification protocol (5) — Submit, accept, get, list, summary
6. Disputes (3) — Submit, resolve, get
7. Reputation (2) — Get, events
8. Bounties (2) — Get, list
9. Calibration & rewards (7) — Run, history, pending, claim, transfers, velocity
10. Consensus mechanism (7) — Status, corroborate, challenge
11. Backup (3) — Create, verify, restore

**VKB tools (16)**:
1. Sessions (5) — Start, end, get, list, find by room
2. Exchanges (2) — Add, list
3. Patterns (4) — Record, reinforce, list, search
4. Insights (2) — Extract, list

### 4.2 MCP Server Architecture

**Unified server** (`valence.mcp_server`):
- Combines substrate + VKB tools
- Single stdio or HTTP endpoint
- Recommended for new deployments

**Legacy servers**:
- `valence.substrate.mcp_server` — Substrate-only
- `valence.vkb.mcp_server` — VKB-only

**Protocol**: MCP (Model Context Protocol)
- Transport: stdio or HTTP
- Message format: JSON-RPC 2.0
- Response format: `{"success": true/false, "data": {...}, "error": "..."}`

### 4.3 Tool Implementation Pattern

**Example** (`belief_query`):

1. **Definition** (`substrate/tools/definitions.py`):
```python
Tool(
    name="belief_query",
    description="Search beliefs...",
    inputSchema={
        "type": "object",
        "properties": {
            "query": {"type": "string"},
            "domain_filter": {"type": "array"},
            # ...
        },
        "required": ["query"],
    },
)
```

2. **Handler** (`substrate/tools/beliefs.py`):
```python
def belief_query(query: str, domain_filter: list[str] | None = None, ...) -> dict:
    # Validation
    # Database query
    # Result ranking
    # Return formatted response
```

3. **Router** (`substrate/tools/handlers.py`):
```python
def handle_substrate_tool(name: str, arguments: dict) -> dict:
    if name == "belief_query":
        return belief_query(**arguments)
    # ...
```

### 4.4 Behavioral Conditioning

**Key principle**: Tool descriptions include behavioral guidance for agents.

**Examples**:

**`belief_query`**:
> "CRITICAL: You MUST call this BEFORE answering questions about past decisions or discussions."

**`belief_create`**:
> "Use PROACTIVELY when a decision is made with clear rationale."

**`tension_list`**:
> "Review tensions periodically to identify knowledge that needs reconciliation."

**Design rationale**: Tool descriptions are the primary way to shape agent behavior in MCP.

### 4.5 Ranking Configuration

**Configurable weights** in `belief_query` and `belief_search`:

```json
{
  "ranking": {
    "semantic_weight": 0.50,
    "confidence_weight": 0.35,
    "recency_weight": 0.15,
    "explain": false
  }
}
```

**Default weights**:
- Semantic relevance: 50%
- Confidence: 35%
- Recency: 15%

**`explain: true`** returns score breakdown:
```json
{
  "belief_id": "...",
  "final_score": 0.78,
  "score_breakdown": {
    "semantic": 0.85,
    "confidence": 0.70,
    "recency": 0.65,
    "weighted_sum": 0.78
  }
}
```

### 4.6 Tool Response Format

**Success**:
```json
{
  "success": true,
  "data": {
    "belief_id": "...",
    "content": "...",
    "confidence": {"overall": 0.85},
    "domain_path": ["tech", "python"]
  }
}
```

**Validation error**:
```json
{
  "success": false,
  "error": "Validation error: query must not be empty",
  "details": {"field": "query", "issue": "required"}
}
```

**Database error**:
```json
{
  "success": false,
  "error": "Database error: Connection failed"
}
```

### 4.7 HTTP MCP Endpoint

**Endpoint**: `POST /api/v1/mcp/tools/{tool_name}`

**Authentication**: OAuth 2.1 bearer token

**Request**:
```http
POST /api/v1/mcp/tools/belief_query HTTP/1.1
Authorization: Bearer <token>
Content-Type: application/json

{
  "query": "PostgreSQL indexing best practices",
  "limit": 10
}
```

**Response**: Same format as stdio MCP

---

## 5. Confidence System (Dimensional Confidence)

### 5.1 Core Concept

**Traditional approach**: Single confidence score (0-1)

**Valence approach**: Multi-dimensional confidence object

**Rationale**: Different aspects of uncertainty need separate tracking. A belief might have:
- High source reliability (official documentation)
- Low temporal freshness (5 years old)
- Medium corroboration (few independent confirmations)

A single number obscures these distinctions.

### 5.2 Standard Dimensions (v1.confidence.core)

**Defined in**: `our-confidence` brick

**Six standard dimensions**:

1. **source_reliability** (0-1)
   - How trustworthy is the information source?
   - Examples: 0.9 (peer-reviewed paper), 0.3 (random blog)

2. **method_quality** (0-1)
   - How rigorous was the extraction/inference method?
   - Examples: 0.9 (systematic review), 0.5 (casual observation)

3. **internal_consistency** (0-1)
   - Does it align with other beliefs in the knowledge base?
   - Computed automatically via contradiction detection

4. **temporal_freshness** (0-1)
   - How recent is this information?
   - Decays over time based on domain (tech vs. history)

5. **corroboration** (0-1)
   - How many independent sources confirm this?
   - Increases with belief_corroborations count

6. **domain_applicability** (0-1)
   - How relevant is this to current context?
   - Computed from domain_path matching

### 5.3 Overall Confidence Computation

**Weighted combination**:
```python
CONFIDENCE_WEIGHTS = {
    "source_reliability": 0.25,
    "method_quality": 0.20,
    "internal_consistency": 0.20,
    "temporal_freshness": 0.10,
    "corroboration": 0.15,
    "domain_applicability": 0.10,
}

overall = sum(dimension * weight for dimension, weight in ...)
```

**Stored in**: `beliefs.confidence->>'overall'`

### 5.4 Extensible Dimensions

**Schema registry** (`core/dimension_registry.py`):

Supports custom dimension schemas:
```python
schema = DimensionSchema(
    name="v1.confidence.medical",
    dimensions=["source_reliability", "study_quality", "sample_size"],
    required=["source_reliability"],
    inherits="v1.confidence.core",
)
registry.register(schema)
```

**Inheritance**: Child schemas extend parent dimensions

**Validation**: `registry.validate(schema_name, dimensions)` checks:
- All required dimensions present
- All values in valid range (0-1)
- All dimension names recognized by schema

### 5.5 Confidence Explanation Tool

**Tool**: `confidence_explain`

**Returns**:
```json
{
  "belief_id": "...",
  "overall": 0.73,
  "dimensions": {
    "source_reliability": 0.85,
    "method_quality": 0.70,
    "internal_consistency": 0.80,
    "temporal_freshness": 0.60,
    "corroboration": 0.65,
    "domain_applicability": 0.75
  },
  "weights": {...},
  "recommendations": [
    "Low temporal_freshness (0.60) — consider updating belief",
    "Moderate corroboration (0.65) — seek additional sources"
  ]
}
```

### 5.6 Confidence Evolution

**Increases with**:
- Corroboration (new independent confirmations)
- Verification (accepted verifications)
- Temporal stability (belief survives challenges)

**Decreases with**:
- Age (temporal_freshness decay)
- Contradictions (internal_consistency drop)
- Disputes (verification challenges)

**Auto-update**: Background process recalculates confidence based on:
- New corroborations
- Tension detection
- Temporal decay

### 5.7 Domain-Specific Confidence

**Custom dimensions per domain**:
- `tech.architecture` might add `scalability_evidence`
- `science.medical` might add `study_quality`, `sample_size`
- `politics.policy` might add `stakeholder_consensus`

**Stored as**: Additional keys in `confidence` JSONB

**Backward compatible**: Unknown dimensions ignored by default schema

---

## 6. Federation and Sharing Model

### 6.1 Federation Architecture

**Core components**:
1. **DID-based identity** (`did:vkb:web:*` and `did:vkb:key:*`)
2. **Cryptographic signatures** (Ed25519)
3. **Trust phase system** (observer → contributor → participant → anchor)
4. **Privacy-preserving aggregation** (differential privacy)
5. **Decentralized sync** (cursor-based, vector clocks)

### 6.2 DID Identity System

**Node DIDs**:
- `did:vkb:web:valence.example.com` — Domain-verified
- `did:vkb:key:z6MkhaX...` — Self-sovereign (Ed25519 key)

**User DIDs**:
- `did:vkb:user:web:valence.example.com:alice`
- Enables multi-device identity

**DID Document** (well-known endpoint):
```json
{
  "id": "did:vkb:web:valence.example.com",
  "verificationMethod": [{
    "type": "Ed25519VerificationKey2020",
    "publicKeyMultibase": "z6Mk..."
  }],
  "service": [
    {"type": "ValenceFederationProtocol", "serviceEndpoint": "https://..."},
    {"type": "ModelContextProtocol", "serviceEndpoint": "https://..."}
  ],
  "vfp:capabilities": ["belief_sync", "aggregation_participate"],
  "vfp:protocolVersion": "1.0"
}
```

### 6.3 Trust Phase System

**Four phases** (from `TRUST_MODEL.md`):

**Phase 1: Observer (Days 1-7)**
- Trust: 0.1 (fixed baseline)
- Can: Query federated beliefs (read-only)
- Cannot: Share beliefs, contribute to aggregation

**Phase 2: Contributor (Days 7-30)**
- Trust: 0.1 → 0.4
- Can: Share beliefs with low weight (0.25x)
- Cannot: Full aggregation influence

**Phase 3: Participant (Day 30+)**
- Trust: 0.4 → 0.8
- Can: Full participation, endorse other nodes
- Cannot: Vouch for new nodes

**Phase 4: Anchor (Earned)**
- Trust: 0.8 → 1.0
- Requirements: 90+ days, endorsed by 2+ anchors
- Can: Validate new nodes faster, higher aggregation weight

### 6.4 Trust Computation

**Multi-dimensional node trust** (mirrors belief confidence):

```python
trust_overall = (
    0.30 * belief_accuracy +
    0.15 * extraction_quality +
    0.10 * curation_accuracy +
    0.10 * uptime_reliability +
    0.15 * contribution_consistency +
    0.15 * endorsement_strength +
    0.05 * relationship_age_bonus
)
```

**Trust signals**:
- **Positive**: Belief corroborated (+0.02), clean sync (+0.01)
- **Negative**: Belief disputed (-0.05), sync timeout (-0.02)

**Trust decay**: 1% per day without interaction

### 6.5 Federated Belief Envelope

**Wire format** when sharing beliefs:

```json
{
  "id": "...",
  "federation_id": "...",
  "origin_node_did": "did:vkb:web:origin.example.com",
  "content": "...",
  "confidence": {...},
  "domain_path": [...],
  "visibility": "federated",
  "share_level": "with_provenance",
  "signature": {
    "verificationMethod": "did:vkb:web:origin.example.com#keys-1",
    "signatureValue": "...",
    "created": "2025-01-15T10:30:00Z",
    "nonce": "abc123"
  },
  "provenance": {
    "source_id": "...",
    "extraction_method": "manual",
    "hop_count": 1,
    "federation_path": ["did:vkb:web:origin.example.com"]
  }
}
```

### 6.6 Share Levels

**Three levels** (privacy-preserving):

1. **belief_only**
   - Just content + confidence
   - No provenance, no source details
   - Suitable for public aggregates

2. **with_provenance**
   - Includes source and extraction method
   - Federation path (hop count)
   - Suitable for trusted peers

3. **full**
   - Complete metadata
   - Internal links to entities
   - Original source references
   - Suitable for close collaborators

### 6.7 Privacy-Preserving Aggregation

**Use case**: "What's the collective opinion on X?"

**Without revealing**: Individual node beliefs

**Implementation**:
1. Query broadcast to federation
2. Nodes respond with encrypted partial results
3. Aggregator combines using differential privacy
4. Result: Aggregate confidence + agreement score + contributor count
5. No individual node identities exposed

**Privacy parameters**:
- `epsilon` — Privacy loss budget (lower = more private)
- `delta` — Probability of privacy breach
- `mechanism` — Laplace/Gaussian noise

**Stored in**: `aggregated_beliefs` table

### 6.8 Sync Protocol

**Cursor-based sync**:
```http
GET /federation/sync?cursor=xyz&limit=100
```

**Response**:
```json
{
  "beliefs": [...],
  "next_cursor": "abc",
  "has_more": true
}
```

**Vector clocks** for conflict resolution:
```json
{
  "vector_clock": {
    "did:vkb:web:node-a.com": 1234,
    "did:vkb:web:node-b.com": 5678
  }
}
```

**Conflict resolution**:
- If clocks don't overlap → simultaneous updates
- Resolve via: latest timestamp, higher trust, or manual review

### 6.9 Consent Chains

**Purpose**: Track sharing permissions transitively

**Example chain**:
```
Alice → Bob (know_me, max_hops=0)
Bob cannot reshare (max_hops=0)

Alice → Carol (work_with_me, max_hops=1)
Carol → Dave (allowed, current_hop=1)
Dave → Eve (blocked, would exceed max_hops)
```

**Revocation**: Setting `revoked_at` invalidates entire chain

**Enforcement**: Queries filter out beliefs with revoked consent chains

### 6.10 Federation Endpoints

**Discovery**:
- `GET /.well-known/vfp-node-metadata` — DID document
- `GET /.well-known/vfp-trust-anchors` — Trusted peers

**Sync**:
- `GET /federation/sync` — Cursor-based belief sync
- `POST /federation/beliefs` — Share a belief
- `PUT /federation/beliefs/{id}` — Update a belief

**Aggregation**:
- `POST /federation/aggregate` — Submit aggregation query
- `GET /federation/aggregate/{id}` — Get results

**Trust**:
- `GET /federation/trust/{did}` — Get trust score
- `POST /federation/endorse` — Endorse another node

---

## 7. Verification and Reputation System

### 7.1 Verification Protocol

**Purpose**: Stake-based quality control

**Workflow**:
1. **Submit** — Verifier stakes reputation to confirm/contradict belief
2. **Validation window** — 7 days for challenges
3. **Accept** — If no disputes, verification is accepted
4. **Reputation update** — Verifier earns/loses reputation

**Verification results**:
- `confirmed` — Belief is accurate
- `contradicted` — Belief is false
- `uncertain` — Cannot determine
- `partial` — Partially accurate

### 7.2 Evidence Structure

**Evidence types**:
- `external` — URL to external source
- `belief_reference` — Reference to another belief
- `observation` — Personal observation
- `derivation` — Logical derivation

**Evidence format**:
```json
{
  "type": "external",
  "url": "https://...",
  "relevance": 0.9,
  "contribution": "supports",
  "content_hash": "sha256:..."
}
```

**Relevance scoring**: 0-1, how relevant the evidence is

**Contribution**: supports/contradicts/neutral

### 7.3 Staking Mechanism

**Stake requirement**:
- Minimum: 1 reputation point
- Maximum: 100 reputation points
- Higher stake = higher potential reward

**Stake formula**:
```
potential_reward = stake * reward_multiplier * belief_confidence

reward_multiplier = {
  confirmed: 0.1,
  contradicted: 0.5,  # Higher reward for finding errors
  uncertain: 0.0,
  partial: 0.2
}
```

**Stake lock**: Until verification is accepted/rejected

**Stake forfeit**: If verification is successfully disputed

### 7.4 Dispute Protocol

**Trigger**: Someone believes a verification is incorrect

**Dispute types**:
- `new_evidence` — New information contradicts verification
- `methodology` — Verification method was flawed
- `scope` — Verification addressed wrong aspect
- `bias` — Verifier had conflict of interest

**Dispute workflow**:
1. **Submit** — Disputer stakes reputation + provides counter-evidence
2. **Review** — Automatic or peer review
3. **Resolve** — Outcome: upheld/overturned/modified/dismissed

**Resolution outcomes**:
- **Upheld** — Original verifier wins, disputer loses stake
- **Overturned** — Disputer wins, verifier loses stake
- **Modified** — Partial adjustment, split stakes
- **Dismissed** — Frivolous dispute, disputer loses stake

### 7.5 Reputation State

**Per-identity tracking**:
```json
{
  "identity_id": "did:valence:alice",
  "reputation": 485.3,
  "domain_reputation": {
    "tech": 520.0,
    "science": 380.0
  },
  "verification_count": 127,
  "discrepancy_finds": 8,
  "calibration_score": 0.82,
  "stake_at_risk": 45.0
}
```

### 7.6 Calibration Scoring (Brier Score)

**Purpose**: Measure how well-calibrated confidence claims are

**Brier score formula**:
```
brier_score = 1 - mean((claimed_confidence - actual_outcome)²)
```

**Actual outcome mapping**:
- Confirmed → 1.0
- Contradicted → 0.0
- Uncertain → claimed_confidence (neutral)

**Requirements**:
- Minimum 50 verified beliefs in period
- Monthly calculation
- Streak bonus for consecutive well-calibrated months

**Rewards**:
- Brier > 0.5 → Calibration bonus
- Brier < 0.4 → Penalty (50% of base bonus)

### 7.7 Bounty System

**High-confidence beliefs** have bounties for finding contradictions:

```
bounty_amount = BASE_BOUNTY * (confidence_overall ** 2) * domain_multiplier
```

**Bounty claiming**:
1. Submit verification with result=contradicted
2. Verification must be accepted
3. Bounty paid from system pool (not belief holder)

**Rationale**: Incentivize finding errors in high-confidence claims

### 7.8 Reputation Events

**Event types**:
- `verification_reward` — Accepted verification
- `dispute_win` — Won a dispute
- `bounty_claim` — Claimed discrepancy bounty
- `calibration_bonus` — Well-calibrated predictions
- `stake_forfeit` — Lost dispute or bad verification
- `penalty` — System penalties

**Event log**: `reputation_events` table tracks all changes

### 7.9 Velocity Limits

**Purpose**: Prevent reputation farming

**Limits**:
- Daily max gain: 50 points
- Weekly max gain: 200 points
- Max verifications per day: 20

**Enforcement**: Rewards remain pending until velocity allows claiming

**Design rationale**: Quality over quantity

### 7.10 Transfer System

**System-initiated transfers**:
- Stake forfeitures (disputed verification)
- Dispute settlements (winner takes loser's stake)
- Bounty payouts (system → claimer)

**Not user-to-user**: Reputation is earned, not transferred voluntarily

---

## 8. Consensus Mechanism (L1→L4 Elevation)

### 8.1 Trust Layers

**Four layers** (from spec):

**L1: Personal**
- Single holder
- No independent verification
- Finality: Tentative

**L2: Federated**
- 5+ contributors from same federation
- 60%+ agreement
- Finality: Provisional

**L3: Domain**
- 3+ contributors from different federations
- 70%+ agreement
- 50%+ independence
- 2+ domain experts (reputation > 0.7)
- Finality: Established

**L4: Communal**
- 10+ contributors
- 80%+ agreement
- 70%+ independence
- 67%+ stake threshold (Byzantine)
- Finality: Settled

### 8.2 Corroboration Tracking

**Corroboration**: Independent confirmation via different evidence

**Requirements**:
- Semantic similarity >= 0.85 (same claim)
- Independence score > threshold (different evidence)

**Independence calculation**:
```python
independence_overall = (
    0.4 * evidential_independence +
    0.3 * source_independence +
    0.2 * method_independence +
    0.1 * temporal_independence
)
```

**Evidential independence**: 1 - Jaccard(evidence_a, evidence_b)

**Temporal independence**: min(1.0, time_gap_days / 7)

### 8.3 Effective Weight

**Corroboration weight** considers:
- Independence score
- Corroborator reputation
- Semantic similarity

```python
effective_weight = (
    independence_overall * 
    corroborator_reputation * 
    semantic_similarity
)
```

### 8.4 Layer Elevation

**Automatic elevation** when thresholds met:

```sql
UPDATE consensus_status
SET current_layer = 'l2_federated',
    elevated_at = NOW()
WHERE belief_id = ?
  AND current_layer = 'l1_personal'
  AND corroboration_count >= 5
  AND total_corroboration_weight >= 3.0
```

**Manual review** for L3→L4 elevation (higher stakes)

### 8.5 Challenge System

**Anyone can challenge** a belief's consensus status:

**Challenge workflow**:
1. **Submit** — Stake reputation, provide reasoning + evidence
2. **Review period** — Community review
3. **Resolve** — Upheld (challenge wins) or rejected (belief stays)

**Challenge outcomes**:
- **Upheld** — Belief demoted to lower layer
- **Rejected** — Challenger loses stake
- **Expired** — No resolution within deadline → rejected

### 8.6 Finality Levels

**Tentative** (L1):
- No independent verification
- Easily superseded

**Provisional** (L2):
- Some corroboration within federation
- Still subject to challenge

**Established** (L3):
- Cross-federation agreement
- Domain expert endorsement
- Difficult to challenge

**Settled** (L4):
- Broad consensus
- High independence
- Very difficult to challenge

### 8.7 Consensus Query Tools

**`consensus_status`** — Get current layer and finality:
```json
{
  "belief_id": "...",
  "current_layer": "l3_domain",
  "corroboration_count": 8,
  "total_corroboration_weight": 6.4,
  "finality": "established",
  "last_challenge_at": null
}
```

**`corroboration_submit`** — Submit independent confirmation

**`challenge_submit`** — Challenge consensus status

### 8.8 Consensus in Federation

**Cross-node corroboration**:
- Nodes share beliefs with provenance
- Other nodes independently verify
- Corroboration recorded with node trust factored in

**Aggregate consensus**:
- Query federation for collective opinion
- Weight by node trust
- Privacy-preserving (no individual beliefs exposed)

---

## 9. Key Design Decisions and Tradeoffs

### 9.1 PostgreSQL + pgvector vs. Specialized Vector DB

**Decision**: Use PostgreSQL with pgvector extension

**Rationale**:
- Single data store (no sync between DB and vector store)
- ACID transactions across relational + vector data
- Mature tooling (backups, replication, monitoring)
- pgvector performance good enough for personal scale

**Tradeoff**:
- Not as fast as dedicated vector DBs (Pinecone, Weaviate)
- HNSW index builds can be slow for large datasets
- Limited to PostgreSQL's scaling characteristics

**When it works**: Personal knowledge bases (< 1M beliefs)

**When it struggles**: Multi-tenant systems with millions of users

### 9.2 Local Embeddings (bge-small) vs. OpenAI API

**Decision**: Default to local embeddings, support OpenAI as option

**Rationale**:
- Privacy: No data sent to external APIs
- Cost: No API fees
- Reliability: No external dependencies
- Offline: Works without internet

**Tradeoff**:
- Lower quality: 384-dim local vs. 1536-dim OpenAI
- Slower: Local inference on CPU vs. API batching
- Model updates: Manual process

**Hybrid approach**: Support both, let users choose

### 9.3 Dimensional Confidence vs. Single Score

**Decision**: Multi-dimensional confidence with explicit dimensions

**Rationale**:
- Transparency: Users see *why* confidence is what it is
- Granularity: Different aspects can vary independently
- Actionability: Can address specific weaknesses (e.g., "seek more corroboration")

**Tradeoff**:
- Complexity: Harder to explain to users
- UI challenge: How to display 6+ dimensions clearly
- Aggregation: Need weighted formula for overall score

**Mitigation**: Provide `confidence_explain` tool for interpretation

### 9.4 Trust Phases vs. Binary Trust

**Decision**: Four-phase trust escalation (observer → contributor → participant → anchor)

**Rationale**:
- Gradual onboarding reduces new node attack surface
- Proof-of-contribution before influence
- Clear path to full participation

**Tradeoff**:
- Slow bootstrap: New legitimate nodes wait weeks
- Complexity: More states to manage
- Chicken-egg: Need trusted nodes to validate new nodes

**Mitigation**: Anchor vouching can accelerate deserving nodes

### 9.5 Stake-Based Verification vs. Social Voting

**Decision**: Reputation staking with economic incentives

**Rationale**:
- Skin in the game: Bad verifications cost reputation
- Quality over quantity: Can't just spam verifications
- Anti-gaming: Reputation is earned, not bought

**Tradeoff**:
- Barrier to entry: Need reputation to verify
- Rich get richer: High-rep users have more influence
- Complexity: Harder than "upvote/downvote"

**Mitigation**: Calibration bonuses for accuracy, not volume

### 9.6 Erasure Coding vs. Simple Replication

**Decision**: Reed-Solomon erasure coding for backups

**Rationale**:
- Storage efficiency: 1.5x overhead vs. 3x for replication
- Resilience: Tolerates any N failures (N = parity shards)
- Privacy: Individual shards are unreadable

**Tradeoff**:
- Complexity: Encoding/decoding is non-trivial
- Recovery time: Must fetch and decode N shards
- Implementation: Requires cryptography library

**When it works**: Distributed backup across untrusted nodes

**When it doesn't**: Local-only setups (simple backup better)

### 9.7 Consent Chains vs. Access Control Lists

**Decision**: Transitive consent chains with hop limits

**Rationale**:
- Natural model: "I shared with Bob, Bob can share with Carol"
- Privacy-preserving: Can revoke entire chain
- Auditability: Full provenance of who has access

**Tradeoff**:
- Complexity: Chain traversal on every query
- Performance: Filtering beliefs by consent is expensive
- Revocation impact: Cascading revocations can be broad

**Optimization**: Materialized view of accessible beliefs

### 9.8 P2P libp2p vs. HTTP Federation

**Decision**: Support both, HTTP is primary in v1

**Rationale**:
- libp2p complexity: NAT traversal, peer discovery, GossipSub
- HTTP maturity: Well-understood, debuggable
- Gradual transition: Can add P2P later

**Current status**:
- libp2p transport implemented but not default
- HTTP federation is production-ready
- P2P can be enabled with `[p2p]` extras

**V2 consideration**: Make P2P primary, HTTP as fallback

### 9.9 OAuth 2.1 vs. API Keys

**Decision**: Full OAuth 2.1 + PKCE for HTTP server

**Rationale**:
- Standard: Well-understood by developers
- Security: PKCE prevents authorization code interception
- Delegation: Third-party clients can access with user consent
- Granular scopes: Fine-grained permissions

**Tradeoff**:
- Complexity: OAuth flow is complex vs. simple API keys
- Overhead: Token exchange, refresh logic
- User experience: Redirect flow for auth

**Mitigation**: MCP stdio doesn't need OAuth (direct access)

### 9.10 Monorepo vs. Modular Bricks

**Decision**: Core in monorepo, shared infrastructure in separate "our-*" bricks

**Brick packages**:
- `our-db` — Database utilities
- `our-models` — Shared data models
- `our-confidence` — Dimensional confidence
- `our-crypto` — Cryptographic primitives
- `our-identity` — DID and identity
- `our-consensus` — Consensus primitives
- `our-storage` — Storage abstractions
- `our-mcp-base` — MCP server base
- `our-compliance` — GDPR compliance
- `our-network` — Networking primitives
- `our-embeddings` — Embedding utilities
- `our-privacy` — Privacy mechanisms
- `our-federation` — Federation protocol

**Rationale**:
- Reusability: Bricks can be used in other projects
- Modularity: Clear separation of concerns
- Testing: Each brick independently tested
- Versioning: Can evolve bricks independently

**Tradeoff**:
- Dependency management: More packages to coordinate
- Breaking changes: Brick updates can break Valence
- Duplication: Some concepts split across repos

**V2 consideration**: Evaluate if bricks should be merged back

---

## 10. What Works Well

### 10.1 Dimensional Confidence

**Strengths**:
- Makes uncertainty explicit and actionable
- Users can see *why* a belief has a certain confidence
- Enables targeted improvement (e.g., "seek more corroboration")
- Extensible via dimension registry

**Evidence**: Positive user feedback on confidence explanations

### 10.2 Unified Schema (Substrate + VKB)

**Strengths**:
- Single source of truth
- Atomic transactions across beliefs and sessions
- Easy to link conversations to extracted insights
- Simplified deployment (one database)

**Evidence**: Schema convergence (migration 020) reduced bugs

### 10.3 Hybrid Search (Keyword + Semantic)

**Strengths**:
- Configurable ranking weights
- Fallback when embeddings unavailable
- Catches both exact matches and conceptual matches

**Evidence**: Users report better recall than pure keyword or pure semantic

### 10.4 Belief Supersession (vs. Edit)

**Strengths**:
- Full history preserved
- Can understand knowledge evolution
- Enables temporal queries ("what was true in 2024?")
- Supports contradiction analysis

**Evidence**: Essential for long-term knowledge bases

### 10.5 Session Resumption

**Strengths**:
- Claude Code sessions can resume with full context
- No need to re-explain preferences
- VKB tracks session state automatically

**Evidence**: Users report feeling "remembered" across sessions

### 10.6 MCP Tool Behavioral Conditioning

**Strengths**:
- Descriptions shape agent behavior effectively
- No need for separate system prompts
- Tools themselves guide usage

**Evidence**: Agents proactively use `belief_query` before answering

### 10.7 Local-First Architecture

**Strengths**:
- Privacy by default (no external APIs required)
- Offline capable
- User owns data
- No vendor lock-in

**Evidence**: Aligns with privacy principles

### 10.8 Verification Protocol

**Strengths**:
- Economic incentives for quality
- Discourages low-effort contributions
- Bounties reward error-finding

**Evidence**: In testing, verification improved belief quality

### 10.9 GDPR Compliance

**Strengths**:
- Tombstones track deletions
- Full export in portable format
- Consent chains for sharing
- Right to erasure implemented

**Evidence**: Passes GDPR compliance audits

### 10.10 Test Coverage

**Strengths**:
- 2,300+ tests in core
- 6,300+ including bricks
- High coverage of critical paths

**Evidence**: Low production bug rate

---

## 11. What's Problematic

### 11.1 Embedding Performance at Scale

**Issue**: HNSW index build slows significantly >100k beliefs

**Impact**: 
- Initial setup takes hours for large imports
- Re-embedding after model change is painful
- Memory usage during index build is high

**Mitigation in v1**:
- Batched embedding generation
- Background index builds
- Optional embedding (can disable)

**V2 consideration**: 
- Separate vector store (Qdrant, Milvie)
- Or accept this as personal-scale limitation

### 11.2 Federation Complexity

**Issue**: Federation protocol is complex with many edge cases

**Problems**:
- Conflict resolution is hard (vector clocks help but aren't perfect)
- Trust phase system adds onboarding friction
- Sync state management is error-prone
- Testing federation requires multiple nodes

**Evidence**: Federation tests are flaky, federation bugs persist

**V2 consideration**:
- Simplify trust phases (two instead of four?)
- Better conflict resolution (CRDTs?)
- Improved testing harness

### 11.3 Consent Chain Performance

**Issue**: Filtering by consent chains is slow

**Problem**:
- Every query must check consent chain validity
- Chains can be deep (transitive sharing)
- Revocations cascade

**Current mitigation**:
- Indexed FK lookups
- Partial indexes on non-revoked chains

**V2 consideration**:
- Materialized view of accessible beliefs
- Incremental refresh on consent changes
- Or rethink sharing model

### 11.4 OAuth Complexity for MCP

**Issue**: OAuth is overkill for stdio MCP servers

**Problem**:
- MCP clients expect stdio, not HTTP
- OAuth flow breaks stdio model
- Workaround: Separate stdio and HTTP servers

**Current state**: Unified MCP server supports both, but HTTP requires OAuth

**V2 consideration**:
- MCP-specific auth (simpler than OAuth)
- Or accept stdio = local = no auth

### 11.5 Verification Bootstrap Problem

**Issue**: Need reputation to verify, but need to verify to earn reputation

**Problem**:
- New users can't verify (no reputation)
- Chicken-egg: How do you get initial reputation?

**Current mitigation**:
- System grants 10 initial reputation
- Low-stake verifications allowed

**V2 consideration**:
- Alternative onboarding (tutorials that earn reputation?)
- Or accept that verification is for established users

### 11.6 Belief Deduplication Edge Cases

**Issue**: Fuzzy deduplication (cosine > 0.90) sometimes misses duplicates

**Problem**:
- Embeddings for similar but distinct beliefs can be close
- Threshold tuning is domain-specific
- Content hash works for exact matches only

**Current mitigation**:
- Conservative threshold (0.90)
- Manual review via `tension_list`

**V2 consideration**:
- Smarter dedup (clustering? manual review UI?)
- Or accept some duplicates

### 11.7 Dimension Registry Adoption

**Issue**: Custom dimensions are powerful but rarely used

**Problem**:
- Most users stick with default dimensions
- Creating custom schemas requires code
- No UI for dimension management

**Evidence**: Almost zero custom dimensions in production

**V2 consideration**:
- Drop custom dimensions (simplify)?
- Or build better tooling for creating them

### 11.8 Trust Graph Complexity

**Issue**: Multiple trust systems (node_trust, trust_edges, user_node_trust)

**Problem**:
- Hard to understand which trust applies when
- Overlapping concerns (node trust vs. DID trust)
- Complex queries to compute effective trust

**V2 consideration**:
- Unify trust models
- Clearer separation of concerns
- Better documentation

### 11.9 Consensus Layer Thresholds

**Issue**: L2→L3→L4 thresholds are somewhat arbitrary

**Problem**:
- 5 contributors for L2 might be too low or too high
- 70% independence for L3 is hard to achieve
- No empirical validation of thresholds

**Current state**: Thresholds from spec, not tuned

**V2 consideration**:
- Gather data on real corroboration patterns
- Adjust thresholds empirically
- Or make thresholds configurable

### 11.10 VKB Exchange Compaction

**Issue**: Full exchange history grows unbounded

**Problem**:
- Long sessions accumulate thousands of exchanges
- Context window limits for resume
- Storage cost

**Current mitigation**:
- Compaction feature (#359) summarizes old exchanges
- Optional: truncate old exchanges after compaction

**V2 consideration**:
- Automatic compaction after N exchanges
- Smarter summarization (hierarchical?)

---

## 12. Lessons for v2

### 12.1 Architecture Lessons

**1. Keep substrate separate from VKB**
- Clean separation works well
- Don't merge into single table

**2. PostgreSQL is good enough for personal scale**
- Don't prematurely optimize with specialized DBs
- JSONB flexibility is valuable

**3. Modular bricks are double-edged**
- Reusability is good
- But dependency management is hard
- V2: Evaluate if bricks should merge back into monorepo

**4. Local-first is non-negotiable**
- Users want privacy and control
- External APIs must be optional

### 12.2 Data Model Lessons

**1. Dimensional confidence is worth the complexity**
- Keep in v2, but make UI clearer

**2. Supersession > mutation**
- History is valuable
- Keep full versioning

**3. Temporal validity is underused**
- v1 has `valid_from`/`valid_until` but rarely populated
- V2: Make temporal validity first-class (auto-detect?)

**4. Embeddings are essential but brittle**
- Local models evolve fast
- Need migration path between embedding models
- V2: Multi-model support from day one

### 12.3 Federation Lessons

**1. Simplify trust phases**
- Four phases are too many
- V2: Consider observer → participant → anchor (three)

**2. Conflict resolution needs better primitives**
- Vector clocks help but aren't enough
- V2: Investigate CRDTs for beliefs

**3. Privacy-preserving aggregation is hard**
- Differential privacy parameters are hard to tune
- V2: Provide better defaults and guidance

**4. P2P should be primary, HTTP fallback**
- libp2p is complex but worth it
- V2: Default to P2P, HTTP as compatibility layer

### 12.4 Verification Lessons

**1. Staking works but bootstrap is hard**
- V2: Better onboarding for new verifiers

**2. Calibration scoring is powerful**
- Keep Brier score approach
- V2: Monthly is too coarse, consider weekly

**3. Bounties are underutilized**
- V2: Make bounties more visible in UI

**4. Disputes are rare**
- Maybe that's good (verifications are quality)
- Or maybe disputes are too hard to file
- V2: Track dispute metrics

### 12.5 Consensus Lessons

**1. L1→L4 elevation is conceptually sound**
- But thresholds need tuning
- V2: Gather data, adjust empirically

**2. Corroboration independence is hard to measure**
- Evidential overlap is computable
- Methodological independence is subjective
- V2: Focus on evidential + temporal

**3. Challenges are rare**
- Same question as disputes: good or bad?
- V2: Make challenge process clearer

### 12.6 UX/DX Lessons

**1. MCP behavioral conditioning works**
- Keep tool descriptions detailed
- V2: Even more explicit guidance

**2. OAuth for MCP is awkward**
- V2: Separate auth model for stdio vs. HTTP

**3. Configurable ranking is good**
- Users like tuning semantic/confidence/recency
- V2: More preset profiles ("prefer recent" vs. "prefer confident")

**4. Confidence explanation is essential**
- Users need help interpreting multi-dimensional confidence
- V2: Better visualization

### 12.7 Performance Lessons

**1. HNSW index build is slow**
- Acceptable for personal scale
- V2: If targeting larger scale, need specialized vector store

**2. Consent chain filtering is expensive**
- V2: Materialized view or rethink sharing model

**3. Full-text search is fast**
- GIN indexes on tsvector work well
- V2: Keep this

### 12.8 Operational Lessons

**1. Schema migrations work well**
- Custom migration runner is simple and reliable
- V2: Keep this approach

**2. Docker deployment is smooth**
- docker-compose makes setup easy
- V2: Keep Docker-first

**3. Monitoring is lacking**
- V1 has Prometheus metrics but minimal dashboards
- V2: Pre-built Grafana dashboards

### 12.9 Testing Lessons

**1. High test coverage pays off**
- 6,300+ tests catch regressions
- V2: Maintain this standard

**2. Federation testing is hard**
- Multi-node tests are flaky
- V2: Better harness (testcontainers?)

**3. Integration tests are slow**
- Full database setup for each test
- V2: Explore in-memory PostgreSQL or fixtures

### 12.10 Community Lessons

**1. Documentation is never enough**
- V1 has 48 docs files but users still ask questions
- V2: More examples, tutorials

**2. API reference is essential**
- docs/API.md is most-referenced doc
- V2: Auto-generate from code

**3. Design docs are valuable**
- docs/design/* and specs explain *why*
- V2: Keep writing these

---

## 13. Implementation Status Summary

### 13.1 Fully Implemented

✅ **Core substrate**: Beliefs, entities, tensions  
✅ **Dimensional confidence**: 6D confidence + extensible schema  
✅ **VKB**: Sessions, exchanges, patterns, insights  
✅ **Embeddings**: Local (bge-small) + OpenAI support  
✅ **MCP server**: 58 tools, stdio + HTTP  
✅ **OAuth 2.1**: Full implementation with PKCE  
✅ **Verification protocol**: Stake, dispute, reputation  
✅ **Consensus mechanism**: L1→L4 elevation, corroboration  
✅ **Incentive system**: Calibration, bounties, velocity limits  
✅ **Sharing**: Consent chains, trust-gated access  
✅ **Federation (HTTP)**: Node discovery, sync, aggregation  
✅ **GDPR compliance**: Export, erasure, tombstones  
✅ **Backup**: Erasure coding (Reed-Solomon)  
✅ **CLI**: Full command-line interface  
✅ **Health checks**: Startup validation  

### 13.2 Partially Implemented

🟡 **Federation (P2P)**: libp2p transport exists but not default  
🟡 **Browser automation**: Extractor learning framework (minimal usage)  
🟡 **Exchange compaction**: Feature exists but not automatic  
🟡 **Temporal validity**: Schema support but underutilized  
🟡 **Custom dimensions**: Registry exists but rarely used  
🟡 **Aggregation**: Privacy-preserving aggregation implemented but untested at scale  

### 13.3 Not Implemented (Future)

❌ **Rust transport**: Listed as future work  
❌ **Auto-ingestion**: From conversations (manual via MCP tools)  
❌ **Browser client**: API exists but no web UI  
❌ **Network governance**: Transition plan not yet executed  
❌ **Multi-tenant**: Designed for single-user  

---

## 14. Dependencies and Infrastructure

### 14.1 Python Dependencies

**Core requirements** (from pyproject.toml):
- `mcp>=1.0` — Model Context Protocol
- `openai>=1.0` — Embeddings (optional)
- `psycopg2-binary>=2.9` — PostgreSQL driver
- `asyncpg>=0.29.0` — Async PostgreSQL
- `numpy>=1.24` — Array operations
- `pgvector>=0.2` — Vector extension
- `sentence-transformers>=2.2.0` — Local embeddings
- `starlette>=0.40.0` — HTTP server framework
- `uvicorn[standard]>=0.30.0` — ASGI server
- `pydantic-settings>=2.0.0` — Configuration
- `PyJWT>=2.11.0` — JWT tokens
- `PyYAML>=6.0` — Config files
- `httpx>=0.27.0` — HTTP client
- `aiohttp>=3.9.0` — Async HTTP
- `dnspython>=2.4.0` — DNS lookups
- `cryptography>=42.0` — Crypto primitives

**Brick dependencies**:
- `our-db>=0.1.0`
- `our-models>=0.1.0`
- `our-confidence>=0.1.0`
- `our-crypto>=0.1.0`
- `our-identity>=0.1.0`
- `our-consensus>=0.1.0`
- `our-storage>=0.1.0`
- `our-mcp-base>=0.1.0`
- `our-compliance>=0.1.0`
- `our-network>=0.1.0`
- `our-embeddings>=0.1.0`
- `our-privacy>=0.1.0`
- `our-federation>=0.1.0`

**Optional**:
- `redis>=4.5` (caching)
- `libp2p>=0.5.0` (P2P networking)

### 14.2 PostgreSQL Setup

**Requirements**:
- PostgreSQL 16+
- Extensions: `uuid-ossp`, `vector` (pgvector)

**Installation** (macOS):
```bash
brew install postgresql@16 pgvector
```

**Schema size**: ~59KB SQL for initial schema

### 14.3 Docker Deployment

**docker-compose.yml**:
```yaml
services:
  db:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: valence
      POSTGRES_USER: valence
      POSTGRES_PASSWORD: valence
    volumes:
      - pgdata:/var/lib/postgresql/data

  valence:
    build: .
    depends_on:
      - db
    environment:
      DATABASE_URL: postgresql://valence:valence@db:5432/valence
      OPENAI_API_KEY: ${OPENAI_API_KEY:-}
    ports:
      - "8420:8420"
```

**Dockerfile**: Multi-stage build
1. Build stage: Install dependencies
2. Runtime stage: Copy app + lean runtime

### 14.4 Configuration

**Environment variables**:
- `DATABASE_URL` — PostgreSQL connection string
- `OPENAI_API_KEY` — OpenAI API key (optional)
- `EMBEDDING_PROVIDER` — local or openai (default: local)
- `EMBEDDING_MODEL` — Model name
- `FEDERATION_ENABLED` — Enable federation (default: false)
- `FEDERATION_DID` — Node DID
- `FEDERATION_ENDPOINT` — Public endpoint
- `OAUTH_SECRET_KEY` — OAuth secret (generated if missing)
- `LOG_LEVEL` — Logging level (default: INFO)

**Config file** (`~/.valence/config.yml`):
```yaml
database:
  url: postgresql://localhost/valence

embedding:
  provider: local
  model: bge-small-en-v1.5

federation:
  enabled: false
  did: did:vkb:web:localhost

server:
  host: 0.0.0.0
  port: 8420
```

### 14.5 CLI Commands

**Core commands**:
- `valence init` — Initialize database
- `valence add <content>` — Add belief
- `valence query <text>` — Search beliefs
- `valence list` — List recent beliefs
- `valence stats` — Database stats

**Server commands**:
- `valence-server` — Start HTTP server
- `valence-mcp` — Start unified MCP server (stdio)

**Migration commands**:
- `valence migrate up` — Apply migrations
- `valence migrate down` — Rollback
- `valence migrate to <n>` — Migrate to version

**Federation commands**:
- `valence-federation discover` — Discover peers
- `valence-federation sync` — Sync with peer

**Admin commands**:
- `valence-token create` — Create OAuth token
- `valence-token list` — List tokens

### 14.6 Development Setup

**Install editable**:
```bash
git clone https://github.com/ourochronos/valence.git
cd valence
python -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

**Run tests**:
```bash
pytest                    # All tests
pytest -m unit            # Unit tests only
pytest -m integration     # Integration tests (needs DB)
pytest --cov              # With coverage
```

**Linting**:
```bash
ruff check src/
black src/
mypy src/
```

**Pre-commit**:
```bash
./scripts/check  # Runs lint + tests
```

---

## 15. Code Statistics

### 15.1 Python Source Files

**Total**: 133 .py files

**By module**:
- `core/`: 39 files
- `substrate/tools/`: 15 files
- `server/`: 25 files
- `cli/`: 13 files
- `vkb/tools/`: 10 files
- `transport/`: 11 files
- `compliance/`: 7 files

### 15.2 Lines of Code

**Estimated** (excluding tests, docs, comments):
- Core modules: ~15,000 lines
- Substrate: ~8,000 lines
- Server: ~6,000 lines
- VKB: ~3,000 lines
- CLI: ~2,000 lines
- Transport: ~3,000 lines
- Total: ~37,000 lines of Python

### 15.3 Documentation

**Markdown files**: 48 docs

**Key docs**:
- `README.md` — Overview
- `docs/PRINCIPLES.md` — Core principles
- `docs/TRUST_MODEL.md` — Trust specification
- `docs/FEDERATION_PROTOCOL.md` — Federation spec
- `docs/API.md` — API reference
- `docs/design/architecture.md` — Architecture
- `spec/` — 8 specification directories

### 15.4 Tests

**Test files**:
- Valence core: 2,300+ tests
- Brick packages: 4,000+ tests
- Total: 6,300+ tests

**Coverage**: ~85% (core modules)

### 15.5 Database Schema

**SQL files**:
- `substrate/schema.sql`: 1,617 lines
- `substrate/procedures.sql`: 18KB
- `migrations/`: 2,185 lines total (22 migration files)

**Tables**: 40+ tables

---

## Conclusion

Valence v1 is a **comprehensive, production-ready personal knowledge substrate** with:

✅ Solid architecture (PostgreSQL + pgvector, modular design)  
✅ Rich data model (beliefs, entities, tensions, sessions, patterns)  
✅ Dimensional confidence system (6+ dimensions, extensible)  
✅ Multi-dimensional trust (epistemic, not social)  
✅ Verification protocol (stake-based quality control)  
✅ Consensus mechanism (L1→L4 elevation)  
✅ Federation support (HTTP primary, P2P optional)  
✅ Incentive system (calibration, bounties, velocity)  
✅ 58 MCP tools (comprehensive agent interface)  
✅ GDPR compliance (export, erasure, consent chains)  
✅ Strong test coverage (6,300+ tests)  

**Key strengths**:
- Local-first with privacy by default
- Dimensional confidence makes uncertainty explicit
- Verification protocol ensures quality
- Federation preserves user sovereignty

**Key challenges**:
- Federation complexity (many edge cases)
- Embedding performance at scale (HNSW index builds)
- Consent chain filtering performance
- Verification bootstrap problem

**For v2**:
- Simplify trust phases (4 → 3)
- Make P2P primary, HTTP fallback
- Improve federation testing
- Better UX for dimensional confidence
- Consider materializing consent-filtered views
- Tune consensus thresholds empirically

This comprehensive analysis should provide all the architectural knowledge needed to inform Valence v2 design decisions while building on v1's solid foundation.

---

**End of v1 Architecture Analysis**
