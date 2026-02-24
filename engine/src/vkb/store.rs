use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use super::models::{Session, Exchange, Pattern, Insight, SessionStatus, Platform};

#[async_trait]
pub trait SessionStore: Send + Sync {
    // Session CRUD
    async fn create_session(&self, session: Session) -> Result<Uuid>;
    async fn get_session(&self, id: Uuid) -> Result<Option<Session>>;
    async fn update_session(&self, session: Session) -> Result<()>;
    async fn list_sessions(
        &self,
        status: Option<SessionStatus>,
        platform: Option<Platform>,
        project_context: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Session>>;
    async fn find_session_by_room(&self, external_room_id: &str) -> Result<Option<Session>>;
    async fn end_session(&self, id: Uuid, status: SessionStatus, summary: Option<String>, themes: Vec<String>) -> Result<()>;

    // Exchange CRUD
    async fn add_exchange(&self, exchange: Exchange) -> Result<Uuid>;
    async fn list_exchanges(&self, session_id: Uuid, limit: u32, offset: u32) -> Result<Vec<Exchange>>;

    // Pattern CRUD
    async fn record_pattern(&self, pattern: Pattern) -> Result<Uuid>;
    async fn reinforce_pattern(&self, pattern_id: Uuid, session_id: Option<Uuid>) -> Result<()>;
    async fn list_patterns(&self, status: Option<&str>, pattern_type: Option<&str>, limit: u32) -> Result<Vec<Pattern>>;
    async fn search_patterns(&self, query: &str, limit: u32) -> Result<Vec<Pattern>>;

    // Insight CRUD
    async fn extract_insight(&self, insight: Insight) -> Result<Uuid>;
    async fn list_insights(&self, session_id: Uuid) -> Result<Vec<Insight>>;
}
