# Resilience Module: Graceful Degradation

This module implements graceful degradation for the Valence engine, ensuring the system continues to function even when components fail.

## Design Philosophy

From `docs/concepts/graceful-degradation.md`:
- **Full mode**: Embeddings + graph + confidence (best quality)
- **Cold mode**: Graph + confidence only (good quality, no embedding costs)
- **Minimal mode**: Graph traversal + recency (acceptable quality)
- **Offline mode**: Cached results only (store unavailable)

## Components

### `degradation.rs`
Tracks degradation state and levels:
- `DegradationLevel`: Four levels (Full, Cold, Minimal, Offline)
- `DegradationState`: Tracks component failures and warnings
- Automatic level adjustment based on component health

### `fallback.rs`
Fallback strategies for resilient operations:
- `ResilientResult<T>`: Wraps results with optional degradation warnings
- `ResilientOperation` trait: Pattern for operations with automatic fallback
- `FallbackStrategy`: Enum defining fallback behaviors

### `retrieval.rs`
Resilient retrieval with automatic fallback:
- `ResilientRetrieval`: Retrieval engine with multi-level fallback
- `RetrievalMode`: Indicates which strategy was used
- Automatic degradation from embedding-based → graph-based → minimal

### `mod.rs`
Main module exports and `ResilienceManager`:
- Thread-safe degradation state tracking
- Component failure/success recording
- Warning retrieval and diagnostics

## Usage

### Basic Usage

```rust
use valence_engine::{ValenceEngine, resilience::ResilientRetrieval};
use std::sync::Arc;

let engine = ValenceEngine::new();

// Check current degradation level
let level = engine.resilience.current_level().await;
println!("Current level: {:?}", level);

// Perform resilient search
let retrieval = ResilientRetrieval::new(Arc::new(engine.clone()));
let result = retrieval.search("Alice", 10).await;

if result.used_fallback {
    println!("Warning: {}", result.warning.unwrap());
}
```

### Checking Degradation State

```rust
// Check if a component is degraded
if engine.resilience.is_degraded("embeddings").await {
    println!("Embeddings are unavailable");
}

// Get all warnings
let warnings = engine.resilience.get_warnings().await;
for warning in warnings {
    println!("{}: {}", warning.component, warning.message);
}
```

### Manual Level Control (Testing)

```rust
use valence_engine::DegradationLevel;

// Force cold mode
engine.resilience.set_level(DegradationLevel::Cold).await;
```

## API Endpoints

### `GET /resilience/status`
Returns current degradation status:
```json
{
  "level": "Cold",
  "is_degraded": true,
  "warnings": [
    {
      "component": "embeddings",
      "message": "Embeddings unavailable. Using graph-based retrieval only.",
      "since": "2025-02-16T15:55:00Z",
      "last_error": "Insufficient data to compute embeddings"
    }
  ],
  "capabilities": {
    "has_embeddings": false,
    "has_graph": true,
    "has_confidence": true,
    "has_store": true
  }
}
```

### `POST /resilience/reset`
Reset degradation state (for testing):
```json
{
  "level": "full"  // optional: "full", "cold", "minimal", "offline"
}
```

## Testing

Run resilience tests:
```bash
cargo test --lib resilience
```

Run integration tests:
```bash
cargo test --test resilience_integration
```

## Failure Tracking

Components track failures automatically:
- **3 consecutive failures**: Component marked as degraded
- **3 consecutive successes**: Component recovered
- Degradation level updates automatically based on component states

## Future Enhancements

- [ ] Cached results for offline mode
- [ ] Metrics export for monitoring
- [ ] Configurable failure thresholds
- [ ] Circuit breaker pattern integration
- [ ] Degradation event logging
