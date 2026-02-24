# VKB and WorkingSet Integration

The VKB (Valence Knowledge Base) conversation tracking system integrates lightly with the WorkingSet concept from `engine/src/context/working_set.rs`.

## Integration Points

### 1. Session-Scoped WorkingSet

When a VKB session starts, you can optionally initialize a WorkingSet for that session:

```rust
use valence_engine::vkb::{Session, Platform, MemorySessionStore, SessionStore};
use valence_engine::context::WorkingSet;

// Start a session
let session = Session::new(Platform::ClaudeCode);
let session_id = store.create_session(session).await?;

// Create a session-scoped working set
let mut working_set = WorkingSet::new_session(session_id);
```

### 2. Recording Exchanges with Node Activation

When exchanges are recorded, the caller can activate nodes in the working set to track which concepts are being discussed:

```rust
use valence_engine::vkb::{Exchange, ExchangeRole};

// Add an exchange
let exchange = Exchange::new(session_id, ExchangeRole::User, "How does async work?");
store.add_exchange(exchange).await?;

// Activate relevant nodes in the working set
let rust_node = engine.store.find_node_by_value("Rust").await?.unwrap();
let async_node = engine.store.find_node_by_value("async programming").await?.unwrap();

working_set.activate_nodes(&[rust_node.id, async_node.id]);

// Update turn to apply decay
working_set.update_turn(0.2);
```

### 3. Pattern Extraction from WorkingSet

After a session, you can extract patterns from the working set's conversation threads:

```rust
use valence_engine::vkb::{Pattern, create_pattern};

// Extract patterns from resolved threads
for (thread_id, thread) in &working_set.threads {
    if thread.status == ThreadStatus::Resolved && thread.thread_type == ThreadType::Decision {
        create_pattern(
            &store,
            "decision",
            thread.description.clone(),
            Some(vec![session_id]),
        ).await?;
    }
}
```

## Design Philosophy

The integration is **intentionally light** to keep the two systems loosely coupled:

- **VKB** tracks the conversation history (sessions, exchanges, patterns, insights)
- **WorkingSet** tracks the active conceptual context for retrieval

They share a `session_id` but don't have deep dependencies. This allows:

1. Using VKB without WorkingSets (e.g., for simple chat logging)
2. Using WorkingSets without VKB (e.g., for one-off queries)
3. Using both together for rich context-aware conversation tracking

## Example: Full Integration

```rust
use valence_engine::ValenceEngine;
use valence_engine::vkb::{Session, Exchange, ExchangeRole, Platform, MemorySessionStore, SessionStore};
use valence_engine::context::{WorkingSet, ThreadType};

async fn conversation_loop(engine: &ValenceEngine) -> Result<()> {
    let store = MemorySessionStore::new();

    // Start session
    let session = Session::new(Platform::ClaudeCode);
    let session_id = store.create_session(session).await?;

    // Initialize working set from initial query
    let mut working_set = WorkingSet::from_query(engine, "Rust async patterns", 5).await?;
    working_set.session_id = Some(session_id);

    // Conversation loop
    loop {
        // Get user input
        let user_input = get_user_input();
        if user_input == "exit" { break; }

        // Record exchange
        let exchange = Exchange::new(session_id, ExchangeRole::User, &user_input);
        store.add_exchange(exchange).await?;

        // Activate nodes based on input
        // (In a real implementation, you'd parse the input and find relevant nodes)

        // Generate response
        let response = generate_response(&working_set, &user_input);
        let exchange = Exchange::new(session_id, ExchangeRole::Assistant, &response);
        store.add_exchange(exchange).await?;

        // Update working set
        working_set.update_turn(0.2);
    }

    // End session
    store.end_session(
        session_id,
        SessionStatus::Completed,
        Some("Discussed Rust async patterns".to_string()),
        vec!["rust".to_string(), "async".to_string()],
    ).await?;

    Ok(())
}
```

## Future Enhancements

Potential future integrations (not currently implemented):

1. **Automatic Pattern Detection**: Analyze working set thread patterns to suggest VKB patterns
2. **Context Injection**: Use recent exchanges to seed the working set query
3. **Insight Linking**: Link VKB insights directly to triples in the working set
4. **Stigmergy Integration**: Use VKB access patterns to inform working set activation scores
