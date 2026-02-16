use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "migrate-v1")]
#[command(about = "Migrate Valence v1 beliefs to v2 triples")]
struct Args {
    /// V1 PostgreSQL connection URL
    #[arg(long, env)]
    v1_url: String,

    /// V2 API base URL
    #[arg(long, env, default_value = "http://localhost:8421")]
    v2_url: String,

    /// Dry run mode (output JSON without sending to v2)
    #[arg(long)]
    dry_run: bool,

    /// Limit number of beliefs to migrate (for testing)
    #[arg(long)]
    limit: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct V1Belief {
    id: Uuid,
    content: String,
    domain_path: Vec<String>,
    confidence: JsonValue,
    created_at: chrono::DateTime<chrono::Utc>,
    source_type: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct V1Entity {
    id: Uuid,
    name: String,
    entity_type: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BeliefEntity {
    entity_id: Uuid,
    role: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Triple {
    subject: String,
    predicate: String,
    object: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<JsonValue>,
}

#[derive(Debug, Serialize)]
struct MigrationOutput {
    triples: Vec<Triple>,
    stats: MigrationStats,
}

#[derive(Debug, Serialize)]
struct MigrationStats {
    beliefs_processed: usize,
    triples_generated: usize,
    domain_nodes_created: usize,
    entity_references: usize,
}

/// Truncate content to max 200 chars for node values
fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        let truncated = &content[..max_len];
        format!("{}...", truncated)
    }
}

/// Generate a stable node ID from domain path segments
fn domain_node_id(segments: &[String]) -> String {
    format!("domain:{}", segments.join("/"))
}

/// Generate triples for a single v1 belief
fn belief_to_triples(
    belief: &V1Belief,
    entities: &[V1Entity],
    belief_entities: &[BeliefEntity],
) -> Vec<Triple> {
    let mut triples = Vec::new();
    let belief_node = format!("belief:{}", belief.id);

    // 1. Create the belief content node
    let content_truncated = truncate_content(&belief.content, 200);
    triples.push(Triple {
        subject: belief_node.clone(),
        predicate: "has_content".to_string(),
        object: content_truncated,
        metadata: None,
    });

    // 2. Add v1 provenance metadata
    let mut provenance = serde_json::Map::new();
    provenance.insert("v1_belief_id".to_string(), JsonValue::String(belief.id.to_string()));
    provenance.insert("v1_created_at".to_string(), JsonValue::String(belief.created_at.to_rfc3339()));
    if let Some(ref source_type) = belief.source_type {
        provenance.insert("v1_source_type".to_string(), JsonValue::String(source_type.clone()));
    }
    provenance.insert("v1_confidence".to_string(), belief.confidence.clone());

    triples.push(Triple {
        subject: belief_node.clone(),
        predicate: "has_provenance".to_string(),
        object: "v1_migration".to_string(),
        metadata: Some(JsonValue::Object(provenance)),
    });

    // 3. Create domain path structure
    // Create nodes for each domain segment and connect them hierarchically
    for (i, segment) in belief.domain_path.iter().enumerate() {
        let domain_segments = &belief.domain_path[..=i];
        let domain_id = domain_node_id(domain_segments);

        // Connect belief to the most specific domain (leaf node)
        if i == belief.domain_path.len() - 1 {
            triples.push(Triple {
                subject: belief_node.clone(),
                predicate: "belongs_to_domain".to_string(),
                object: domain_id.clone(),
                metadata: None,
            });
        }

        // Create domain node with label
        triples.push(Triple {
            subject: domain_id.clone(),
            predicate: "domain_label".to_string(),
            object: segment.clone(),
            metadata: None,
        });

        // Connect to parent domain (if not root)
        if i > 0 {
            let parent_segments = &belief.domain_path[..i];
            let parent_id = domain_node_id(parent_segments);
            triples.push(Triple {
                subject: domain_id,
                predicate: "subdomain_of".to_string(),
                object: parent_id,
                metadata: None,
            });
        }
    }

    // 4. Link to entities via belief_entities junction table
    for be in belief_entities {
        if let Some(entity) = entities.iter().find(|e| e.id == be.entity_id) {
            let entity_node = format!("entity:{}:{}", entity.entity_type, entity.id);

            // Create entity node with label
            triples.push(Triple {
                subject: entity_node.clone(),
                predicate: "entity_name".to_string(),
                object: entity.name.clone(),
                metadata: Some(serde_json::json!({
                    "entity_type": entity.entity_type,
                    "v1_entity_id": entity.id.to_string(),
                })),
            });

            // Link belief to entity based on role
            let predicate = match be.role.as_str() {
                "subject" => "about",
                "object" => "mentions",
                "context" => "in_context",
                "source" => "from_source",
                _ => "related_to",
            };

            triples.push(Triple {
                subject: belief_node.clone(),
                predicate: predicate.to_string(),
                object: entity_node,
                metadata: Some(serde_json::json!({"role": be.role})),
            });
        }
    }

    triples
}

async fn fetch_v1_beliefs(pool: &sqlx::PgPool, limit: Option<i64>) -> Result<Vec<V1Belief>> {
    let beliefs = if let Some(limit_val) = limit {
        sqlx::query_as::<_, V1Belief>(
            r#"
            SELECT id, content, domain_path, confidence, created_at, 
                   source_type
            FROM beliefs
            WHERE status = 'active'
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit_val)
        .fetch_all(pool)
        .await
        .context("Failed to fetch beliefs from v1 database")?
    } else {
        sqlx::query_as::<_, V1Belief>(
            r#"
            SELECT id, content, domain_path, confidence, created_at, 
                   source_type
            FROM beliefs
            WHERE status = 'active'
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await
        .context("Failed to fetch beliefs from v1 database")?
    };

    Ok(beliefs)
}

async fn fetch_belief_entities(pool: &sqlx::PgPool, belief_id: Uuid) -> Result<Vec<BeliefEntity>> {
    let entities = sqlx::query_as::<_, BeliefEntity>(
        r#"
        SELECT entity_id, role
        FROM belief_entities
        WHERE belief_id = $1
        "#,
    )
    .bind(belief_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch belief entities")?;

    Ok(entities)
}

async fn fetch_entities(pool: &sqlx::PgPool, entity_ids: &[Uuid]) -> Result<Vec<V1Entity>> {
    if entity_ids.is_empty() {
        return Ok(Vec::new());
    }

    let entities = sqlx::query_as::<_, V1Entity>(
        r#"
        SELECT id, name, type as entity_type
        FROM entities
        WHERE id = ANY($1)
        "#,
    )
    .bind(entity_ids)
    .fetch_all(pool)
    .await
    .context("Failed to fetch entities")?;

    Ok(entities)
}

async fn send_triples_to_v2(v2_url: &str, triples: &[Triple]) -> Result<()> {
    let client = reqwest::Client::new();

    // Send triples in batches (100 at a time)
    const BATCH_SIZE: usize = 100;
    for (i, chunk) in triples.chunks(BATCH_SIZE).enumerate() {
        let response = client
            .post(format!("{}/triples", v2_url))
            .json(&serde_json::json!({ "triples": chunk }))
            .send()
            .await
            .context("Failed to send triples to v2 API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "V2 API returned error for batch {}: {} - {}",
                i,
                status,
                body
            );
        }

        tracing::info!("Sent batch {} ({} triples)", i + 1, chunk.len());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    tracing::info!("Connecting to v1 database...");
    let v1_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&args.v1_url)
        .await
        .context("Failed to connect to v1 database")?;

    tracing::info!("Fetching beliefs from v1...");
    let beliefs = fetch_v1_beliefs(&v1_pool, args.limit).await?;
    tracing::info!("Found {} beliefs to migrate", beliefs.len());

    let mut all_triples = Vec::new();
    let mut domain_nodes = std::collections::HashSet::new();
    let mut entity_count = 0;

    for (idx, belief) in beliefs.iter().enumerate() {
        if idx % 100 == 0 {
            tracing::info!("Processing belief {}/{}", idx + 1, beliefs.len());
        }

        // Fetch linked entities for this belief
        let belief_entities = fetch_belief_entities(&v1_pool, belief.id).await?;
        let entity_ids: Vec<Uuid> = belief_entities.iter().map(|be| be.entity_id).collect();
        let entities = fetch_entities(&v1_pool, &entity_ids).await?;

        entity_count += entities.len();

        // Generate triples
        let triples = belief_to_triples(belief, &entities, &belief_entities);

        // Track domain nodes for stats
        for segment_idx in 0..belief.domain_path.len() {
            let segments = &belief.domain_path[..=segment_idx];
            domain_nodes.insert(domain_node_id(segments));
        }

        all_triples.extend(triples);
    }

    let stats = MigrationStats {
        beliefs_processed: beliefs.len(),
        triples_generated: all_triples.len(),
        domain_nodes_created: domain_nodes.len(),
        entity_references: entity_count,
    };

    if args.dry_run {
        // Output as JSON
        let output = MigrationOutput {
            triples: all_triples,
            stats,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Send to v2 API
        tracing::info!("Sending {} triples to v2 API...", all_triples.len());
        send_triples_to_v2(&args.v2_url, &all_triples).await?;
        tracing::info!("Migration complete!");
        tracing::info!("Stats: {:#?}", stats);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_content() {
        assert_eq!(truncate_content("short", 200), "short");
        
        let long = "a".repeat(300);
        let truncated = truncate_content(&long, 200);
        assert_eq!(truncated.len(), 203); // 200 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_domain_node_id() {
        assert_eq!(
            domain_node_id(&["tech".to_string(), "rust".to_string()]),
            "domain:tech/rust"
        );
    }

    #[test]
    fn test_belief_to_triples_basic() {
        let belief = V1Belief {
            id: Uuid::new_v4(),
            content: "Rust has memory safety".to_string(),
            domain_path: vec!["tech".to_string(), "rust".to_string()],
            confidence: serde_json::json!({"overall": 0.9}),
            created_at: chrono::Utc::now(),
            source_type: Some("manual".to_string()),
        };

        let triples = belief_to_triples(&belief, &[], &[]);

        // Should have: content triple, provenance triple, domain triples
        // Domain triples: belief->domain, domain label for each segment, parent link
        // 2 domains: "tech" and "tech/rust"
        // Triples: belongs_to_domain, 2x domain_label, 1x subdomain_of, content, provenance
        assert!(triples.len() >= 6);

        // Check content triple
        assert!(triples.iter().any(|t| t.predicate == "has_content"
            && t.object == "Rust has memory safety"));

        // Check domain connection
        assert!(triples.iter().any(|t| t.predicate == "belongs_to_domain"
            && t.object == "domain:tech/rust"));

        // Check provenance
        assert!(triples.iter().any(|t| t.predicate == "has_provenance"));
    }

    #[test]
    fn test_belief_to_triples_with_entity() {
        let belief = V1Belief {
            id: Uuid::new_v4(),
            content: "PostgreSQL uses MVCC".to_string(),
            domain_path: vec!["tech".to_string(), "database".to_string()],
            confidence: serde_json::json!({"overall": 0.85}),
            created_at: chrono::Utc::now(),
            source_type: None,
        };

        let entity_id = Uuid::new_v4();
        let entities = vec![V1Entity {
            id: entity_id,
            name: "PostgreSQL".to_string(),
            entity_type: "tool".to_string(),
        }];

        let belief_entities = vec![BeliefEntity {
            entity_id,
            role: "subject".to_string(),
        }];

        let triples = belief_to_triples(&belief, &entities, &belief_entities);

        // Should have entity node and link
        assert!(triples.iter().any(|t| t.predicate == "entity_name"
            && t.object == "PostgreSQL"));
        assert!(triples.iter().any(|t| t.predicate == "about"));
    }

    #[test]
    fn test_domain_path_decomposition() {
        let belief = V1Belief {
            id: Uuid::new_v4(),
            content: "Test content".to_string(),
            domain_path: vec![
                "tech".to_string(),
                "languages".to_string(),
                "rust".to_string(),
            ],
            confidence: serde_json::json!({"overall": 0.7}),
            created_at: chrono::Utc::now(),
            source_type: None,
        };

        let triples = belief_to_triples(&belief, &[], &[]);

        // Check that all domain nodes are created
        assert!(triples.iter().any(|t| t.subject == "domain:tech"
            && t.predicate == "domain_label"));
        assert!(triples.iter().any(|t| t.subject == "domain:tech/languages"
            && t.predicate == "domain_label"));
        assert!(triples.iter().any(|t| t.subject == "domain:tech/languages/rust"
            && t.predicate == "domain_label"));

        // Check hierarchy links
        assert!(triples.iter().any(|t| t.subject == "domain:tech/languages"
            && t.predicate == "subdomain_of"
            && t.object == "domain:tech"));
        assert!(triples.iter().any(|t| t.subject == "domain:tech/languages/rust"
            && t.predicate == "subdomain_of"
            && t.object == "domain:tech/languages"));
    }
}
