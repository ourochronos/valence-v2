//! Error types for the Valence engine.

use thiserror::Error;

/// Main error type for the Valence engine.
#[derive(Error, Debug)]
pub enum ValenceError {
    /// Storage-related errors
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Graph-related errors
    #[error("Graph error: {0}")]
    Graph(#[from] GraphError),

    /// Embedding-related errors
    #[error("Embedding error: {0}")]
    Embedding(#[from] EmbeddingError),

    /// API/HTTP errors
    #[error("API error: {0}")]
    Api(#[from] ApiError),

    /// Generic error for wrapping anyhow
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Storage-specific errors
#[derive(Error, Debug)]
pub enum StorageError {
    /// Node not found
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Triple not found
    #[error("Triple not found: {0}")]
    TripleNotFound(String),

    /// Database connection error
    #[error("Database connection failed: {0}")]
    ConnectionFailed(String),

    /// Query execution error
    #[error("Query failed: {0}")]
    QueryFailed(String),

    /// Lock acquisition error
    #[error("Failed to acquire lock: {0}")]
    LockError(String),

    /// PostgreSQL-specific error
    #[cfg(feature = "postgres")]
    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] sqlx::Error),
}

/// Graph algorithm errors
#[derive(Error, Debug)]
pub enum GraphError {
    /// Graph is empty
    #[error("Graph is empty")]
    EmptyGraph,

    /// Node not in graph
    #[error("Node {0} not in graph")]
    NodeNotInGraph(String),

    /// Invalid graph structure
    #[error("Invalid graph structure: {0}")]
    InvalidStructure(String),

    /// Algorithm failed
    #[error("Graph algorithm failed: {0}")]
    AlgorithmFailed(String),
}

/// Embedding computation errors
#[derive(Error, Debug)]
pub enum EmbeddingError {
    /// Insufficient data for embedding
    #[error("Insufficient data to compute embeddings (need at least {min} nodes, found {found})")]
    InsufficientData { min: usize, found: usize },

    /// Invalid dimension count
    #[error("Invalid embedding dimension: {0}")]
    InvalidDimension(usize),

    /// Embedding not found for node
    #[error("No embedding found for node: {0}")]
    NotFound(String),

    /// Matrix computation error
    #[error("Matrix computation failed: {0}")]
    MatrixError(String),

    /// Numerical stability issue
    #[error("Numerical instability detected: {0}")]
    NumericalInstability(String),
}

/// API/HTTP errors
#[derive(Error, Debug)]
pub enum ApiError {
    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Bad request (client error)
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),
}

/// Result type alias for ValenceError
pub type Result<T> = std::result::Result<T, ValenceError>;
