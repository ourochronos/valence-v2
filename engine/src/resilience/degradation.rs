//! Degradation state tracking and levels.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Degradation levels for the engine.
///
/// From docs/concepts/graceful-degradation.md:
/// - Full: All features available (embeddings + graph + confidence)
/// - Cold: Deterministic only (graph + confidence, no embeddings)
/// - Minimal: Graph traversal + recency only
/// - Offline: Cached results only (store unavailable)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DegradationLevel {
    /// Full mode: embeddings + graph + confidence. Best quality.
    Full,
    /// Cold mode: graph + confidence only. Good quality, no embedding costs.
    Cold,
    /// Minimal mode: graph traversal + recency. Acceptable quality.
    Minimal,
    /// Offline mode: cached results only. Store unavailable.
    Offline,
}

impl DegradationLevel {
    /// Check if embeddings are available at this level
    pub fn has_embeddings(&self) -> bool {
        matches!(self, DegradationLevel::Full)
    }

    /// Check if graph operations are available at this level
    pub fn has_graph(&self) -> bool {
        !matches!(self, DegradationLevel::Offline)
    }

    /// Check if confidence computation is available at this level
    pub fn has_confidence(&self) -> bool {
        matches!(self, DegradationLevel::Full | DegradationLevel::Cold)
    }

    /// Check if store is available at this level
    pub fn has_store(&self) -> bool {
        !matches!(self, DegradationLevel::Offline)
    }
}

/// A warning about degraded functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationWarning {
    /// Component that is degraded (e.g., "embeddings", "storage", "confidence")
    pub component: String,
    /// Human-readable warning message
    pub message: String,
    /// When this warning was first issued
    pub since: DateTime<Utc>,
    /// Last error that caused this warning
    pub last_error: Option<String>,
}

/// Component failure tracking
#[derive(Debug, Clone)]
struct ComponentStatus {
    /// Number of consecutive failures
    failures: u32,
    /// Last failure time
    last_failure: Option<DateTime<Utc>>,
    /// Last error message
    last_error: Option<String>,
    /// Number of consecutive successes (for recovery)
    successes: u32,
}

impl ComponentStatus {
    fn new() -> Self {
        Self {
            failures: 0,
            last_failure: None,
            last_error: None,
            successes: 0,
        }
    }

    fn record_failure(&mut self, error: &str) {
        self.failures += 1;
        self.successes = 0;
        self.last_failure = Some(Utc::now());
        self.last_error = Some(error.to_string());
    }

    fn record_success(&mut self) {
        self.successes += 1;
        if self.successes >= 3 {
            // Three consecutive successes clears the failure state
            self.failures = 0;
            self.last_failure = None;
            self.last_error = None;
        }
    }

    fn is_degraded(&self) -> bool {
        self.failures > 0
    }
}

/// Degradation state for the engine
#[derive(Debug, Clone)]
pub struct DegradationState {
    /// Current degradation level
    pub level: DegradationLevel,
    /// Component-specific failure tracking
    components: HashMap<String, ComponentStatus>,
    /// When degradation was last updated
    pub last_updated: DateTime<Utc>,
}

impl Default for DegradationState {
    fn default() -> Self {
        Self {
            level: DegradationLevel::Full,
            components: HashMap::new(),
            last_updated: Utc::now(),
        }
    }
}

impl DegradationState {
    /// Record a component failure
    pub fn record_failure(&mut self, component: &str, error: &str) {
        let status = self
            .components
            .entry(component.to_string())
            .or_insert_with(ComponentStatus::new);
        
        status.record_failure(error);
        self.last_updated = Utc::now();

        // Update degradation level based on component failures
        self.update_level();
    }

    /// Record a successful operation
    pub fn record_success(&mut self, component: &str) {
        if let Some(status) = self.components.get_mut(component) {
            status.record_success();
            self.last_updated = Utc::now();

            // May restore degradation level
            self.update_level();
        }
    }

    /// Check if a component is degraded
    pub fn is_component_degraded(&self, component: &str) -> bool {
        self.components
            .get(component)
            .map(|s| s.is_degraded())
            .unwrap_or(false)
    }

