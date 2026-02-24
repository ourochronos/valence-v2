-- Valence v2 Initial Schema
-- PostgreSQL 16+
-- All UUIDs, all TIMESTAMPTZ, JSONB for flexible fields

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- CORE KNOWLEDGE GRAPH
-- ============================================================================

CREATE TABLE IF NOT EXISTS nodes (
    id UUID PRIMARY KEY,
    value TEXT NOT NULL,
    node_type TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    access_count BIGINT NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_value ON nodes(value);

CREATE TABLE IF NOT EXISTS triples (
    id UUID PRIMARY KEY,
    subject_id UUID NOT NULL REFERENCES nodes(id),
    predicate TEXT NOT NULL,
    object_id UUID NOT NULL REFERENCES nodes(id),
    origin_did TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signature BYTEA,
    base_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    local_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    access_count BIGINT NOT NULL DEFAULT 0,
    last_accessed TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_triples_spo ON triples(subject_id, predicate, object_id);
CREATE INDEX IF NOT EXISTS idx_triples_pos ON triples(predicate, object_id, subject_id);
CREATE INDEX IF NOT EXISTS idx_triples_osp ON triples(object_id, subject_id, predicate);
CREATE INDEX IF NOT EXISTS idx_triples_origin ON triples(origin_did) WHERE origin_did IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_triples_local_weight ON triples(local_weight);

CREATE TABLE IF NOT EXISTS sources (
    id UUID PRIMARY KEY,
    source_type TEXT NOT NULL,
    reference TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB
);
CREATE INDEX IF NOT EXISTS idx_sources_type ON sources(source_type);

CREATE TABLE IF NOT EXISTS source_triples (
    source_id UUID NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    triple_id UUID NOT NULL REFERENCES triples(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, triple_id)
);
CREATE INDEX IF NOT EXISTS idx_source_triples_triple ON source_triples(triple_id);

-- ============================================================================
-- VKB: CONVERSATION TRACKING
-- ============================================================================

CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform TEXT NOT NULL,
    project_context TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    summary TEXT,
    themes TEXT[] DEFAULT '{}',
    claude_session_id TEXT,
    external_room_id TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,

    CONSTRAINT sessions_valid_status CHECK (status IN ('active', 'completed', 'abandoned')),
    CONSTRAINT sessions_valid_platform CHECK (platform IN (
        'claude-code', 'api', 'slack', 'claude-web', 'claude-desktop', 'claude-mobile', 'matrix'
    ))
);
CREATE INDEX IF NOT EXISTS idx_sessions_platform ON sessions(platform);
CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
CREATE INDEX IF NOT EXISTS idx_sessions_created ON sessions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_context);
CREATE INDEX IF NOT EXISTS idx_sessions_external_room ON sessions(external_room_id);
CREATE INDEX IF NOT EXISTS idx_sessions_claude_id ON sessions(claude_session_id) WHERE claude_session_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS exchanges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    tokens_approx INTEGER,
    tool_uses TEXT[] DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT exchanges_valid_role CHECK (role IN ('user', 'assistant', 'system'))
);
CREATE INDEX IF NOT EXISTS idx_exchanges_session ON exchanges(session_id);
CREATE INDEX IF NOT EXISTS idx_exchanges_created ON exchanges(created_at DESC);

CREATE TABLE IF NOT EXISTS patterns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pattern_type TEXT NOT NULL,
    description TEXT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    evidence_session_ids UUID[] DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'emerging',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT patterns_valid_confidence CHECK (confidence >= 0.0 AND confidence <= 1.0),
    CONSTRAINT patterns_valid_status CHECK (status IN ('emerging', 'established', 'fading', 'archived'))
);
CREATE INDEX IF NOT EXISTS idx_patterns_type ON patterns(pattern_type);
CREATE INDEX IF NOT EXISTS idx_patterns_status ON patterns(status);

CREATE TABLE IF NOT EXISTS insights (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    triple_ids UUID[] DEFAULT '{}',
    domain_path TEXT[] DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_insights_session ON insights(session_id);
CREATE INDEX IF NOT EXISTS idx_insights_domain ON insights USING GIN (domain_path);
