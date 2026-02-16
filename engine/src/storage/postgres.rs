#[cfg(feature = "postgres")]
use anyhow::{Context, Result};
#[cfg(feature = "postgres")]
use sqlx::{PgPool, Row};
#[cfg(feature = "postgres")]
use async_trait::async_trait;

#[cfg(feature = "postgres")]
use crate::models::{Triple, TripleId, Node, NodeId, Source, SourceId, Predicate, SourceType};
#[cfg(feature = "postgres")]
use super::traits::{TripleStore, TriplePattern};

#[cfg(feature = "postgres")]
const INIT_SQL: &str = r#"
-- Nodes table
CREATE TABLE IF NOT EXISTS nodes (
    id UUID PRIMARY KEY,
    value TEXT NOT NULL,
    node_type TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    access_count BIGINT NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_value ON nodes(value);

-- Triples table
CREATE TABLE IF NOT EXISTS triples (
    id UUID PRIMARY KEY,
    subject_id UUID NOT NULL REFERENCES nodes(id),
    predicate TEXT NOT NULL,
    object_id UUID NOT NULL REFERENCES nodes(id),
    weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    access_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_triples_spo ON triples(subject_id, predicate, object_id);
CREATE INDEX IF NOT EXISTS idx_triples_pos ON triples(predicate, object_id, subject_id);
CREATE INDEX IF NOT EXISTS idx_triples_osp ON triples(object_id, subject_id, predicate);

-- Sources table
CREATE TABLE IF NOT EXISTS sources (
    id UUID PRIMARY KEY,
    source_type TEXT NOT NULL,
    reference TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB
);

-- Junction table for source-triple relationships
CREATE TABLE IF NOT EXISTS source_triples (
    source_id UUID NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    triple_id UUID NOT NULL REFERENCES triples(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, triple_id)
);
CREATE INDEX IF NOT EXISTS idx_source_triples_triple ON source_triples(triple_id);
"#;

#[cfg(feature = "postgres")]
/// PostgreSQL-backed implementation of TripleStore using sqlx.
pub struct PgStore {
    pool: PgPool,
}

#[cfg(feature = "postgres")]
impl PgStore {
    /// Create a new PgStore with the given database URL.
    /// Initializes the schema if needed.
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;
        
        // Run migrations
        sqlx::raw_sql(INIT_SQL)
            .execute(&pool)
            .await
            .context("Failed to initialize database schema")?;
        
        Ok(Self { pool })
    }
    
    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl TripleStore for PgStore {
    async fn insert_node(&self, node: Node) -> Result<NodeId> {
        sqlx::query(
            r#"
            INSERT INTO nodes (id, value, node_type, created_at, last_accessed, access_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
            "#
        )
        .bind(node.id)
        .bind(&node.value)
        .bind(&node.node_type)
        .bind(node.created_at)
        .bind(node.last_accessed)
        .bind(node.access_count as i64)
        .execute(&self.pool)
        .await
        .context("Failed to insert node")?;
        
        Ok(node.id)
    }

    async fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        let row = sqlx::query(
            r#"
            SELECT id, value, node_type, created_at, last_accessed, access_count
            FROM nodes
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get node")?;
        
        Ok(row.map(|r| Node {
            id: r.get("id"),
            value: r.get("value"),
            node_type: r.get("node_type"),
            created_at: r.get("created_at"),
            last_accessed: r.get("last_accessed"),
            access_count: r.get::<i64, _>("access_count") as u64,
        }))
    }

    async fn find_node_by_value(&self, value: &str) -> Result<Option<Node>> {
        let row = sqlx::query(
            r#"
            SELECT id, value, node_type, created_at, last_accessed, access_count
            FROM nodes
            WHERE value = $1
            "#
        )
        .bind(value)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to find node by value")?;
        
        Ok(row.map(|r| Node {
            id: r.get("id"),
            value: r.get("value"),
            node_type: r.get("node_type"),
            created_at: r.get("created_at"),
            last_accessed: r.get("last_accessed"),
            access_count: r.get::<i64, _>("access_count") as u64,
        }))
    }

    async fn find_or_create_node(&self, value: &str) -> Result<Node> {
        // Try to find existing node
        if let Some(node) = self.find_node_by_value(value).await? {
            return Ok(node);
        }
        
        // Create new node
        let node = Node::new(value);
        self.insert_node(node.clone()).await?;
        Ok(node)
    }

    async fn insert_triple(&self, triple: Triple) -> Result<TripleId> {
        sqlx::query(
            r#"
            INSERT INTO triples (id, subject_id, predicate, object_id, weight, access_count, created_at, last_accessed)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO NOTHING
            "#
        )
        .bind(triple.id)
        .bind(triple.subject)
        .bind(&triple.predicate.value)
        .bind(triple.object)
        .bind(triple.weight)
        .bind(triple.access_count as i64)
        .bind(triple.created_at)
        .bind(triple.last_accessed)
        .execute(&self.pool)
        .await
        .context("Failed to insert triple")?;
        
        Ok(triple.id)
    }

    async fn get_triple(&self, id: TripleId) -> Result<Option<Triple>> {
        let row = sqlx::query(
            r#"
            SELECT id, subject_id, predicate, object_id, weight, access_count, created_at, last_accessed
            FROM triples
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get triple")?;
        
        Ok(row.map(|r| Triple {
            id: r.get("id"),
            subject: r.get("subject_id"),
            predicate: Predicate::new(r.get::<String, _>("predicate")),
            object: r.get("object_id"),
            weight: r.get("weight"),
            access_count: r.get::<i64, _>("access_count") as u64,
            created_at: r.get("created_at"),
            last_accessed: r.get("last_accessed"),
        }))
    }

    async fn update_triple(&self, triple: Triple) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE triples
            SET subject_id = $2, predicate = $3, object_id = $4, weight = $5,
                access_count = $6, last_accessed = $7
            WHERE id = $1
            "#
        )
        .bind(triple.id)
        .bind(triple.subject)
        .bind(&triple.predicate.value)
        .bind(triple.object)
        .bind(triple.weight)
        .bind(triple.access_count as i64)
        .bind(triple.last_accessed)
        .execute(&self.pool)
        .await
        .context("Failed to update triple")?;
        
        Ok(())
    }

    async fn query_triples(&self, pattern: TriplePattern) -> Result<Vec<Triple>> {
        let mut query = String::from(
            "SELECT id, subject_id, predicate, object_id, weight, access_count, created_at, last_accessed FROM triples WHERE 1=1"
        );
        
        if pattern.subject.is_some() {
            query.push_str(" AND subject_id = $1");
        }
        if pattern.predicate.is_some() {
            let param_num = if pattern.subject.is_some() { 2 } else { 1 };
            query.push_str(&format!(" AND predicate = ${}", param_num));
        }
        if pattern.object.is_some() {
            let param_num = match (pattern.subject.is_some(), pattern.predicate.is_some()) {
                (true, true) => 3,
                (true, false) | (false, true) => 2,
                (false, false) => 1,
            };
            query.push_str(&format!(" AND object_id = ${}", param_num));
        }
        
        let mut q = sqlx::query(&query);
        
        if let Some(subj) = pattern.subject {
            q = q.bind(subj);
        }
        if let Some(pred) = pattern.predicate {
            q = q.bind(pred);
        }
        if let Some(obj) = pattern.object {
            q = q.bind(obj);
        }
        
        let rows = q
            .fetch_all(&self.pool)
            .await
            .context("Failed to query triples")?;
        
        Ok(rows.into_iter().map(|r| Triple {
            id: r.get("id"),
            subject: r.get("subject_id"),
            predicate: Predicate::new(r.get::<String, _>("predicate")),
            object: r.get("object_id"),
            weight: r.get("weight"),
            access_count: r.get::<i64, _>("access_count") as u64,
            created_at: r.get("created_at"),
            last_accessed: r.get("last_accessed"),
        }).collect())
    }

    async fn touch_triple(&self, id: TripleId) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE triples
            SET last_accessed = NOW(),
                access_count = access_count + 1,
                weight = 1.0
            WHERE id = $1
            "#
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to touch triple")?;
        
        Ok(())
    }

    async fn delete_triple(&self, id: TripleId) -> Result<()> {
        sqlx::query("DELETE FROM triples WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete triple")?;
        
        Ok(())
    }

    async fn insert_source(&self, source: Source) -> Result<SourceId> {
        let source_type_str = match source.source_type {
            SourceType::Conversation => "conversation",
            SourceType::Observation => "observation",
            SourceType::Inference => "inference",
            SourceType::UserInput => "user_input",
            SourceType::Document => "document",
            SourceType::Decomposition => "decomposition",
        };
        
        // Insert source
        sqlx::query(
            r#"
            INSERT INTO sources (id, source_type, reference, created_at, metadata)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO NOTHING
            "#
        )
        .bind(source.id)
        .bind(source_type_str)
        .bind(&source.reference)
        .bind(source.created_at)
        .bind(&source.metadata)
        .execute(&self.pool)
        .await
        .context("Failed to insert source")?;
        
        // Insert source-triple relationships
        for triple_id in &source.triple_ids {
            sqlx::query(
                "INSERT INTO source_triples (source_id, triple_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
            )
            .bind(source.id)
            .bind(triple_id)
            .execute(&self.pool)
            .await
            .context("Failed to insert source-triple relationship")?;
        }
        
        Ok(source.id)
    }

    async fn get_sources_for_triple(&self, triple_id: TripleId) -> Result<Vec<Source>> {
        let rows = sqlx::query(
            r#"
            SELECT s.id, s.source_type, s.reference, s.created_at, s.metadata
            FROM sources s
            JOIN source_triples st ON s.id = st.source_id
            WHERE st.triple_id = $1
            "#
        )
        .bind(triple_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to get sources for triple")?;
        
        let mut sources = Vec::new();
        for row in rows {
            let source_id: SourceId = row.get("id");
            
            // Get all triple IDs for this source
            let triple_rows = sqlx::query(
                "SELECT triple_id FROM source_triples WHERE source_id = $1"
            )
            .bind(source_id)
            .fetch_all(&self.pool)
            .await
            .context("Failed to get triple IDs for source")?;
            
            let triple_ids: Vec<TripleId> = triple_rows
                .into_iter()
                .map(|r| r.get("triple_id"))
                .collect();
            
            let source_type_str: String = row.get("source_type");
            let source_type = match source_type_str.as_str() {
                "conversation" => SourceType::Conversation,
                "observation" => SourceType::Observation,
                "inference" => SourceType::Inference,
                "user_input" => SourceType::UserInput,
                "document" => SourceType::Document,
                "decomposition" => SourceType::Decomposition,
                _ => SourceType::UserInput, // Default fallback
            };
            
            sources.push(Source {
                id: source_id,
                triple_ids,
                source_type,
                reference: row.get("reference"),
                created_at: row.get("created_at"),
                metadata: row.get("metadata"),
            });
        }
        
        Ok(sources)
    }

    async fn neighbors(&self, node_id: NodeId, depth: u32) -> Result<Vec<Triple>> {
        if depth == 0 {
            return Ok(Vec::new());
        }
        
        // Use recursive CTE to find neighbors up to specified depth
        let query = r#"
            WITH RECURSIVE neighbor_triples AS (
                -- Base case: direct neighbors
                SELECT t.id, t.subject_id, t.predicate, t.object_id, t.weight, 
                       t.access_count, t.created_at, t.last_accessed, 1 as depth
                FROM triples t
                WHERE t.subject_id = $1 OR t.object_id = $1
                
                UNION
                
                -- Recursive case: neighbors of neighbors
                SELECT t.id, t.subject_id, t.predicate, t.object_id, t.weight,
                       t.access_count, t.created_at, t.last_accessed, nt.depth + 1
                FROM triples t
                JOIN neighbor_triples nt ON (t.subject_id = nt.subject_id OR t.subject_id = nt.object_id
                                            OR t.object_id = nt.subject_id OR t.object_id = nt.object_id)
                WHERE nt.depth < $2
                    AND t.id NOT IN (SELECT id FROM neighbor_triples)
            )
            SELECT DISTINCT id, subject_id, predicate, object_id, weight, access_count, created_at, last_accessed
            FROM neighbor_triples
        "#;
        
        let rows = sqlx::query(query)
            .bind(node_id)
            .bind(depth as i32)
            .fetch_all(&self.pool)
            .await
            .context("Failed to get neighbors")?;
        
        Ok(rows.into_iter().map(|r| Triple {
            id: r.get("id"),
            subject: r.get("subject_id"),
            predicate: Predicate::new(r.get::<String, _>("predicate")),
            object: r.get("object_id"),
            weight: r.get("weight"),
            access_count: r.get::<i64, _>("access_count") as u64,
            created_at: r.get("created_at"),
            last_accessed: r.get("last_accessed"),
        }).collect())
    }

    async fn count_triples(&self) -> Result<u64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM triples")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count triples")?;
        
        Ok(row.get::<i64, _>("count") as u64)
    }

    async fn count_nodes(&self) -> Result<u64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM nodes")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count nodes")?;
        
        Ok(row.get::<i64, _>("count") as u64)
    }

    async fn decay(&self, factor: f64, min_weight: f64) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE triples
            SET weight = GREATEST(weight * $1, $2)
            WHERE weight >= $2
            "#
        )
        .bind(factor)
        .bind(min_weight)
        .execute(&self.pool)
        .await
        .context("Failed to decay triples")?;
        
        Ok(result.rows_affected())
    }

    async fn evict_below_weight(&self, threshold: f64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM triples WHERE weight < $1")
            .bind(threshold)
            .execute(&self.pool)
            .await
            .context("Failed to evict triples")?;
        
        Ok(result.rows_affected())
    }
}

