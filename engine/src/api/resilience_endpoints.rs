//! API endpoints for resilience and degradation status.

use axum::{
    extract::State,
    response::Json,
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use crate::{
    api::ApiState,
    resilience::{DegradationLevel, DegradationWarning},
};

use super::ApiError;

/// Response for degradation status endpoint
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DegradationStatusResponse {
    /// Current degradation level
    pub level: String,
    /// Whether any components are degraded
    pub is_degraded: bool,
    /// Active warnings
    pub warnings: Vec<DegradationWarningResponse>,
    /// Capabilities at current level
    pub capabilities: DegradationCapabilities,
}

/// Serializable degradation warning
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DegradationWarningResponse {
    /// Component name
    pub component: String,
    /// Warning message
    pub message: String,
    /// When the warning was first issued (ISO 8601)
    pub since: String,
    /// Last error that caused this warning
    pub last_error: Option<String>,
}

impl From<DegradationWarning> for DegradationWarningResponse {
    fn from(warning: DegradationWarning) -> Self {
        Self {
            component: warning.component,
            message: warning.message,
            since: warning.since.to_rfc3339(),
            last_error: warning.last_error,
        }
    }
}

/// Capabilities available at current degradation level
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DegradationCapabilities {
    /// Whether embeddings are available
    pub has_embeddings: bool,
    /// Whether graph operations are available
    pub has_graph: bool,
    /// Whether confidence computation is available
    pub has_confidence: bool,
    /// Whether store is available
    pub has_store: bool,
}

impl From<DegradationLevel> for DegradationCapabilities {
    fn from(level: DegradationLevel) -> Self {
        Self {
            has_embeddings: level.has_embeddings(),
            has_graph: level.has_graph(),
            has_confidence: level.has_confidence(),
            has_store: level.has_store(),
        }
    }
}

/// GET /resilience/status — Get current degradation status
pub async fn get_degradation_status(
    State(state): State<ApiState>,
) -> Result<Json<DegradationStatusResponse>, ApiError> {
    let level = state.engine.resilience.current_level().await;
    let warnings = state.engine.resilience.get_warnings().await;
    
    Ok(Json(DegradationStatusResponse {
        level: format!("{:?}", level),
        is_degraded: !warnings.is_empty(),
        warnings: warnings.into_iter().map(Into::into).collect(),
        capabilities: level.into(),
    }))
}

/// POST /resilience/reset — Reset degradation state (for testing)
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResetDegradationRequest {
    /// Optional level to set
    pub level: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResetDegradationResponse {
    /// New level after reset
    pub level: String,
    /// Success message
    pub message: String,
}

pub async fn reset_degradation(
    State(state): State<ApiState>,
    Json(req): Json<ResetDegradationRequest>,
) -> Result<Json<ResetDegradationResponse>, ApiError> {
    let new_level = if let Some(level_str) = req.level {
        match level_str.to_lowercase().as_str() {
            "full" => DegradationLevel::Full,
            "cold" => DegradationLevel::Cold,
            "minimal" => DegradationLevel::Minimal,
            "offline" => DegradationLevel::Offline,
            _ => return Err(ApiError::BadRequest(
                "Invalid degradation level. Must be one of: full, cold, minimal, offline".to_string()
            )),
        }
    } else {
        DegradationLevel::Full
    };

    state.engine.resilience.set_level(new_level).await;

    Ok(Json(ResetDegradationResponse {
        level: format!("{:?}", new_level),
        message: "Degradation state reset successfully".to_string(),
    }))
}
