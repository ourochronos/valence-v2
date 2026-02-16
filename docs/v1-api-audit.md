# Valence v1 API Surface Audit

**Date:** 2026-02-16  
**Purpose:** Catalog all v1 endpoints/tools to map to v2 triples-based model

## Summary

Valence v1 exposes **56 MCP tools** organized into 8 major categories:
- 9 Belief CRUD operations
- 2 Entity operations  
- 2 Tension/conflict operations
- 9 Trust & verification operations
- 12 Incentive & reputation operations
- 9 Consensus mechanism operations
- 4 Backup/resilience operations
- 9 Session/conversation tracking (VKB) operations

## Detailed Tool Catalog

| Tool Name | Category | Purpose | Key Parameters | Operates On | Maps to Triples | Core/Derived |
|-----------|----------|---------|----------------|-------------|-----------------|--------------|
| **belief_query** | Belief CRUD | Hybrid search (keyword + semantic) | query, domain_filter, entity_id, include_superseded, include_revoked, include_archived, include_expired, limit, ranking | beliefs | **Becomes simpler** - Query triples by subject/predicate/object with filters | **Core** |
| **belief_create** | Belief CRUD | Create new belief with entity links | content, confidence, domain_path, source_type, source_ref, opt_out_federation, entities, visibility, sharing_intent | beliefs, entities | **Becomes simpler** - Insert triples (subject, predicate, object) + metadata triples for confidence/source | **Core** |
| **belief_supersede** | Belief CRUD | Replace old belief with new, maintain history | old_belief_id, new_content, reason, confidence | beliefs | **Maps directly** - Create new triple + provenance triple linking to old | **Core** |
| **belief_get** | Belief CRUD | Get single belief by ID with full details | belief_id, include_history, include_tensions | beliefs, tensions | **Stays same** - Query all triples with belief ID as subject | **Core** |
| **belief_search** | Belief CRUD | Semantic vector search | query, min_similarity, min_confidence, domain_filter, include_archived, limit, ranking | beliefs, embeddings | **Becomes simpler** - Vector search on object embeddings, rank by graph topology | **Derived** (uses embeddings) |
| **belief_corroboration** | Belief CRUD | Get corroboration details (how many sources confirm) | belief_id | beliefs, corroborations | **Stays same** - Query corroboration triples | **Derived** (computed from verification triples) |
| **belief_share** | Belief CRUD | Share belief with person via DID | belief_id, recipient_did, intent, max_hops, expires_at | beliefs, shares, consent_chains | **Maps to triples** - Create share triple + consent chain triples | **Core** |
| **belief_shares_list** | Belief CRUD | List outgoing or incoming shares | direction, belief_id, include_revoked, limit | shares | **Stays same** - Query share triples filtered by direction | **Derived** |
| **belief_share_revoke** | Belief CRUD | Revoke a share | share_id, reason | shares, consent_chains | **Maps directly** - Update share triple status to revoked | **Core** |
| **entity_get** | Entity Ops | Get entity with optional beliefs | entity_id, include_beliefs, belief_limit | entities, beliefs | **Becomes simpler** - Query all triples with entity as subject/object | **Core** |
| **entity_search** | Entity Ops | Find entities by name or type | query, type, limit | entities | **Stays same** - Query entity triples by name/type predicates | **Derived** |
| **tension_list** | Tension/Conflict | List contradictions between beliefs | status, severity, entity_id, limit | tensions, beliefs | **Computed** - Detect from conflicting triples (same subject/predicate, different objects) | **Derived** (dynamic detection) |
| **tension_resolve** | Tension/Conflict | Mark tension as resolved | tension_id, resolution, action | tensions, beliefs | **Maps to triples** - Create resolution triple + update belief status | **Core** |
| **trust_check** | Trust | Check trust levels for entities/nodes on topic | topic, entity_name, include_federated, min_trust, limit, domain | entities, federation_nodes, node_trust | **Computed** - Derive from graph topology (PageRank-style on verification/corroboration edges) | **Derived** (topology-based) |
| **confidence_explain** | Trust | Explain confidence score breakdown | belief_id | beliefs, confidence | **Computed** - Analyze incoming/outgoing verification edges, source diversity | **Derived** (topology-based) |
| **verification_submit** | Verification | Submit verification for belief (stake reputation) | belief_id, verifier_id, result, evidence, stake_amount, reasoning, result_details | verifications, beliefs | **Maps to triples** - Create verification triple with evidence edges | **Core** |
| **verification_accept** | Verification | Accept pending verification, trigger reputation update | verification_id | verifications, reputation | **Core** - Update verification triple status, create reputation transfer triples | **Core** |
| **verification_get** | Verification | Get verification details | verification_id | verifications | **Stays same** - Query verification triples | **Core** |
| **verification_list** | Verification | List verifications for belief | belief_id | verifications | **Stays same** - Query verification triples by belief | **Derived** |
| **verification_summary** | Verification | Summarize verification activity for belief | belief_id | verifications | **Computed** - Aggregate verification triples, weight by reputation | **Derived** |
| **dispute_submit** | Verification | Submit dispute against verification | verification_id, disputer_id, counter_evidence, stake_amount, dispute_type, reasoning, proposed_result | disputes, verifications | **Maps to triples** - Create dispute triple with counter-evidence edges | **Core** |
| **dispute_resolve** | Verification | Resolve pending dispute | dispute_id, outcome, resolution_reasoning, resolution_method | disputes, reputation | **Core** - Update dispute triple, create reputation transfer triples | **Core** |
| **dispute_get** | Verification | Get dispute details | dispute_id | disputes | **Stays same** - Query dispute triples | **Core** |
| **reputation_get** | Reputation | Get reputation score for identity | identity_id | reputation | **Computed** - Aggregate reputation transfer triples, calculate domain-specific scores from topology | **Derived** (topology-based) |
| **reputation_events** | Reputation | Get reputation event history | identity_id, limit | reputation_events | **Stays same** - Query reputation transfer triples ordered by time | **Derived** |
| **bounty_get** | Incentives | Get discrepancy bounty for belief | belief_id | bounties, beliefs | **Computed** - Calculate from belief confidence + existing verifications | **Derived** |
| **bounty_list** | Incentives | List available discrepancy bounties | unclaimed_only, limit | bounties, beliefs | **Computed** - Query high-confidence beliefs, calculate bounties | **Derived** |
| **calibration_run** | Incentives | Run calibration scoring (Brier score) | identity_id, period_start | calibration, beliefs, verifications | **Computed** - Analyze prediction accuracy from belief confidence vs verification outcomes | **Derived** |
| **calibration_history** | Incentives | Get calibration score history | identity_id, limit | calibration | **Stays same** - Query calibration snapshot triples | **Derived** |
| **rewards_pending** | Incentives | Get unclaimed rewards | identity_id | rewards | **Stays same** - Query reward triples with status=pending | **Derived** |
| **reward_claim** | Incentives | Claim single pending reward | reward_id | rewards, reputation | **Core** - Update reward triple status, create reputation transfer triple | **Core** |
| **rewards_claim_all** | Incentives | Claim all pending rewards (up to velocity limit) | identity_id | rewards, reputation, velocity | **Core** - Batch update reward triples, create reputation transfers | **Core** |
| **transfer_history** | Incentives | Get reputation transfer history | identity_id, direction, limit | reputation_transfers | **Stays same** - Query reputation transfer triples | **Derived** |
| **velocity_status** | Incentives | Get velocity status (rate limiting) | identity_id | velocity | **Computed** - Aggregate recent reputation transfers, check against limits | **Derived** |
| **consensus_status** | Consensus | Get consensus status (trust layer L1-L4) | belief_id | beliefs, consensus, corroborations, challenges | **Computed** - Analyze corroboration triples, calculate trust layer from topology | **Derived** (topology-based) |
| **corroboration_submit** | Consensus | Submit corroboration between beliefs | primary_belief_id, corroborating_belief_id, primary_holder, corroborator, semantic_similarity, evidence_sources_a, evidence_sources_b, corroborator_reputation | corroborations, beliefs | **Maps to triples** - Create corroboration edge between belief triples | **Core** |
| **corroboration_list** | Consensus | List corroborations for belief | belief_id | corroborations | **Stays same** - Query corroboration triples | **Derived** |
| **challenge_submit** | Consensus | Challenge belief's consensus status | belief_id, challenger_id, reasoning, evidence, stake_amount | challenges, beliefs | **Maps to triples** - Create challenge triple with evidence edges | **Core** |
| **challenge_resolve** | Consensus | Resolve pending challenge | challenge_id, upheld, resolution_reasoning | challenges, beliefs, reputation | **Core** - Update challenge triple, potentially update belief trust layer | **Core** |
| **challenge_get** | Consensus | Get challenge details | challenge_id | challenges | **Stays same** - Query challenge triples | **Core** |
| **challenges_list** | Consensus | List challenges for belief | belief_id | challenges | **Stays same** - Query challenge triples by belief | **Derived** |
| **backup_create** | Backup | Create erasure-coded backup | redundancy, domain_filter, min_confidence, encrypt | beliefs, backups | **Same mechanism** - Serialize triples, create backup shards | **Infrastructure** |
| **backup_verify** | Backup | Verify backup integrity | backup_set_id | backups | **Same mechanism** - Check shard checksums | **Infrastructure** |
| **backup_list** | Backup | List backup sets | limit | backups | **Stays same** - Query backup set triples | **Derived** |
| **backup_get** | Backup | Get backup set details | backup_set_id | backups | **Stays same** - Query backup set triples | **Core** |
| **session_start** | VKB/Session | Begin conversation session | platform, project_context, external_room_id, claude_session_id, metadata | sessions | **Maps to triples** - Create session triple with context metadata | **Core** |
| **session_end** | VKB/Session | Close session with summary | session_id, summary, themes, status | sessions | **Maps to triples** - Update session triple with summary/themes | **Core** |
| **session_get** | VKB/Session | Get session details | session_id, include_exchanges, exchange_limit | sessions, exchanges | **Stays same** - Query session triples + linked exchange triples | **Core** |
| **session_list** | VKB/Session | List sessions with filters | platform, project_context, status, limit | sessions | **Stays same** - Query session triples with filters | **Derived** |
| **session_find_by_room** | VKB/Session | Find active session by room ID | external_room_id | sessions | **Stays same** - Query session triples by room ID predicate | **Derived** |
| **exchange_add** | VKB/Exchange | Record conversation turn | session_id, role, content, tokens_approx, tool_uses | exchanges, sessions | **Maps to triples** - Create exchange triple linked to session | **Core** |
| **exchange_list** | VKB/Exchange | Get exchanges from session | session_id, limit, offset | exchanges | **Stays same** - Query exchange triples by session | **Derived** |
| **pattern_record** | VKB/Pattern | Record behavioral pattern | type, description, evidence, confidence | patterns, sessions | **Maps to triples** - Create pattern triple with evidence edges to sessions | **Core** |
| **pattern_reinforce** | VKB/Pattern | Strengthen pattern with new evidence | pattern_id, session_id | patterns, sessions | **Maps to triples** - Add evidence edge to pattern triple | **Core** |
| **pattern_list** | VKB/Pattern | List patterns with filters | type, status, min_confidence, limit | patterns | **Stays same** - Query pattern triples with filters | **Derived** |
| **pattern_search** | VKB/Pattern | Search patterns by description | query, limit | patterns | **Stays same** - Full-text or semantic search on pattern triples | **Derived** |
| **insight_extract** | VKB/Insight | Extract insight from session → create belief | session_id, content, domain_path, confidence, entities | sessions, beliefs, entities | **Maps to triples** - Create belief triples with provenance edge to session | **Core** |
| **insight_list** | VKB/Insight | List insights from session | session_id | beliefs, sessions | **Stays same** - Query belief triples linked to session | **Derived** |

