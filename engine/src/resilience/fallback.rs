//! Fallback strategies for resilient operations.

use async_trait::async_trait;
use std::fmt;

/// Result of a resilient operation, with optional degradation warning
#[derive(Debug)]
pub struct ResilientResult<T> {
    /// The result value (may be partial)
    pub value: T,
    /// Optional warning if degraded operation was used
    pub warning: Option<String>,
    /// Whether a fallback was used
    pub used_fallback: bool,
}

impl<T> ResilientResult<T> {
    /// Create a successful result without fallback
    pub fn ok(value: T) -> Self {
        Self {
            value,
            warning: None,
            used_fallback: false,
        }
    }

    /// Create a result using fallback strategy
    pub fn with_fallback(value: T, warning: String) -> Self {
        Self {
            value,
            warning: Some(warning),
            used_fallback: true,
        }
    }

    /// Map the value while preserving warnings
    pub fn map<U, F>(self, f: F) -> ResilientResult<U>
    where
        F: FnOnce(T) -> U,
    {
        ResilientResult {
            value: f(self.value),
            warning: self.warning,
            used_fallback: self.used_fallback,
        }
    }
}

/// Strategy for fallback behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackStrategy {
    /// Return empty/default result
    ReturnEmpty,
    /// Use cached result if available
    UseCache,
    /// Use simplified algorithm
    UseSimplified,
    /// Retry with different parameters
    Retry,
}

/// Trait for operations that support graceful degradation
#[async_trait]
pub trait ResilientOperation: Send + Sync {
    type Output: Send;
    type Error: fmt::Display + Send;

    /// Try the primary operation
    async fn try_primary(&self) -> Result<Self::Output, Self::Error>;

    /// Fallback operation when primary fails
    async fn fallback(&self, error: &Self::Error) -> ResilientResult<Self::Output>;

    /// Execute with automatic fallback on failure
    async fn execute(&self) -> ResilientResult<Self::Output> {
        match self.try_primary().await {
            Ok(output) => ResilientResult::ok(output),
            Err(error) => {
                // Log the error (in production, use proper logging)
                eprintln!("Primary operation failed: {}. Using fallback.", error);
                self.fallback(&error).await
            }
        }
    }
}

/// Wraps a fallible operation with a fallback
pub struct WithFallback<F, FB> {
    /// Primary operation
    primary: F,
    /// Fallback operation
    fallback: FB,
    /// Description for warnings
    description: String,
}

impl<F, FB> WithFallback<F, FB> {
    pub fn new(primary: F, fallback: FB, description: String) -> Self {
        Self {
            primary,
            fallback,
            description,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resilient_result_ok() {
        let result = ResilientResult::ok(42);
        assert_eq!(result.value, 42);
        assert!(result.warning.is_none());
        assert!(!result.used_fallback);
    }

    #[test]
    fn test_resilient_result_with_fallback() {
        let result = ResilientResult::with_fallback(
            vec![1, 2, 3],
            "Used cached results".to_string(),
        );
        assert_eq!(result.value, vec![1, 2, 3]);
        assert_eq!(result.warning, Some("Used cached results".to_string()));
        assert!(result.used_fallback);
    }

    #[test]
    fn test_resilient_result_map() {
        let result = ResilientResult::with_fallback(42, "fallback used".to_string());
        let mapped = result.map(|x| x * 2);
        assert_eq!(mapped.value, 84);
        assert_eq!(mapped.warning, Some("fallback used".to_string()));
        assert!(mapped.used_fallback);
    }

    #[test]
    fn test_fallback_strategy_types() {
        assert_eq!(FallbackStrategy::ReturnEmpty, FallbackStrategy::ReturnEmpty);
        assert_ne!(FallbackStrategy::ReturnEmpty, FallbackStrategy::UseCache);
    }
}
