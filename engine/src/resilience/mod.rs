//! Resilience module: graceful degradation for Valence engine operations.
//!
//! When components fail (embeddings unavailable, store errors, etc.), the engine
//! degrades gracefully rather than returning errors. This module provides:
//!
//! - Fallback strategies for each operation type
//! - Degradation state tracking and warnings
//! - Partial result returns with degradation metadata
//!
//! Design philosophy (from docs/concepts/graceful-degradation.md):
//! - Full mode: embeddings + graph + confidence (best quality)
//! - Cold mode: graph + confidence only (good quality, no embedding costs)
//! - Minimal mode: graph traversal + recency only (acceptable quality)
//! - Offline mode: cached results when store is unavailable

pub mod fallback;
pub mod degradation;
pub mod retrieval;

pub use degradation::{DegradationLevel, DegradationState, DegradationWarning};
pub use fallback::{FallbackStrategy, ResilientOperation};
pub use retrieval::{ResilientRetrieval, RetrievalMode};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe degradation state tracker
#[derive(Clone)]
pub struct ResilienceManager {
    state: Arc<RwLock<DegradationState>>,
}

impl ResilienceManager {
    /// Create a new resilience manager
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(DegradationState::default())),
        }
    }

    /// Get current degradation level
    pub async fn current_level(&self) -> DegradationLevel {
        self.state.read().await.level
    }

    /// Record a component failure and adjust degradation level if needed
    pub async fn record_failure(&self, component: &str, error: &str) {
        let mut state = self.state.write().await;
        state.record_failure(component, error);
    }

    /// Record a successful operation (may restore degradation level)
    pub async fn record_success(&self, component: &str) {
        let mut state = self.state.write().await;
        state.record_success(component);
    }

    /// Get all current warnings
    pub async fn get_warnings(&self) -> Vec<DegradationWarning> {
        self.state.read().await.get_warnings()
    }

    /// Check if a specific component is degraded
    pub async fn is_degraded(&self, component: &str) -> bool {
        self.state.read().await.is_component_degraded(component)
    }

    /// Force a specific degradation level (for testing or manual control)
    pub async fn set_level(&self, level: DegradationLevel) {
        let mut state = self.state.write().await;
        state.level = level;
    }

    /// Get the full degradation state (for diagnostics)
    pub async fn get_state(&self) -> DegradationState {
        self.state.read().await.clone()
    }
}

impl Default for ResilienceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resilience_manager_creation() {
        let manager = ResilienceManager::new();
        assert_eq!(manager.current_level().await, DegradationLevel::Full);
    }

    #[tokio::test]
    async fn test_failure_recording() {
        let manager = ResilienceManager::new();
        
        // Record embedding failure
        manager.record_failure("embeddings", "compute failed").await;

        // Should degrade to Cold level (embeddings unavailable)
        let level = manager.current_level().await;
        assert_eq!(level, DegradationLevel::Cold);

        // Check warnings exist
        let warnings = manager.get_warnings().await;
        assert!(!warnings.is_empty());
    }

    #[tokio::test]
    async fn test_degradation_detection() {
        let manager = ResilienceManager::new();
        
        // Initially not degraded
        assert!(!manager.is_degraded("embeddings").await);

        // Record failure
        manager.record_failure("embeddings", "compute failed").await;

        // Should be marked as degraded
        assert!(manager.is_degraded("embeddings").await);
    }

    #[tokio::test]
    async fn test_success_recovery() {
        let manager = ResilienceManager::new();
        
        // Record failure
        manager.record_failure("embeddings", "compute failed").await;
        assert!(manager.is_degraded("embeddings").await);
        assert_eq!(manager.current_level().await, DegradationLevel::Cold);

        // Record 3 consecutive successes (required for recovery)
        manager.record_success("embeddings").await;
        manager.record_success("embeddings").await;
        manager.record_success("embeddings").await;

        // Should recover
        assert!(!manager.is_degraded("embeddings").await);
        assert_eq!(manager.current_level().await, DegradationLevel::Full);
    }

    #[tokio::test]
    async fn test_manual_level_override() {
        let manager = ResilienceManager::new();
        
        // Force cold mode
        manager.set_level(DegradationLevel::Cold).await;
        assert_eq!(manager.current_level().await, DegradationLevel::Cold);

        // Force minimal mode
        manager.set_level(DegradationLevel::Minimal).await;
        assert_eq!(manager.current_level().await, DegradationLevel::Minimal);
    }
}
