//! MCP (Model Context Protocol) server implementation
//!
//! Exposes the ValenceEngine as an MCP server via stdio transport
//! for integration with OpenClaw and other MCP clients.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::{Json, Parameters}},
    model::*,
    tool, tool_handler, tool_router,
    transport::stdio,
};

use crate::{
    api::{
        InsertTriplesRequest, InsertTriplesResponse,
        QueryTriplesResponse, SearchRequest, SearchResponse, NeighborsResponse,
        SourcesResponse, StatsResponse,
    },
    ValenceEngine,
};

mod tools;
use tools::*;

// Import parameter and response types for the new high-level tools
use tools::{
    ContextForQueryParams, ContextForQueryResponse,
    RecordFeedbackParams, RecordFeedbackResponse,
    SessionStartParams, SessionStartResponse,
    SessionEndParams, SessionEndResponse,
    ExploreParams, ExploreResponse,
};

/// MCP server wrapper for ValenceEngine
#[derive(Clone)]
pub struct McpServer {
    engine: ValenceEngine,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl McpServer {
    /// Create a new MCP server with the given engine
    pub fn new(engine: ValenceEngine) -> Self {
        Self {
            engine,
            tool_router: Self::tool_router(),
        }
    }

    /// Tool 1: insert_triples - Insert triples with source provenance
    #[tool(
        name = "insert_triples",
        description = "Insert one or more triples (subject-predicate-object) with optional source provenance"
    )]
    async fn insert_triples(
        &self,
        params: Parameters<InsertTriplesRequest>,
    ) -> Result<Json<InsertTriplesResponse>, String> {
        tools::insert_triples_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 2: query_triples - Pattern-match query with S/P/O wildcards
    #[tool(
        name = "query_triples",
        description = "Query triples by pattern matching. Supports wildcards by omitting subject, predicate, or object filters"
    )]
    async fn query_triples(
        &self,
        params: Parameters<QueryTriplesParams>,
    ) -> Result<Json<QueryTriplesResponse>, String> {
        tools::query_triples_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 3: search - Semantic search via topology embeddings
    #[tool(
        name = "search",
        description = "Semantic search for nodes similar to the query node using topology-derived embeddings"
    )]
    async fn search(
        &self,
        params: Parameters<SearchRequest>,
    ) -> Result<Json<SearchResponse>, String> {
        tools::search_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 4: neighbors - K-hop subgraph traversal
    #[tool(
        name = "neighbors",
        description = "Get k-hop neighborhood of a node - all triples within specified depth"
    )]
    async fn neighbors(
        &self,
        params: Parameters<NeighborsParams>,
    ) -> Result<Json<NeighborsResponse>, String> {
        tools::neighbors_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 5: sources - Get provenance for a triple
    #[tool(
        name = "sources",
        description = "Get provenance sources for a specific triple by ID"
    )]
    async fn sources(
        &self,
        params: Parameters<SourcesParams>,
    ) -> Result<Json<SourcesResponse>, String> {
        tools::sources_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 6: stats - Engine statistics
    #[tool(
        name = "stats",
        description = "Get current engine statistics (triple count, node count, average weight)"
    )]
    async fn stats(&self) -> Result<Json<StatsResponse>, String> {
        tools::stats_impl(&self.engine)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 7: maintain - Run decay/eviction/recompute cycle
    #[tool(
        name = "maintain",
        description = "Run maintenance operations: decay weights, evict low-weight triples, and/or recompute embeddings"
    )]
    async fn maintain(
        &self,
        params: Parameters<MaintainParams>,
    ) -> Result<Json<MaintainResponse>, String> {
        tools::maintain_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    // ========================================================================
    // High-level tools leveraging new modules
    // ========================================================================

    /// Tool 8: context_for_query - Assemble optimal context using working set + budget + fusion scoring
    #[tool(
        name = "context_for_query",
        description = "Assemble optimal context for a query using working set, budget constraints, and fusion scoring. Returns formatted context ready for LLM consumption."
    )]
    async fn context_for_query(
        &self,
        params: Parameters<ContextForQueryParams>,
    ) -> Result<Json<ContextForQueryResponse>, String> {
        tools::context_for_query_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 9: record_feedback - Record which triples from context were useful
    #[tool(
        name = "record_feedback",
        description = "Record feedback about which triples were useful in the context. Boosts weights of useful triples and decays not-useful ones. Feeds the inference loop."
    )]
    async fn record_feedback(
        &self,
        params: Parameters<RecordFeedbackParams>,
    ) -> Result<Json<RecordFeedbackResponse>, String> {
        tools::record_feedback_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 10: session_start - Start a new session with working set lifecycle
    #[tool(
        name = "session_start",
        description = "Start a new conversation session with an initial query. Creates a working set that evolves over the session."
    )]
    async fn session_start(
        &self,
        params: Parameters<SessionStartParams>,
    ) -> Result<Json<SessionStartResponse>, String> {
        tools::session_start_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 11: session_end - End a session
    #[tool(
        name = "session_end",
        description = "End a conversation session. Archives resolved threads and cleans up session state."
    )]
    async fn session_end(
        &self,
        params: Parameters<SessionEndParams>,
    ) -> Result<Json<SessionEndResponse>, String> {
        tools::session_end_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 12: explore - Interactive graph exploration with tiered retrieval
    #[tool(
        name = "explore",
        description = "Explore the knowledge graph interactively starting from a node. Uses tiered retrieval with budget constraints for efficient exploration."
    )]
    async fn explore(
        &self,
        params: Parameters<ExploreParams>,
    ) -> Result<Json<ExploreResponse>, String> {
        tools::explore_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool 13: combined_query - Connected AND similar (the killer query)
    #[tool(
        name = "combined_query",
        description = "Find nodes connected to anchor AND similar to target. Combines graph traversal with embedding similarity into a single query: 'what's connected to X that looks like Y?'"
    )]
    async fn combined_query(
        &self,
        params: Parameters<crate::query::combined::CombinedQueryParams>,
    ) -> Result<Json<crate::query::combined::CombinedQueryResponse>, String> {
        tools::combined_query_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: confidence_explain - Explain dynamic confidence score for a triple
    #[tool(
        name = "confidence_explain",
        description = "Explain the dynamic confidence score for a triple. Returns a breakdown of source_reliability (0.5 weight), path_diversity (0.3 weight), and centrality (0.2 weight). Optionally provide a context node to compute path diversity relative to a query."
    )]
    async fn confidence_explain(
        &self,
        params: Parameters<tools::ConfidenceExplainParams>,
    ) -> Result<Json<tools::ConfidenceExplainResponse>, String> {
        tools::confidence_explain_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    // ========================================================================
    // VKB Tools
    // ========================================================================

    /// Tool: session_get - Get a session by ID
    #[tool(
        name = "session_get",
        description = "Get details of a specific session by ID"
    )]
    async fn session_get(
        &self,
        params: Parameters<tools::SessionGetParams>,
    ) -> Result<Json<tools::VkbSessionResponse>, String> {
        tools::session_get_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: session_list - List sessions
    #[tool(
        name = "session_list",
        description = "List sessions with optional status filter"
    )]
    async fn session_list(
        &self,
        params: Parameters<tools::SessionListParams>,
    ) -> Result<Json<tools::VkbSessionListResponse>, String> {
        tools::session_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: session_find_by_room - Find session by room ID
    #[tool(
        name = "session_find_by_room",
        description = "Find an active session by external room ID"
    )]
    async fn session_find_by_room(
        &self,
        params: Parameters<tools::SessionFindByRoomParams>,
    ) -> Result<Json<tools::VkbSessionResponse>, String> {
        tools::session_find_by_room_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: exchange_add - Add an exchange to a session
    #[tool(
        name = "exchange_add",
        description = "Add a conversation exchange (user/assistant/system message) to a session"
    )]
    async fn exchange_add(
        &self,
        params: Parameters<tools::ExchangeAddParams>,
    ) -> Result<Json<tools::VkbExchangeResponse>, String> {
        tools::exchange_add_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: exchange_list - List exchanges for a session
    #[tool(
        name = "exchange_list",
        description = "List conversation exchanges for a session"
    )]
    async fn exchange_list(
        &self,
        params: Parameters<tools::ExchangeListParams>,
    ) -> Result<Json<tools::VkbExchangeListResponse>, String> {
        tools::exchange_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: pattern_record - Record a behavioral pattern
    #[tool(
        name = "pattern_record",
        description = "Record a new behavioral pattern observed in conversations"
    )]
    async fn pattern_record(
        &self,
        params: Parameters<tools::PatternRecordParams>,
    ) -> Result<Json<tools::VkbPatternResponse>, String> {
        tools::pattern_record_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: pattern_reinforce - Reinforce an existing pattern
    #[tool(
        name = "pattern_reinforce",
        description = "Reinforce an existing pattern with new evidence"
    )]
    async fn pattern_reinforce(
        &self,
        params: Parameters<tools::PatternReinforceParams>,
    ) -> Result<Json<tools::VkbPatternResponse>, String> {
        tools::pattern_reinforce_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: pattern_list - List patterns
    #[tool(
        name = "pattern_list",
        description = "List behavioral patterns with optional filters"
    )]
    async fn pattern_list(
        &self,
        params: Parameters<tools::PatternListParams>,
    ) -> Result<Json<tools::VkbPatternListResponse>, String> {
        tools::pattern_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: pattern_search - Search patterns by description
    #[tool(
        name = "pattern_search",
        description = "Search patterns by description text"
    )]
    async fn pattern_search(
        &self,
        params: Parameters<tools::PatternSearchParams>,
    ) -> Result<Json<tools::VkbPatternListResponse>, String> {
        tools::pattern_search_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: insight_extract - Extract an insight from a session
    #[tool(
        name = "insight_extract",
        description = "Extract an insight or learning from a conversation session"
    )]
    async fn insight_extract(
        &self,
        params: Parameters<tools::InsightExtractParams>,
    ) -> Result<Json<tools::VkbInsightResponse>, String> {
        tools::insight_extract_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: insight_list - List insights for a session
    #[tool(
        name = "insight_list",
        description = "List all insights extracted from a session"
    )]
    async fn insight_list(
        &self,
        params: Parameters<tools::InsightListParams>,
    ) -> Result<Json<tools::VkbInsightListResponse>, String> {
        tools::insight_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    // ========================================================================
    // Trust/Identity Tools
    // ========================================================================

    /// Tool: trust_query - Query trust score for a DID using PageRank
    #[tool(
        name = "trust_query",
        description = "Query trust score for a DID (computed via PageRank on the graph)"
    )]
    async fn trust_query(
        &self,
        params: Parameters<tools::TrustQueryParams>,
    ) -> Result<Json<tools::TrustQueryResponse>, String> {
        tools::trust_query_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: sign_triple - Sign a triple with the local keypair
    #[tool(
        name = "sign_triple",
        description = "Sign a triple with the engine's local keypair"
    )]
    async fn sign_triple(
        &self,
        params: Parameters<tools::SignTripleParams>,
    ) -> Result<Json<tools::SignTripleResponse>, String> {
        tools::sign_triple_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: verify_triple - Verify a triple's signature
    #[tool(
        name = "verify_triple",
        description = "Verify a triple's signature against its origin DID"
    )]
    async fn verify_triple(
        &self,
        params: Parameters<tools::VerifyTripleParams>,
    ) -> Result<Json<tools::VerifyTripleResponse>, String> {
        tools::verify_triple_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    // ========================================================================
    // Knowledge Management Tools
    // ========================================================================

    /// Tool: triple_get - Get a triple with full details
    #[tool(
        name = "triple_get",
        description = "Get detailed information about a specific triple including sources and metadata"
    )]
    async fn triple_get(
        &self,
        params: Parameters<tools::TripleGetParams>,
    ) -> Result<Json<crate::api::TripleResponse>, String> {
        tools::triple_get_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: triple_supersede - Supersede a triple with a new one
    #[tool(
        name = "triple_supersede",
        description = "Replace an old triple with a new one, maintaining history"
    )]
    async fn triple_supersede(
        &self,
        params: Parameters<tools::TripleSupersedePar>,
    ) -> Result<Json<tools::TripleSupersedeResponse>, String> {
        tools::triple_supersede_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: node_search - Search nodes by value
    #[tool(
        name = "node_search",
        description = "Search for nodes by value or type"
    )]
    async fn node_search(
        &self,
        params: Parameters<tools::NodeSearchParams>,
    ) -> Result<Json<tools::NodeSearchResponse>, String> {
        tools::node_search_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    // ========================================================================
    // Social/Trust/Sharing/Verification/Tension Tools
    // ========================================================================

    /// Tool: trust_check - Check trust for a topic
    #[tool(
        name = "trust_check",
        description = "Check trust levels for entities on a specific topic. Redirects to trust_query with PageRank-based trust scores."
    )]
    async fn trust_check(
        &self,
        params: Parameters<tools::TrustCheckParams>,
    ) -> Result<Json<tools::PlaceholderResponse>, String> {
        tools::trust_check_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: trust_edge_create - Create a trust edge between two DIDs
    #[tool(
        name = "trust_edge_create",
        description = "Create a trust edge between two DIDs (stored as a triple)"
    )]
    async fn trust_edge_create(
        &self,
        params: Parameters<tools::TrustEdgeCreateParams>,
    ) -> Result<Json<tools::TrustEdgeCreateResponse>, String> {
        tools::trust_edge_create_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: reputation_get - Get reputation (PageRank trust score) for a DID
    #[tool(
        name = "reputation_get",
        description = "Get the reputation score for a DID, computed via PageRank on the trust graph"
    )]
    async fn reputation_get(
        &self,
        params: Parameters<tools::ReputationGetParams>,
    ) -> Result<Json<tools::ReputationGetResponse>, String> {
        tools::reputation_get_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: share_create - Share a triple with a recipient DID
    #[tool(
        name = "share_create",
        description = "Share a triple with a specific recipient by DID"
    )]
    async fn share_create(
        &self,
        params: Parameters<tools::ShareCreateParams>,
    ) -> Result<Json<tools::ShareCreateResponse>, String> {
        tools::share_create_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: share_list - List all shares
    #[tool(
        name = "share_list",
        description = "List all shared triples and their recipients"
    )]
    async fn share_list(
        &self,
        params: Parameters<tools::ShareListParams>,
    ) -> Result<Json<tools::ShareListResponse>, String> {
        tools::share_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: share_revoke - Revoke a share
    #[tool(
        name = "share_revoke",
        description = "Revoke a previously shared triple, removing access"
    )]
    async fn share_revoke(
        &self,
        params: Parameters<tools::ShareRevokeParams>,
    ) -> Result<Json<tools::ShareRevokeResponse>, String> {
        tools::share_revoke_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: verification_submit - Submit a verification for a triple
    #[tool(
        name = "verification_submit",
        description = "Submit a verification for a triple (confirmed, contradicted, or uncertain)"
    )]
    async fn verification_submit(
        &self,
        params: Parameters<tools::VerificationSubmitParams>,
    ) -> Result<Json<tools::VerificationSubmitResponse>, String> {
        tools::verification_submit_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: verification_list - List verifications for a triple
    #[tool(
        name = "verification_list",
        description = "List all verifications submitted for a specific triple"
    )]
    async fn verification_list(
        &self,
        params: Parameters<tools::VerificationListParams>,
    ) -> Result<Json<tools::VerificationListResponse>, String> {
        tools::verification_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: tension_list - Detect tensions (contradictions) in the graph
    #[tool(
        name = "tension_list",
        description = "Detect tensions and contradictions in the knowledge graph by analyzing conflicting triples"
    )]
    async fn tension_list(
        &self,
        params: Parameters<tools::TensionListParams>,
    ) -> Result<Json<tools::TensionListResponse>, String> {
        tools::tension_list_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Tool: tension_resolve - Resolve a tension between two triples
    #[tool(
        name = "tension_resolve",
        description = "Resolve a tension between two conflicting triples by choosing which to keep"
    )]
    async fn tension_resolve(
        &self,
        params: Parameters<tools::TensionResolveParams>,
    ) -> Result<Json<tools::TensionResolveResponse>, String> {
        tools::tension_resolve_impl(&self.engine, params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    /// Run the MCP server on stdio
    pub async fn run_stdio(self) -> anyhow::Result<()> {
        tracing::info!("Starting MCP server on stdio");
        
        let service = self.serve(stdio()).await.inspect_err(|e| {
            tracing::error!("Error starting MCP server: {}", e);
        })?;
        
        service.waiting().await?;
        
        Ok(())
    }
}

/// Implement the server handler
#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Valence v2 Knowledge Engine: Triple-based knowledge substrate with topology-derived embeddings.\n\
                 \n\
                 Low-level tools: insert_triples, query_triples, search, neighbors, sources, stats, maintain.\n\
                 \n\
                 High-level tools (NEW):\n\
                 - context_for_query: Assemble optimal context using working set + budget + fusion scoring\n\
                 - record_feedback: Record which triples were useful (feeds inference loop)\n\
                 - session_start/session_end: Manage working set lifecycle\n\
                 - explore: Interactive graph exploration with tiered retrieval"
                    .into()
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let engine = ValenceEngine::new();
        let _server = McpServer::new(engine);
        // Just verify we can create the server
    }
}
