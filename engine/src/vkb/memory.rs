//! In-memory SessionStore implementation with Arc<RwLock<...>> for thread safety.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use anyhow::{Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use super::models::{Session, Exchange, Pattern, Insight, SessionStatus, Platform, PatternStatus};
use super::store::SessionStore;

/// In-memory storage for VKB data
#[derive(Debug)]
struct MemoryState {
    sessions: HashMap<Uuid, Session>,
    exchanges: HashMap<Uuid, Vec<Exchange>>,
    patterns: HashMap<Uuid, Pattern>,
    insights: HashMap<Uuid, Vec<Insight>>,
}

impl MemoryState {
    fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            exchanges: HashMap::new(),
            patterns: HashMap::new(),
            insights: HashMap::new(),
        }
    }
}

/// In-memory SessionStore implementation
#[derive(Debug, Clone)]
pub struct MemorySessionStore {
    state: Arc<RwLock<MemoryState>>,
}

impl MemorySessionStore {
    /// Create a new in-memory session store
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(MemoryState::new())),
        }
    }
}

impl Default for MemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn create_session(&self, session: Session) -> Result<Uuid> {
        let mut state = self.state.write().unwrap();
        let id = session.id;
        state.sessions.insert(id, session);
        state.exchanges.insert(id, Vec::new());
        state.insights.insert(id, Vec::new());
        Ok(id)
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        let state = self.state.read().unwrap();
        Ok(state.sessions.get(&id).cloned())
    }

    async fn update_session(&self, session: Session) -> Result<()> {
        let mut state = self.state.write().unwrap();
        if !state.sessions.contains_key(&session.id) {
            bail!("Session not found: {}", session.id);
        }
        state.sessions.insert(session.id, session);
        Ok(())
    }

    async fn list_sessions(
        &self,
        status: Option<SessionStatus>,
        platform: Option<Platform>,
        project_context: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Session>> {
        let state = self.state.read().unwrap();
        let mut sessions: Vec<_> = state
            .sessions
            .values()
            .filter(|s| {
                if let Some(st) = status {
                    if s.status != st {
                        return false;
                    }
                }
                if let Some(pl) = platform {
                    if s.platform != pl {
                        return false;
                    }
                }
                if let Some(pc) = project_context {
                    if s.project_context.as_deref() != Some(pc) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by created_at descending (most recent first)
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply limit
        sessions.truncate(limit as usize);

        Ok(sessions)
    }

    async fn find_session_by_room(&self, external_room_id: &str) -> Result<Option<Session>> {
        let state = self.state.read().unwrap();
        let session = state
            .sessions
            .values()
            .find(|s| {
                s.external_room_id.as_deref() == Some(external_room_id)
                    && s.status == SessionStatus::Active
            })
            .cloned();
        Ok(session)
    }

    async fn end_session(
        &self,
        id: Uuid,
        status: SessionStatus,
        summary: Option<String>,
        themes: Vec<String>,
    ) -> Result<()> {
        let mut state = self.state.write().unwrap();
        let session = state
            .sessions
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))?;

        session.status = status;
        if let Some(s) = summary {
            session.summary = Some(s);
        }
        if !themes.is_empty() {
            session.themes = themes;
        }
        session.ended_at = Some(Utc::now());
        session.updated_at = Utc::now();

        Ok(())
    }

    async fn add_exchange(&self, exchange: Exchange) -> Result<Uuid> {
        let mut state = self.state.write().unwrap();

        // Validate session exists
        if !state.sessions.contains_key(&exchange.session_id) {
            bail!("Session not found: {}", exchange.session_id);
        }

        let id = exchange.id;
        state
            .exchanges
            .entry(exchange.session_id)
            .or_insert_with(Vec::new)
            .push(exchange);

        Ok(id)
    }

    async fn list_exchanges(&self, session_id: Uuid, limit: u32, offset: u32) -> Result<Vec<Exchange>> {
        let state = self.state.read().unwrap();
        let exchanges = state.exchanges.get(&session_id).cloned().unwrap_or_default();

        // Return in chronological order (created_at ascending)
        let start = offset as usize;
        let end = start + limit as usize;
        Ok(exchanges.into_iter().skip(start).take(end - start).collect())
    }

    async fn record_pattern(&self, pattern: Pattern) -> Result<Uuid> {
        let mut state = self.state.write().unwrap();
        let id = pattern.id;
        state.patterns.insert(id, pattern);
        Ok(id)
    }

    async fn reinforce_pattern(&self, pattern_id: Uuid, session_id: Option<Uuid>) -> Result<()> {
        let mut state = self.state.write().unwrap();
        let pattern = state
            .patterns
            .get_mut(&pattern_id)
            .ok_or_else(|| anyhow::anyhow!("Pattern not found: {}", pattern_id))?;

        // Add session to evidence if provided and not already present
        if let Some(sid) = session_id {
            if !pattern.evidence_session_ids.contains(&sid) {
                pattern.evidence_session_ids.push(sid);
            }
        }

        // Increment confidence by 0.1, capped at 1.0
        pattern.confidence = (pattern.confidence + 0.1).min(1.0);

        // Transition to "established" if confidence >= 0.7
        if pattern.confidence >= 0.7 && pattern.status == PatternStatus::Emerging {
            pattern.status = PatternStatus::Established;
        }

        pattern.updated_at = Utc::now();

        Ok(())
    }

    async fn list_patterns(
        &self,
        status: Option<&str>,
        pattern_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Pattern>> {
        let state = self.state.read().unwrap();
        let mut patterns: Vec<_> = state
            .patterns
            .values()
            .filter(|p| {
                if let Some(st) = status {
                    let p_status = match p.status {
                        PatternStatus::Emerging => "emerging",
                        PatternStatus::Established => "established",
                        PatternStatus::Fading => "fading",
                        PatternStatus::Archived => "archived",
                    };
                    if p_status != st {
                        return false;
                    }
                }
                if let Some(pt) = pattern_type {
                    if p.pattern_type != pt {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by confidence descending
        patterns.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Apply limit
        patterns.truncate(limit as usize);

        Ok(patterns)
    }

    async fn search_patterns(&self, query: &str, limit: u32) -> Result<Vec<Pattern>> {
        let state = self.state.read().unwrap();
        let query_lower = query.to_lowercase();

        let mut patterns: Vec<_> = state
            .patterns
            .values()
            .filter(|p| p.description.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();

        // Sort by confidence descending
        patterns.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Apply limit
        patterns.truncate(limit as usize);

        Ok(patterns)
    }

    async fn extract_insight(&self, insight: Insight) -> Result<Uuid> {
        let mut state = self.state.write().unwrap();

        // Validate session exists
        if !state.sessions.contains_key(&insight.session_id) {
            bail!("Session not found: {}", insight.session_id);
        }

        let id = insight.id;
        state
            .insights
            .entry(insight.session_id)
            .or_insert_with(Vec::new)
            .push(insight);

        Ok(id)
    }

    async fn list_insights(&self, session_id: Uuid) -> Result<Vec<Insight>> {
        let state = self.state.read().unwrap();
        Ok(state.insights.get(&session_id).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vkb::models::ExchangeRole;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let store = MemorySessionStore::new();

        // Create session
        let mut session = Session::new(Platform::ClaudeCode);
        session.project_context = Some("test-project".to_string());
        let session_id = store.create_session(session.clone()).await.unwrap();

        assert_eq!(session_id, session.id);

        // Get session
        let retrieved = store.get_session(session_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, session_id);
        assert_eq!(retrieved.status, SessionStatus::Active);

        // Add exchanges
        let ex1 = Exchange::new(session_id, ExchangeRole::User, "Hello");
        let ex2 = Exchange::new(session_id, ExchangeRole::Assistant, "Hi there!");

        store.add_exchange(ex1.clone()).await.unwrap();
        store.add_exchange(ex2.clone()).await.unwrap();

        // List exchanges
        let exchanges = store.list_exchanges(session_id, 10, 0).await.unwrap();
        assert_eq!(exchanges.len(), 2);
        assert_eq!(exchanges[0].content, "Hello");
        assert_eq!(exchanges[1].content, "Hi there!");

        // End session
        store
            .end_session(
                session_id,
                SessionStatus::Completed,
                Some("Test session completed".to_string()),
                vec!["testing".to_string()],
            )
            .await
            .unwrap();

        let ended = store.get_session(session_id).await.unwrap().unwrap();
        assert_eq!(ended.status, SessionStatus::Completed);
        assert_eq!(ended.summary, Some("Test session completed".to_string()));
        assert_eq!(ended.themes, vec!["testing".to_string()]);
        assert!(ended.ended_at.is_some());
    }

    #[tokio::test]
    async fn test_session_listing() {
        let store = MemorySessionStore::new();

        // Create multiple sessions
        let s1 = Session::new(Platform::ClaudeCode);
        let s2 = Session::new(Platform::Slack);
        let mut s3 = Session::new(Platform::ClaudeCode);
        s3.status = SessionStatus::Completed;

        store.create_session(s1.clone()).await.unwrap();
        store.create_session(s2.clone()).await.unwrap();
        store.create_session(s3.clone()).await.unwrap();

        // List all
        let all = store.list_sessions(None, None, None, 100).await.unwrap();
        assert_eq!(all.len(), 3);

        // Filter by status
        let active = store.list_sessions(Some(SessionStatus::Active), None, None, 100).await.unwrap();
        assert_eq!(active.len(), 2);

        // Filter by platform
        let claude = store.list_sessions(None, Some(Platform::ClaudeCode), None, 100).await.unwrap();
        assert_eq!(claude.len(), 2);

        // Limit
        let limited = store.list_sessions(None, None, None, 1).await.unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn test_find_by_room() {
        let store = MemorySessionStore::new();

        let mut session = Session::new(Platform::Matrix);
        session.external_room_id = Some("!room123:matrix.org".to_string());

        store.create_session(session.clone()).await.unwrap();

        let found = store.find_session_by_room("!room123:matrix.org").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, session.id);

        let not_found = store.find_session_by_room("!nonexistent:matrix.org").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_exchange_ordering() {
        let store = MemorySessionStore::new();

        let session = Session::new(Platform::ClaudeCode);
        let session_id = store.create_session(session).await.unwrap();

        // Add exchanges in order
        for i in 1..=5 {
            let ex = Exchange::new(session_id, ExchangeRole::User, format!("Message {}", i));
            store.add_exchange(ex).await.unwrap();
        }

        // List should be in chronological order
        let exchanges = store.list_exchanges(session_id, 10, 0).await.unwrap();
        assert_eq!(exchanges.len(), 5);
        assert_eq!(exchanges[0].content, "Message 1");
        assert_eq!(exchanges[4].content, "Message 5");

        // Test pagination
        let page1 = store.list_exchanges(session_id, 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].content, "Message 1");

        let page2 = store.list_exchanges(session_id, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].content, "Message 3");
    }

    #[tokio::test]
    async fn test_pattern_creation() {
        let store = MemorySessionStore::new();

        let pattern = Pattern::new("preference", "User prefers Rust");
        let pattern_id = store.record_pattern(pattern.clone()).await.unwrap();

        let patterns = store.list_patterns(None, None, 100).await.unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].id, pattern_id);
        assert_eq!(patterns[0].status, PatternStatus::Emerging);
        assert_eq!(patterns[0].confidence, 0.4); // Patterns start at 0.4
    }

    #[tokio::test]
    async fn test_pattern_reinforcement() {
        let store = MemorySessionStore::new();

        let session = Session::new(Platform::ClaudeCode);
        let session_id = store.create_session(session).await.unwrap();

        let mut pattern = Pattern::new("preference", "User prefers Rust");
        pattern.confidence = 0.4;
        let pattern_id = store.record_pattern(pattern).await.unwrap();

        // Reinforce once
        store.reinforce_pattern(pattern_id, Some(session_id)).await.unwrap();

        let patterns = store.list_patterns(None, None, 100).await.unwrap();
        let p = &patterns[0];
        assert_eq!(p.confidence, 0.5);
        assert_eq!(p.evidence_session_ids.len(), 1);
        assert_eq!(p.status, PatternStatus::Emerging);

        // Reinforce multiple times to reach "established"
        for _ in 0..3 {
            store.reinforce_pattern(pattern_id, None).await.unwrap();
        }

        let patterns = store.list_patterns(None, None, 100).await.unwrap();
        let p = &patterns[0];
        assert!(p.confidence >= 0.7);
        assert_eq!(p.status, PatternStatus::Established);
    }

    #[tokio::test]
    async fn test_pattern_search() {
        let store = MemorySessionStore::new();

        store.record_pattern(Pattern::new("preference", "User prefers Rust")).await.unwrap();
        store.record_pattern(Pattern::new("preference", "User likes Python")).await.unwrap();
        store.record_pattern(Pattern::new("workflow", "Uses TDD approach")).await.unwrap();

        // Case-insensitive substring search
        let results = store.search_patterns("rust", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].description.contains("Rust"));

        let results = store.search_patterns("user", 10).await.unwrap();
        assert_eq!(results.len(), 2);

        let results = store.search_patterns("TDD", 10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_pattern_filtering() {
        let store = MemorySessionStore::new();

        let mut p1 = Pattern::new("preference", "Prefers Rust");
        p1.status = PatternStatus::Established;
        store.record_pattern(p1).await.unwrap();

        let p2 = Pattern::new("workflow", "Uses TDD");
        store.record_pattern(p2).await.unwrap();

        let mut p3 = Pattern::new("preference", "Likes testing");
        p3.status = PatternStatus::Fading;
        store.record_pattern(p3).await.unwrap();

        // Filter by status
        let established = store.list_patterns(Some("established"), None, 100).await.unwrap();
        assert_eq!(established.len(), 1);

        // Filter by type
        let preferences = store.list_patterns(None, Some("preference"), 100).await.unwrap();
        assert_eq!(preferences.len(), 2);

        // Both filters
        let filtered = store.list_patterns(Some("fading"), Some("preference"), 100).await.unwrap();
        assert_eq!(filtered.len(), 1);
    }

    #[tokio::test]
    async fn test_insight_extraction() {
        let store = MemorySessionStore::new();

        let session = Session::new(Platform::ClaudeCode);
        let session_id = store.create_session(session).await.unwrap();

        let mut insight1 = Insight::new(session_id, "User values simplicity");
        insight1.domain_path = vec!["tech".to_string(), "rust".to_string()];

        let mut insight2 = Insight::new(session_id, "Project uses async patterns");
        insight2.domain_path = vec!["tech".to_string(), "async".to_string()];

        store.extract_insight(insight1).await.unwrap();
        store.extract_insight(insight2).await.unwrap();

        let insights = store.list_insights(session_id).await.unwrap();
        assert_eq!(insights.len(), 2);
        assert_eq!(insights[0].content, "User values simplicity");
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        use tokio::task;

        let store = MemorySessionStore::new();

        // Spawn multiple tasks that create sessions concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = store.clone();
            let handle = task::spawn(async move {
                let mut session = Session::new(Platform::ClaudeCode);
                session.project_context = Some(format!("project-{}", i));
                store_clone.create_session(session).await
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // Should have 10 sessions
        let sessions = store.list_sessions(None, None, None, 100).await.unwrap();
        assert_eq!(sessions.len(), 10);
    }
}
