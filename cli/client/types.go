package client

import "time"

// TripleInput represents a triple to be inserted
type TripleInput struct {
	Subject   string `json:"subject"`
	Predicate string `json:"predicate"`
	Object    string `json:"object"`
}

// SourceInput represents source information for triples
type SourceInput struct {
	Type      string  `json:"type"`
	Reference *string `json:"reference,omitempty"`
}

// InsertTriplesRequest is the request for inserting triples
type InsertTriplesRequest struct {
	Triples []TripleInput `json:"triples"`
	Source  *SourceInput  `json:"source,omitempty"`
}

// InsertTriplesResponse is the response from inserting triples
type InsertTriplesResponse struct {
	TripleIDs []string `json:"triple_ids"`
	SourceID  *string  `json:"source_id,omitempty"`
}

// NodeResponse represents a node in the graph
type NodeResponse struct {
	ID    string `json:"id"`
	Value string `json:"value"`
}

// SourceResponse represents source information
type SourceResponse struct {
	ID        string    `json:"id"`
	Type      string    `json:"type"`
	Reference *string   `json:"reference,omitempty"`
	CreatedAt time.Time `json:"created_at"`
}

// TripleResponse represents a triple in a query response
type TripleResponse struct {
	ID           string           `json:"id"`
	Subject      NodeResponse     `json:"subject"`
	Predicate    string           `json:"predicate"`
	Object       NodeResponse     `json:"object"`
	Weight       float64          `json:"weight"`
	CreatedAt    time.Time        `json:"created_at"`
	LastAccessed time.Time        `json:"last_accessed"`
	AccessCount  uint64           `json:"access_count"`
	Sources      []SourceResponse `json:"sources,omitempty"`
}

// QueryTriplesResponse is the response from querying triples
type QueryTriplesResponse struct {
	Triples []TripleResponse `json:"triples"`
}

// NeighborsResponse is the response from the neighbors endpoint
type NeighborsResponse struct {
	Triples     []TripleResponse `json:"triples"`
	NodeCount   int              `json:"node_count"`
	TripleCount int              `json:"triple_count"`
}

// SourcesResponse is the response from the sources endpoint
type SourcesResponse struct {
	Sources []SourceResponse `json:"sources"`
}

// SearchRequest is the request for semantic search
type SearchRequest struct {
	QueryNode           string  `json:"query_node"`
	K                   int     `json:"k"`
	IncludeConfidence   bool    `json:"include_confidence"`
	UseTiered           bool    `json:"use_tiered"`
	BudgetMs            *uint64 `json:"budget_ms,omitempty"`
	ConfidenceThreshold *float64 `json:"confidence_threshold,omitempty"`
}

// SearchResult represents a single search result
type SearchResult struct {
	NodeID     string   `json:"node_id"`
	Value      string   `json:"value"`
	Similarity float32  `json:"similarity"`
	Confidence *float64 `json:"confidence,omitempty"`
}

// SearchResponse is the response from search
type SearchResponse struct {
	Results         []SearchResult `json:"results"`
	TierReached     *uint8         `json:"tier_reached,omitempty"`
	TimeMs          *uint64        `json:"time_ms,omitempty"`
	BudgetExhausted *bool          `json:"budget_exhausted,omitempty"`
	Fallback        *bool          `json:"fallback,omitempty"`
}

// StatsResponse is the response from the stats endpoint
type StatsResponse struct {
	TripleCount uint64  `json:"triple_count"`
	NodeCount   uint64  `json:"node_count"`
	AvgWeight   float64 `json:"avg_weight"`
}

// DecayRequest is the request for triggering decay
type DecayRequest struct {
	Factor    float64 `json:"factor"`
	MinWeight float64 `json:"min_weight"`
}

// DecayResponse is the response from decay
type DecayResponse struct {
	AffectedCount uint64 `json:"affected_count"`
}

// EvictRequest is the request for evicting low-weight triples
type EvictRequest struct {
	Threshold float64 `json:"threshold"`
}

// EvictResponse is the response from eviction
type EvictResponse struct {
	EvictedCount uint64 `json:"evicted_count"`
}

// RecomputeEmbeddingsRequest is the request for recomputing embeddings
type RecomputeEmbeddingsRequest struct {
	Dimensions int `json:"dimensions"`
}

// RecomputeEmbeddingsResponse is the response from recomputing embeddings
type RecomputeEmbeddingsResponse struct {
	EmbeddingCount int `json:"embedding_count"`
}

// ErrorResponse represents an API error response
type ErrorResponse struct {
	Error string `json:"error"`
}

// ========== VKB Types ==========

// SessionStartRequest is the request for starting a session
type SessionStartRequest struct {
	Platform       string                 `json:"platform"`
	ProjectContext *string                `json:"project_context,omitempty"`
	ExternalRoomID *string                `json:"external_room_id,omitempty"`
	Metadata       map[string]interface{} `json:"metadata,omitempty"`
}

// SessionStartResponse is the response from starting a session
type SessionStartResponse struct {
	ID        string `json:"id"`
	Status    string `json:"status"`
	CreatedAt string `json:"created_at"`
}

// SessionResponse represents a session in API responses
type SessionResponse struct {
	ID             string   `json:"id"`
	Platform       string   `json:"platform"`
	Status         string   `json:"status"`
	ProjectContext *string  `json:"project_context,omitempty"`
	ExternalRoomID *string  `json:"external_room_id,omitempty"`
	CreatedAt      string   `json:"created_at"`
	EndedAt        *string  `json:"ended_at,omitempty"`
	Summary        *string  `json:"summary,omitempty"`
	Themes         []string `json:"themes,omitempty"`
}

// SessionEndRequest is the request for ending a session
type SessionEndRequest struct {
	Summary *string  `json:"summary,omitempty"`
	Themes  []string `json:"themes,omitempty"`
	Status  *string  `json:"status,omitempty"`
}

// ========== Trust Types ==========

// TrustQueryResponse is the response from a trust query
type TrustQueryResponse struct {
	DID           string          `json:"did"`
	TrustScore    float64         `json:"trust_score"`
	ConnectedDIDs []TrustedEntity `json:"connected_dids"`
}

// TrustedEntity represents a trusted DID with its score
type TrustedEntity struct {
	DID        string  `json:"did"`
	TrustScore float64 `json:"trust_score"`
}