## CLI Commands (Additional Surface)

The CLI exposes these high-level commands that map to tool combinations:

| CLI Command | Purpose | Maps to Tools |
|-------------|---------|---------------|
| `valence init` | Initialize database schema | Schema operations (infrastructure) |
| `valence add` | Add belief (simple wrapper) | belief_create |
| `valence query` | Search beliefs | belief_query |
| `valence list` | List recent beliefs | belief_query (with recent sort) |
| `valence conflicts` | Detect contradictions | tension_list |
| `valence stats` | Show database stats | Resource queries (aggregate) |
| `valence discover` | Network router discovery | Federation (not MCP tool) |
| `valence peer add/list/remove` | Manage federation peers | Federation management |
| `valence export/import` | Import/export beliefs | Serialization + belief_create/belief_query |
| `valence trust` | Check trust levels | trust_check |
| `valence embeddings` | Manage embeddings | Infrastructure (embedding generation) |
| `valence attestations` | Manage attestations | verification_submit, verification_list |
| `valence resources` | Manage resources | Resource queries |
| `valence migrate` | Schema migrations | Infrastructure |
| `valence schema` | Schema operations | Infrastructure |
| `valence qos` | Quality of service metrics | Monitoring/metrics |
| `valence identity` | Identity management | DID operations |
| `valence maintenance` | Maintenance tasks | Infrastructure |