#[cfg(all(test, feature = "postgres"))]
mod tests {
    use super::*;
    use crate::models::{Node, Triple, Source, SourceType};
    
    const TEST_DATABASE_URL: &str = "postgresql://valence:valence@localhost:5433/valence_v2_test";
    
    async fn setup_test_db() -> Result<PgStore> {
        // Connect to postgres database to create test database
        let admin_url = "postgresql://valence:valence@localhost:5433/postgres";
        let admin_pool = PgPool::connect(admin_url).await?;
        
        // Drop and recreate test database
        let _ = sqlx::query("DROP DATABASE IF EXISTS valence_v2_test")
            .execute(&admin_pool)
            .await;
        
        sqlx::query("CREATE DATABASE valence_v2_test")
            .execute(&admin_pool)
            .await
            .ok();
        
        admin_pool.close().await;
        
        // Connect to test database and initialize schema
        PgStore::new(TEST_DATABASE_URL).await
    }
    
    #[tokio::test]
    async fn test_insert_and_retrieve_node() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        let node = Node::new("test_value");
        let id = store.insert_node(node.clone()).await.unwrap();
        
        let retrieved = store.get_node(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, "test_value");
    }
    
    #[tokio::test]
    async fn test_find_node_by_value() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        let node = Node::new("unique_value");
        store.insert_node(node.clone()).await.unwrap();
        
        let found = store.find_node_by_value("unique_value").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().value, "unique_value");
        
        let not_found = store.find_node_by_value("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }
    
    #[tokio::test]
    async fn test_find_or_create_node() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        
        let node1 = store.find_or_create_node("test").await.unwrap();
        let node2 = store.find_or_create_node("test").await.unwrap();
        
        assert_eq!(node1.id, node2.id);
        assert_eq!(store.count_nodes().await.unwrap(), 1);
    }
    
    #[tokio::test]
    async fn test_insert_and_query_triples() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        
        let subj = Node::new("Alice");
        let obj = Node::new("Bob");
        let subj_id = store.insert_node(subj).await.unwrap();
        let obj_id = store.insert_node(obj).await.unwrap();
        
        let triple = Triple::new(subj_id, "knows", obj_id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        // Query by subject
        let pattern = TriplePattern {
            subject: Some(subj_id),
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, triple_id);
        
        // Query by predicate
        let pattern = TriplePattern {
            subject: None,
            predicate: Some("knows".to_string()),
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
        
        // Query by object
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: Some(obj_id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
    }
    
    #[tokio::test]
    async fn test_touch_triple() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        
        let subj = Node::new("A");
        let obj = Node::new("B");
        let subj_id = store.insert_node(subj).await.unwrap();
        let obj_id = store.insert_node(obj).await.unwrap();
        
        let triple = Triple::new(subj_id, "rel", obj_id);
        let triple_id = triple.id;
        store.insert_triple(triple).await.unwrap();
        
        let before = store.get_triple(triple_id).await.unwrap().unwrap();
        let access_count_before = before.access_count;
        
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        store.touch_triple(triple_id).await.unwrap();
        
        let after = store.get_triple(triple_id).await.unwrap().unwrap();
        assert_eq!(after.access_count, access_count_before + 1);
        assert!(after.last_accessed > before.last_accessed);
        assert_eq!(after.weight, 1.0);
    }
    
    #[tokio::test]
    async fn test_neighbors() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        
        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let carol = store.find_or_create_node("Carol").await.unwrap();
        
        let t1 = Triple::new(alice.id, "knows", bob.id);
        let t2 = Triple::new(bob.id, "knows", carol.id);
        
        store.insert_triple(t1.clone()).await.unwrap();
        store.insert_triple(t2.clone()).await.unwrap();
        
        // Depth 1: Alice knows Bob
        let neighbors = store.neighbors(alice.id, 1).await.unwrap();
        assert_eq!(neighbors.len(), 1);
        
        // Depth 2: Alice -> Bob -> Carol
        let neighbors = store.neighbors(alice.id, 2).await.unwrap();
        assert_eq!(neighbors.len(), 2);
    }
    
    #[tokio::test]
    async fn test_decay_and_eviction() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        store.insert_triple(triple.clone()).await.unwrap();
        
        // Initial weight should be 1.0
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 1.0);
        
        // Decay by 0.5
        store.decay(0.5, 0.0).await.unwrap();
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 0.5);
        
        // Decay again
        store.decay(0.5, 0.0).await.unwrap();
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.weight, 0.25);
        
        // Evict below 0.3
        let evicted = store.evict_below_weight(0.3).await.unwrap();
        assert_eq!(evicted, 1);
        assert_eq!(store.count_triples().await.unwrap(), 0);
    }
    
    #[tokio::test]
    async fn test_source_tracking() {
        if std::env::var("RUN_PG_TESTS").is_err() {
            eprintln!("Skipping PostgreSQL test (set RUN_PG_TESTS=1 to run)");
            return;
        }
        
        let store = setup_test_db().await.unwrap();
        
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        
        let triple = Triple::new(a.id, "rel", b.id);
        let triple_id = store.insert_triple(triple).await.unwrap();
        
        let source = Source::new(vec![triple_id], SourceType::UserInput)
            .with_reference("user-123");
        
        store.insert_source(source.clone()).await.unwrap();
        
        let sources = store.get_sources_for_triple(triple_id).await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_type, SourceType::UserInput);
        assert_eq!(sources[0].reference.as_deref(), Some("user-123"));
    }
}
