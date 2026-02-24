//! PostgreSQL SessionStore implementation using sqlx.
//!
//! Feature-gated behind `postgres`. Uses the vkb_* tables from the schema.

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::models::{Session, Exchange, Pattern, Insight, SessionStatus, Platform, ExchangeRole, PatternStatus};
use super::store::SessionStore;

/// PostgreSQL SessionStore implementation
#[derive(Debug, Clone)]
pub struct PgSessionStore {
    pool: PgPool,
}

impl PgSessionStore {
    /// Create a new Postgres session store with an existing pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new Postgres session store from a connection string
    pub async fn from_connection_string(connection_string: &str) -> Result<Self> {
        let pool = PgPool::connect(connection_string)
            .await
            .context("Failed to connect to Postgres")?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl SessionStore for PgSessionStore {
    async fn create_session(&self, session: Session) -> Result<Uuid> {
        let platform_str = session.platform.as_str();
        let status_str = match session.status {
            SessionStatus::Active => "active",
            SessionStatus::Completed => "completed",
            SessionStatus::Abandoned => "abandoned",
        };

        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, platform, project_context, status, summary, themes,
                claude_session_id, external_room_id, metadata, created_at, updated_at, ended_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(session.id)
        .bind(platform_str)
        .bind(session.project_context)
        .bind(status_str)
        .bind(session.summary)
        .bind(&session.themes)
        .bind(session.claude_session_id)
        .bind(session.external_room_id)
        .bind(&session.metadata)
        .bind(session.created_at)
        .bind(session.updated_at)
        .bind(session.ended_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert session")?;

        Ok(session.id)
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<Session>> {
        let row = sqlx::query(
            r#"
            SELECT id, platform, project_context, status, summary, themes,
                   claude_session_id, external_room_id, metadata,
                   created_at, updated_at, ended_at
            FROM sessions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch session")?;

        if let Some(row) = row {
            Ok(Some(row_to_session(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn update_session(&self, session: Session) -> Result<()> {
        let platform_str = session.platform.as_str();
        let status_str = match session.status {
            SessionStatus::Active => "active",
            SessionStatus::Completed => "completed",
            SessionStatus::Abandoned => "abandoned",
        };

        let result = sqlx::query(
            r#"
            UPDATE sessions
            SET platform = $2, project_context = $3, status = $4, summary = $5, themes = $6,
                claude_session_id = $7, external_room_id = $8, metadata = $9,
                updated_at = $10, ended_at = $11
            WHERE id = $1
            "#,
        )
        .bind(session.id)
        .bind(platform_str)
        .bind(session.project_context)
        .bind(status_str)
        .bind(session.summary)
        .bind(&session.themes)
        .bind(session.claude_session_id)
        .bind(session.external_room_id)
        .bind(&session.metadata)
        .bind(session.updated_at)
        .bind(session.ended_at)
        .execute(&self.pool)
        .await
        .context("Failed to update session")?;

        if result.rows_affected() == 0 {
            bail!("Session not found: {}", session.id);
        }

        Ok(())
    }

    async fn list_sessions(
        &self,
        status: Option<SessionStatus>,
        platform: Option<Platform>,
        project_context: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Session>> {
        let mut query = String::from(
            "SELECT id, platform, project_context, status, summary, themes,
                    claude_session_id, external_room_id, metadata,
                    created_at, updated_at, ended_at
             FROM sessions WHERE 1=1"
        );

        if let Some(st) = status {
            let status_str = match st {
                SessionStatus::Active => "active",
                SessionStatus::Completed => "completed",
                SessionStatus::Abandoned => "abandoned",
            };
            query.push_str(&format!(" AND status = '{}'", status_str));
        }

        if let Some(pl) = platform {
            query.push_str(&format!(" AND platform = '{}'", pl.as_str()));
        }

        if let Some(pc) = project_context {
            query.push_str(&format!(" AND project_context = '{}'", pc));
        }

        query.push_str(&format!(" ORDER BY created_at DESC LIMIT {}", limit));

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .context("Failed to list sessions")?;

        rows.into_iter().map(|row| row_to_session(&row)).collect()
    }

    async fn find_session_by_room(&self, external_room_id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(
            r#"
            SELECT id, platform, project_context, status, summary, themes,
                   claude_session_id, external_room_id, metadata,
                   created_at, updated_at, ended_at
            FROM sessions
            WHERE external_room_id = $1 AND status = 'active'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(external_room_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to find session by room")?;

        if let Some(row) = row {
            Ok(Some(row_to_session(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn end_session(
        &self,
        id: Uuid,
        status: SessionStatus,
        summary: Option<String>,
        themes: Vec<String>,
    ) -> Result<()> {
        let status_str = match status {
            SessionStatus::Active => "active",
            SessionStatus::Completed => "completed",
            SessionStatus::Abandoned => "abandoned",
        };

        let result = sqlx::query(
            r#"
            UPDATE sessions
            SET status = $2,
                summary = COALESCE($3, summary),
                themes = CASE WHEN $4 != '{}' THEN $4 ELSE themes END,
                ended_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status_str)
        .bind(summary)
        .bind(&themes)
        .execute(&self.pool)
        .await
        .context("Failed to end session")?;

        if result.rows_affected() == 0 {
            bail!("Session not found: {}", id);
        }

        Ok(())
    }

    async fn add_exchange(&self, exchange: Exchange) -> Result<Uuid> {
        let role_str = match exchange.role {
            ExchangeRole::User => "user",
            ExchangeRole::Assistant => "assistant",
            ExchangeRole::System => "system",
        };

        sqlx::query(
            r#"
            INSERT INTO exchanges (id, session_id, role, content, tokens_approx, tool_uses, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(exchange.id)
        .bind(exchange.session_id)
        .bind(role_str)
        .bind(&exchange.content)
        .bind(exchange.tokens_approx)
        .bind(&exchange.tool_uses)
        .bind(exchange.created_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert exchange")?;

        Ok(exchange.id)
    }

    async fn list_exchanges(&self, session_id: Uuid, limit: u32, offset: u32) -> Result<Vec<Exchange>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, role, content, tokens_approx, tool_uses, created_at
            FROM exchanges
            WHERE session_id = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(session_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list exchanges")?;

        rows.into_iter().map(|row| row_to_exchange(&row)).collect()
    }

    async fn record_pattern(&self, pattern: Pattern) -> Result<Uuid> {
        let status_str = match pattern.status {
            PatternStatus::Emerging => "emerging",
            PatternStatus::Established => "established",
            PatternStatus::Fading => "fading",
            PatternStatus::Archived => "archived",
        };

        sqlx::query(
            r#"
            INSERT INTO patterns (
                id, pattern_type, description, confidence,
                evidence_session_ids, status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(pattern.id)
        .bind(&pattern.pattern_type)
        .bind(&pattern.description)
        .bind(pattern.confidence)
        .bind(&pattern.evidence_session_ids)
        .bind(status_str)
        .bind(pattern.created_at)
        .bind(pattern.updated_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert pattern")?;

        Ok(pattern.id)
    }

    async fn reinforce_pattern(&self, pattern_id: Uuid, session_id: Option<Uuid>) -> Result<()> {
        // Fetch current pattern
        let row = sqlx::query(
            r#"
            SELECT id, pattern_type, description, confidence,
                   evidence_session_ids, status, created_at, updated_at
            FROM patterns
            WHERE id = $1
            "#,
        )
        .bind(pattern_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch pattern")?;

        let row = row.ok_or_else(|| anyhow::anyhow!("Pattern not found: {}", pattern_id))?;

        let mut evidence: Vec<Uuid> = row.try_get("evidence_session_ids")?;
        let mut confidence: f64 = row.try_get("confidence")?;
        let status_str: String = row.try_get("status")?;

        // Add session to evidence if not already present
        if let Some(sid) = session_id {
            if !evidence.contains(&sid) {
                evidence.push(sid);
            }
        }

        // Increment confidence by 0.1, capped at 1.0
        confidence = (confidence + 0.1).min(1.0);

        // Update status if appropriate
        let new_status = if confidence >= 0.7 && status_str == "emerging" {
            "established"
        } else {
            status_str.as_str()
        };

        sqlx::query(
            r#"
            UPDATE patterns
            SET evidence_session_ids = $2, confidence = $3, status = $4, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(pattern_id)
        .bind(&evidence)
        .bind(confidence)
        .bind(new_status)
        .execute(&self.pool)
        .await
        .context("Failed to update pattern")?;

        Ok(())
    }

    async fn list_patterns(
        &self,
        status: Option<&str>,
        pattern_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Pattern>> {
        let mut query = String::from(
            "SELECT id, pattern_type, description, confidence,
                    evidence_session_ids, status, created_at, updated_at
             FROM patterns WHERE 1=1"
        );

        if let Some(st) = status {
            query.push_str(&format!(" AND status = '{}'", st));
        }

        if let Some(pt) = pattern_type {
            query.push_str(&format!(" AND pattern_type = '{}'", pt));
        }

        query.push_str(&format!(" ORDER BY confidence DESC LIMIT {}", limit));

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .context("Failed to list patterns")?;

        rows.into_iter().map(|row| row_to_pattern(&row)).collect()
    }

    async fn search_patterns(&self, query: &str, limit: u32) -> Result<Vec<Pattern>> {
        let rows = sqlx::query(
            r#"
            SELECT id, pattern_type, description, confidence,
                   evidence_session_ids, status, created_at, updated_at
            FROM patterns
            WHERE description ILIKE $1
            ORDER BY confidence DESC
            LIMIT $2
            "#,
        )
        .bind(format!("%{}%", query))
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to search patterns")?;

        rows.into_iter().map(|row| row_to_pattern(&row)).collect()
    }

    async fn extract_insight(&self, insight: Insight) -> Result<Uuid> {
        sqlx::query(
            r#"
            INSERT INTO insights (id, session_id, content, triple_ids, domain_path, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(insight.id)
        .bind(insight.session_id)
        .bind(&insight.content)
        .bind(&insight.triple_ids)
        .bind(&insight.domain_path)
        .bind(insight.created_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert insight")?;

        Ok(insight.id)
    }

    async fn list_insights(&self, session_id: Uuid) -> Result<Vec<Insight>> {
        let rows = sqlx::query(
            r#"
            SELECT id, session_id, content, triple_ids, domain_path, created_at
            FROM insights
            WHERE session_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to list insights")?;

        rows.into_iter().map(|row| row_to_insight(&row)).collect()
    }
}

// Helper functions to convert sqlx rows to models

fn row_to_session(row: &sqlx::postgres::PgRow) -> Result<Session> {
    let platform_str: String = row.try_get("platform")?;
    let platform = match platform_str.as_str() {
        "claude-code" => Platform::ClaudeCode,
        "api" => Platform::Api,
        "slack" => Platform::Slack,
        "claude-web" => Platform::ClaudeWeb,
        "claude-desktop" => Platform::ClaudeDesktop,
        "claude-mobile" => Platform::ClaudeMobile,
        "matrix" => Platform::Matrix,
        _ => bail!("Unknown platform: {}", platform_str),
    };

    let status_str: String = row.try_get("status")?;
    let status = match status_str.as_str() {
        "active" => SessionStatus::Active,
        "completed" => SessionStatus::Completed,
        "abandoned" => SessionStatus::Abandoned,
        _ => bail!("Unknown status: {}", status_str),
    };

    Ok(Session {
        id: row.try_get("id")?,
        platform,
        project_context: row.try_get("project_context")?,
        status,
        summary: row.try_get("summary")?,
        themes: row.try_get("themes")?,
        claude_session_id: row.try_get("claude_session_id")?,
        external_room_id: row.try_get("external_room_id")?,
        metadata: row.try_get("metadata")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        ended_at: row.try_get("ended_at")?,
    })
}

fn row_to_exchange(row: &sqlx::postgres::PgRow) -> Result<Exchange> {
    let role_str: String = row.try_get("role")?;
    let role = match role_str.as_str() {
        "user" => ExchangeRole::User,
        "assistant" => ExchangeRole::Assistant,
        "system" => ExchangeRole::System,
        _ => bail!("Unknown role: {}", role_str),
    };

    Ok(Exchange {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        role,
        content: row.try_get("content")?,
        tokens_approx: row.try_get("tokens_approx")?,
        tool_uses: row.try_get("tool_uses")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_pattern(row: &sqlx::postgres::PgRow) -> Result<Pattern> {
    let status_str: String = row.try_get("status")?;
    let status = match status_str.as_str() {
        "emerging" => PatternStatus::Emerging,
        "established" => PatternStatus::Established,
        "fading" => PatternStatus::Fading,
        "archived" => PatternStatus::Archived,
        _ => bail!("Unknown pattern status: {}", status_str),
    };

    Ok(Pattern {
        id: row.try_get("id")?,
        pattern_type: row.try_get("pattern_type")?,
        description: row.try_get("description")?,
        confidence: row.try_get("confidence")?,
        evidence_session_ids: row.try_get("evidence_session_ids")?,
        status,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_insight(row: &sqlx::postgres::PgRow) -> Result<Insight> {
    Ok(Insight {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        content: row.try_get("content")?,
        triple_ids: row.try_get("triple_ids")?,
        domain_path: row.try_get("domain_path")?,
        created_at: row.try_get("created_at")?,
    })
}