## Resources (Read-Only Endpoints)

| Resource URI | Purpose | Maps to Triples |
|--------------|---------|-----------------|
| `valence://beliefs/recent` | Recent beliefs | Query belief triples sorted by creation time |
| `valence://trust/graph` | Trust relationships | Compute from verification/corroboration edges |
| `valence://stats` | Database statistics | Aggregate counts from all triple types |

## Mapping Analysis

### Becomes Simpler (10 tools)
- belief_query, belief_create, belief_get, entity_get
- Structured query language over triples is more direct than complex SQL

### Stays Same (18 tools)
- Most read operations (lists, searches, gets) map 1:1 to triple queries
- Share/dispute/challenge operations already have graph-like semantics

### Computed/Derived (15 tools)
Key insight: **Many v1 "tools" are actually derived views that will become graph queries in v2:**
- **trust_check** → PageRank-style on verification graph
- **confidence_explain** → Analyze incoming verification edges + source diversity
- **reputation_get** → Aggregate reputation transfer triples
- **consensus_status** → Calculate trust layer from corroboration topology
- **tension_list** → Detect conflicting triples dynamically
- **bounty calculations** → Function of confidence + verification topology

### Core Operations (23 tools)
These are the atomic write operations that directly manipulate triples:
- Create/update/delete triples (belief_create, belief_supersede, etc.)
- Create relationships (belief_share, verification_submit, corroboration_submit)
- Update status (dispute_resolve, challenge_resolve, verification_accept)