    /// Get all current warnings
    pub fn get_warnings(&self) -> Vec<DegradationWarning> {
        let mut warnings = Vec::new();

        for (component, status) in &self.components {
            if status.is_degraded() {
                let message = match component.as_str() {
                    "embeddings" => {
                        "Embeddings unavailable. Using graph-based retrieval only.".to_string()
                    }
                    "storage" => {
                        "Storage unavailable. Using cached results only.".to_string()
                    }
                    "confidence" => {
                        "Confidence computation failed. Returning results without confidence scores.".to_string()
                    }
                    "graph" => {
                        "Graph algorithms unavailable. Using direct lookups only.".to_string()
                    }
                    _ => {
                        format!("Component '{}' is degraded.", component)
                    }
                };

                warnings.push(DegradationWarning {
                    component: component.clone(),
                    message,
                    since: status.last_failure.unwrap_or_else(Utc::now),
                    last_error: status.last_error.clone(),
                });
            }
        }

        warnings
    }

    /// Update degradation level based on component states
    fn update_level(&mut self) {
        // Check component degradation states
        let embeddings_ok = !self.is_component_degraded("embeddings");
        let storage_ok = !self.is_component_degraded("storage");
        let graph_ok = !self.is_component_degraded("graph");

        // Determine appropriate degradation level
        self.level = if !storage_ok {
            DegradationLevel::Offline
        } else if !embeddings_ok && !graph_ok {
            DegradationLevel::Minimal
        } else if !embeddings_ok {
            DegradationLevel::Cold
        } else {
            DegradationLevel::Full
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degradation_level_capabilities() {
        assert!(DegradationLevel::Full.has_embeddings());
        assert!(DegradationLevel::Full.has_graph());
        assert!(DegradationLevel::Full.has_confidence());
        assert!(DegradationLevel::Full.has_store());

        assert!(!DegradationLevel::Cold.has_embeddings());
        assert!(DegradationLevel::Cold.has_graph());
        assert!(DegradationLevel::Cold.has_confidence());
        assert!(DegradationLevel::Cold.has_store());

        assert!(!DegradationLevel::Minimal.has_embeddings());
        assert!(DegradationLevel::Minimal.has_graph());
        assert!(!DegradationLevel::Minimal.has_confidence());
        assert!(DegradationLevel::Minimal.has_store());

        assert!(!DegradationLevel::Offline.has_embeddings());
        assert!(!DegradationLevel::Offline.has_graph());
        assert!(!DegradationLevel::Offline.has_confidence());
        assert!(!DegradationLevel::Offline.has_store());
    }

    #[test]
    fn test_state_failure_tracking() {
        let mut state = DegradationState::default();
        assert_eq!(state.level, DegradationLevel::Full);

        // Record embedding failure
        state.record_failure("embeddings", "compute failed");
        assert_eq!(state.level, DegradationLevel::Cold);
        assert!(state.is_component_degraded("embeddings"));

        // Record storage failure
        state.record_failure("storage", "connection lost");
        assert_eq!(state.level, DegradationLevel::Offline);
        assert!(state.is_component_degraded("storage"));
    }

    #[test]
    fn test_state_recovery() {
        let mut state = DegradationState::default();

        // Degrade
        state.record_failure("embeddings", "compute failed");
        assert_eq!(state.level, DegradationLevel::Cold);

        // Recover (needs 3 consecutive successes)
        state.record_success("embeddings");
        assert_eq!(state.level, DegradationLevel::Cold); // Still degraded
        
        state.record_success("embeddings");
        assert_eq!(state.level, DegradationLevel::Cold); // Still degraded
        
        state.record_success("embeddings");
        assert_eq!(state.level, DegradationLevel::Full); // Recovered!
        assert!(!state.is_component_degraded("embeddings"));
    }

    #[test]
    fn test_warnings_generation() {
        let mut state = DegradationState::default();

        // No warnings initially
        assert_eq!(state.get_warnings().len(), 0);

        // Add failures
        state.record_failure("embeddings", "compute failed");
        state.record_failure("confidence", "matrix error");

        // Should have 2 warnings
        let warnings = state.get_warnings();
        assert_eq!(warnings.len(), 2);

        // Check warning details
        let emb_warning = warnings.iter().find(|w| w.component == "embeddings").unwrap();
        assert!(emb_warning.message.contains("graph-based"));
        assert_eq!(emb_warning.last_error, Some("compute failed".to_string()));
    }

    #[test]
    fn test_multiple_component_degradation() {
        let mut state = DegradationState::default();

        // Degrade embeddings
        state.record_failure("embeddings", "compute failed");
        assert_eq!(state.level, DegradationLevel::Cold);

        // Degrade graph too
        state.record_failure("graph", "algorithm failed");
        assert_eq!(state.level, DegradationLevel::Minimal);

        // Recover graph
        state.record_success("graph");
        state.record_success("graph");
        state.record_success("graph");
        assert_eq!(state.level, DegradationLevel::Cold); // Back to cold (embeddings still degraded)
    }
}
