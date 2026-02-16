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
                "Valence v2 Knowledge Engine: Triple-based knowledge substrate with topology-derived embeddings. \
                 Use insert_triples to store knowledge, query_triples for pattern matching, search for semantic similarity, \
                 neighbors for graph traversal, sources for provenance, stats for monitoring, and maintain for housekeeping."
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
