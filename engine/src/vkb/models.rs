use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Session ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    ClaudeCode,
    Api,
    Slack,
    ClaudeWeb,
    ClaudeDesktop,
    ClaudeMobile,
    Matrix,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Api => "api",
            Self::Slack => "slack",
            Self::ClaudeWeb => "claude-web",
            Self::ClaudeDesktop => "claude-desktop",
            Self::ClaudeMobile => "claude-mobile",
            Self::Matrix => "matrix",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub platform: Platform,
    pub project_context: Option<String>,
    pub status: SessionStatus,
    pub summary: Option<String>,
    pub themes: Vec<String>,
    pub claude_session_id: Option<String>,
    pub external_room_id: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Session {
    pub fn new(platform: Platform) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            platform,
            project_context: None,
            status: SessionStatus::Active,
            summary: None,
            themes: Vec::new(),
            claude_session_id: None,
            external_room_id: None,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            created_at: now,
            updated_at: now,
            ended_at: None,
        }
    }
}

// --- Exchange ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exchange {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: ExchangeRole,
    pub content: String,
    pub tokens_approx: Option<i32>,
    pub tool_uses: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl Exchange {
    pub fn new(session_id: Uuid, role: ExchangeRole, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            role,
            content: content.into(),
            tokens_approx: None,
            tool_uses: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

// --- Pattern ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternStatus {
    Emerging,
    Established,
    Fading,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: Uuid,
    pub pattern_type: String,
    pub description: String,
    pub confidence: f64,
    pub evidence_session_ids: Vec<Uuid>,
    pub status: PatternStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Pattern {
    pub fn new(pattern_type: impl Into<String>, description: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            pattern_type: pattern_type.into(),
            description: description.into(),
            confidence: 0.4, // Start at 0.4 for new patterns
            evidence_session_ids: Vec::new(),
            status: PatternStatus::Emerging,
            created_at: now,
            updated_at: now,
        }
    }
}

// --- Insight ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub id: Uuid,
    pub session_id: Uuid,
    pub content: String,
    pub triple_ids: Vec<Uuid>,
    pub domain_path: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl Insight {
    pub fn new(session_id: Uuid, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            content: content.into(),
            triple_ids: Vec::new(),
            domain_path: Vec::new(),
            created_at: Utc::now(),
        }
    }
}