## Migration Strategy

### Phase 1: Core Triples Engine
**Required for v2 launch:**
- Triple insert/query/delete (replaces belief_create, belief_query, belief_get)
- Provenance edges (supersession chains, source references)
- Entity linking (subject/object resolution)
- Basic filtering (status, timestamps, domains)

### Phase 2: Topology-Derived Computation
**Enables most "computed" tools:**
- PageRank/trust scores from verification graph
- Dynamic confidence from corroboration topology
- Tension detection (conflicting triples)
- Consensus layer calculation

### Phase 3: Incentive & Verification Layer
**Advanced game theory features:**
- Reputation transfer triples
- Verification/dispute workflows
- Calibration scoring
- Velocity limits

### Phase 4: Federation & VKB
**Multi-node and conversation features:**
- Session/exchange triples
- Pattern detection
- Insight extraction
- Share/consent chains

## Key Insights for v2

1. **Confidence becomes emergent:** In v1, confidence is stored as JSON. In v2, confidence is computed dynamically from the verification graph topology, corroboration edges, and source diversity.

2. **Trust is graph-based:** trust_check should use graph centrality (PageRank, HITS) on the verification/corroboration graph rather than stored trust scores.

3. **Many "tools" are just views:** 15 of the 56 tools are read-only aggregate/computed views. In v2, these become standard graph queries or materialized views.

4. **Triple types emerge from v1 structure:**
   - Belief triples: (subject, predicate, object)
   - Provenance triples: (belief_id, supersedes, old_belief_id)
   - Verification triples: (verifier, verifies, belief_id)
   - Corroboration triples: (belief_a, corroborates, belief_b)
   - Evidence triples: (verification, has_evidence, source_uri)
   - Entity triples: (entity_id, is_type, "person")
   - Reputation triples: (identity, has_reputation, score)
   - Session triples: (session_id, has_platform, "claude-code")

5. **Embeddings shift to topology:** v1 uses explicit embeddings for belief_search. v2 should derive embeddings from graph topology (node2vec, graph neural nets) + optional content embeddings.

## Open Questions for v2

1. **Storage engine:** PostgreSQL with triple table? Native triple store (RDF4J, Jena)? Graph DB (Neo4j, DGraph)?
2. **Embedding strategy:** Pure topology-derived? Hybrid with content embeddings?
3. **Materialized views:** Which computed values should be cached vs recomputed?
4. **Query language:** SPARQL? Cypher? Custom?
5. **Federation protocol:** How do triples sync between nodes?
6. **Backward compatibility:** Bridge layer for v1 API? Deprecation timeline?

---

**Next Steps:**
- [ ] Choose storage engine (ADR)
- [ ] Design core triple schema
- [ ] Implement basic insert/query
- [ ] Prototype topology-based confidence
- [ ] Design v2 MCP API (backward compatible where possible)
- [ ] Migration tooling (v1 beliefs → v2 triples)
