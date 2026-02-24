package client

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

// Client wraps the HTTP API for Valence v2
type Client struct {
	BaseURL    string
	AuthToken  string
	HTTPClient *http.Client
}

// NewClient creates a new Valence API client
func NewClient(baseURL, authToken string) *Client {
	return &Client{
		BaseURL:    strings.TrimRight(baseURL, "/"),
		AuthToken:  authToken,
		HTTPClient: &http.Client{},
	}
}

// doRequest performs an HTTP request and handles errors
func (c *Client) doRequest(method, path string, body interface{}, result interface{}) error {
	var reqBody io.Reader
	if body != nil {
		jsonData, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("failed to marshal request: %w", err)
		}
		reqBody = bytes.NewBuffer(jsonData)
	}

	req, err := http.NewRequest(method, c.BaseURL+path, reqBody)
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")
	if c.AuthToken != "" {
		req.Header.Set("Authorization", "Bearer "+c.AuthToken)
	}

	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return fmt.Errorf("request failed: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to read response: %w", err)
	}

	if resp.StatusCode >= 400 {
		var errResp ErrorResponse
		if err := json.Unmarshal(respBody, &errResp); err == nil {
			return fmt.Errorf("API error: %s", errResp.Error)
		}
		return fmt.Errorf("API error: %s (status %d)", string(respBody), resp.StatusCode)
	}

	if result != nil {
		if err := json.Unmarshal(respBody, result); err != nil {
			return fmt.Errorf("failed to unmarshal response: %w", err)
		}
	}

	return nil
}

// InsertTriples inserts one or more triples
func (c *Client) InsertTriples(req InsertTriplesRequest) (*InsertTriplesResponse, error) {
	var resp InsertTriplesResponse
	err := c.doRequest("POST", "/triples", req, &resp)
	return &resp, err
}

// QueryTriples queries triples by pattern
func (c *Client) QueryTriples(subject, predicate, object *string, includeSources bool) (*QueryTriplesResponse, error) {
	params := url.Values{}
	if subject != nil {
		params.Set("subject", *subject)
	}
	if predicate != nil {
		params.Set("predicate", *predicate)
	}
	if object != nil {
		params.Set("object", *object)
	}
	if includeSources {
		params.Set("include_sources", "true")
	}

	path := "/triples"
	if len(params) > 0 {
		path += "?" + params.Encode()
	}

	var resp QueryTriplesResponse
	err := c.doRequest("GET", path, nil, &resp)
	return &resp, err
}

// GetTripleSources gets provenance for a triple
func (c *Client) GetTripleSources(tripleID string) (*SourcesResponse, error) {
	var resp SourcesResponse
	err := c.doRequest("GET", fmt.Sprintf("/triples/%s/sources", tripleID), nil, &resp)
	return &resp, err
}

// GetNeighbors gets k-hop neighborhood for a node
func (c *Client) GetNeighbors(node string, depth int) (*NeighborsResponse, error) {
	path := fmt.Sprintf("/nodes/%s/neighbors?depth=%d", url.PathEscape(node), depth)
	var resp NeighborsResponse
	err := c.doRequest("GET", path, nil, &resp)
	return &resp, err
}

// Search performs semantic search
func (c *Client) Search(req SearchRequest) (*SearchResponse, error) {
	var resp SearchResponse
	err := c.doRequest("POST", "/search", req, &resp)
	return &resp, err
}

// GetStats gets engine statistics
func (c *Client) GetStats() (*StatsResponse, error) {
	var resp StatsResponse
	err := c.doRequest("GET", "/stats", nil, &resp)
	return &resp, err
}

// TriggerDecay triggers a decay cycle
func (c *Client) TriggerDecay(req DecayRequest) (*DecayResponse, error) {
	var resp DecayResponse
	err := c.doRequest("POST", "/maintenance/decay", req, &resp)
	return &resp, err
}

// TriggerEvict removes low-weight triples
func (c *Client) TriggerEvict(req EvictRequest) (*EvictResponse, error) {
	var resp EvictResponse
	err := c.doRequest("POST", "/maintenance/evict", req, &resp)
	return &resp, err
}

// RecomputeEmbeddings regenerates embeddings from the graph
func (c *Client) RecomputeEmbeddings(req RecomputeEmbeddingsRequest) (*RecomputeEmbeddingsResponse, error) {
	var resp RecomputeEmbeddingsResponse
	err := c.doRequest("POST", "/maintenance/recompute-embeddings", req, &resp)
	return &resp, err
}

// HealthCheck checks if the API is healthy
func (c *Client) HealthCheck() (map[string]interface{}, error) {
	var resp map[string]interface{}
	err := c.doRequest("GET", "/health", nil, &resp)
	return resp, err
}

// ========== VKB Methods ==========

// SessionStart starts a new session
func (c *Client) SessionStart(req SessionStartRequest) (*SessionStartResponse, error) {
	var resp SessionStartResponse
	err := c.doRequest("POST", "/sessions", req, &resp)
	return &resp, err
}

// SessionList lists sessions
func (c *Client) SessionList(status *string, limit int) ([]SessionResponse, error) {
	params := url.Values{}
	if status != nil {
		params.Set("status", *status)
	}
	if limit > 0 {
		params.Set("limit", fmt.Sprintf("%d", limit))
	}
	path := "/sessions"
	if len(params) > 0 {
		path += "?" + params.Encode()
	}
	var resp []SessionResponse
	err := c.doRequest("GET", path, nil, &resp)
	return resp, err
}

// SessionGet gets a session by ID
func (c *Client) SessionGet(id string) (*SessionResponse, error) {
	var resp SessionResponse
	err := c.doRequest("GET", fmt.Sprintf("/sessions/%s", id), nil, &resp)
	return &resp, err
}

// SessionEnd ends a session
func (c *Client) SessionEnd(id string, req SessionEndRequest) error {
	return c.doRequest("POST", fmt.Sprintf("/sessions/%s/end", id), req, nil)
}

// ========== Trust Methods ==========

// TrustQuery queries trust score for a DID
func (c *Client) TrustQuery(did string) (*TrustQueryResponse, error) {
	var resp TrustQueryResponse
	err := c.doRequest("GET", fmt.Sprintf("/trust?did=%s", url.QueryEscape(did)), nil, &resp)
	return &resp, err
}
