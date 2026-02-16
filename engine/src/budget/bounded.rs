//! Budget tracking for operations.
//!
//! Every operation should have a budget. When exhausted, return what you have.

use std::time::Instant;

/// Tracks budgets for an operation: time, hops, and result count.
///
/// Once any budget is exhausted, the operation should terminate gracefully
/// and return partial results.
#[derive(Debug, Clone)]
pub struct OperationBudget {
    time_budget_ms: u64,
    hop_budget: u32,
    result_budget: usize,
    start_time: Instant,
}

impl OperationBudget {
    /// Create a new budget with specified limits.
    ///
    /// # Arguments
    /// * `time_ms` - Maximum time allowed in milliseconds
    /// * `max_hops` - Maximum graph hops allowed
    /// * `max_results` - Maximum number of results to return
    pub fn new(time_ms: u64, max_hops: u32, max_results: usize) -> Self {
        Self {
            time_budget_ms: time_ms,
            hop_budget: max_hops,
            result_budget: max_results,
            start_time: Instant::now(),
        }
    }

    /// Get the remaining time budget in milliseconds.
    ///
    /// Returns 0 if the time budget is exhausted.
    pub fn time_remaining_ms(&self) -> u64 {
        let elapsed = self.start_time.elapsed().as_millis() as u64;
        if elapsed >= self.time_budget_ms {
            0
        } else {
            self.time_budget_ms - elapsed
        }
    }

    /// Check if the time budget is exhausted.
    pub fn time_exhausted(&self) -> bool {
        self.start_time.elapsed().as_millis() as u64 >= self.time_budget_ms
    }

    /// Check if a hop count is within budget.
    ///
    /// Returns `true` if `current_hop` is less than the hop budget.
    pub fn check_hop(&self, current_hop: u32) -> bool {
        current_hop < self.hop_budget
    }

    /// Check if a result count is within budget.
    ///
    /// Returns `true` if `current_count` is less than the result budget.
    pub fn check_results(&self, current_count: usize) -> bool {
        current_count < self.result_budget
    }

    /// Check if any budget is exhausted.
    ///
    /// Returns `true` if time budget is exceeded. Hop and result budgets
    /// should be checked with `check_hop` and `check_results` respectively.
    pub fn is_exhausted(&self) -> bool {
        self.time_exhausted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_budget_creation() {
        let budget = OperationBudget::new(100, 5, 20);
        assert!(!budget.time_exhausted());
        assert!(budget.check_hop(0));
        assert!(budget.check_hop(4));
        assert!(!budget.check_hop(5));
        assert!(budget.check_results(19));
        assert!(!budget.check_results(20));
    }

    #[test]
    fn test_time_tracking() {
        let budget = OperationBudget::new(50, 5, 20);
        
        // Initially, time should not be exhausted
        assert!(!budget.time_exhausted());
        assert!(budget.time_remaining_ms() > 0);
        
        // Sleep for 60ms
        thread::sleep(Duration::from_millis(60));
        
        // Now time should be exhausted
        assert!(budget.time_exhausted());
        assert_eq!(budget.time_remaining_ms(), 0);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_hop_budget() {
        let budget = OperationBudget::new(1000, 3, 100);
        
        assert!(budget.check_hop(0));
        assert!(budget.check_hop(1));
        assert!(budget.check_hop(2));
        assert!(!budget.check_hop(3));
        assert!(!budget.check_hop(4));
    }

    #[test]
    fn test_result_budget() {
        let budget = OperationBudget::new(1000, 10, 5);
        
        assert!(budget.check_results(0));
        assert!(budget.check_results(4));
        assert!(!budget.check_results(5));
        assert!(!budget.check_results(10));
    }

    #[test]
    fn test_time_remaining() {
        let budget = OperationBudget::new(100, 5, 20);
        
        let initial_remaining = budget.time_remaining_ms();
        assert!(initial_remaining > 90 && initial_remaining <= 100);
        
        thread::sleep(Duration::from_millis(30));
        
        let remaining = budget.time_remaining_ms();
        assert!(remaining < initial_remaining);
        // Don't assert remaining > 0 as it might be close to 0 on slow systems
        assert!(remaining <= initial_remaining);
    }
}
