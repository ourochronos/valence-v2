//! Supersession chain flattening for sources.
//!
//! When source A supersedes B which supersedes C, the chain is:
//!   C.superseded_by = Some(B.id)
//!   B.superseded_by = Some(A.id)
//!   A.superseded_by = None   <- head (authoritative)
//!
//! This module provides utilities to:
//! - Walk a supersession chain to find its authoritative head
//! - Enumerate the full chain for provenance purposes
//! - Apply de-ranking to triples whose supporting sources are superseded
//!
//! Cycle protection: chains longer than [`MAX_CHAIN_DEPTH`] are truncated with an
//! error rather than looping indefinitely.

use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::models::{Source, SourceId, MAX_CHAIN_DEPTH};
use crate::storage::TripleStore;

// De-rank factor applied to triples supported only by superseded sources.
// Score is multiplied by this value per superseded link in the chain.
pub const SUPERSESSION_DERANK_FACTOR: f64 = 0.1;

/// Walk a supersession chain from `start_id` to the authoritative head.
///
/// Returns the [`SourceId`] of the head — the source with `superseded_by == None`.
/// If `start_id` is already the head, returns it unchanged.
///
/// # Errors
/// - If a source ID in the chain cannot be found in the store.
/// - If the chain exceeds [`MAX_CHAIN_DEPTH`] (cycle protection).
pub async fn find_chain_head(
    store: &(impl TripleStore + ?Sized),
    start_id: SourceId,
) -> Result<SourceId> {
    let chain = walk_chain(store, start_id).await?;
    // Last element is the head (the one with no superseded_by)
    Ok(*chain.last().expect("walk_chain always returns at least one element"))
}

/// Walk the full supersession chain starting from `start_id`.
///
/// Returns an ordered `Vec` beginning with `start_id` and ending at the head.
/// Example for chain A supersedes B supersedes C, starting at C:
///   [C, B, A]   where A is the head.
///
/// # Errors
/// - If a source ID in the chain cannot be found in the store.
/// - If the chain exceeds [`MAX_CHAIN_DEPTH`] (cycle protection).
pub async fn walk_chain(
    store: &(impl TripleStore + ?Sized),
    start_id: SourceId,
) -> Result<Vec<SourceId>> {
    let mut chain: Vec<SourceId> = Vec::new();
    let mut seen: HashSet<SourceId> = HashSet::new();
    let mut current_id = start_id;

    loop {
        if seen.contains(&current_id) {
            bail!(
                "Supersession cycle detected at source {} (chain so far: {} hops)",
                current_id,
                chain.len()
            );
        }
        if chain.len() >= MAX_CHAIN_DEPTH {
            bail!(
                "Supersession chain exceeds maximum depth {} starting from {}",
                MAX_CHAIN_DEPTH,
                start_id
            );
        }

        seen.insert(current_id);
        chain.push(current_id);

        let source = store
            .get_source(current_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Source {} not found in store", current_id))?;

        match source.superseded_by {
            None => break, // current_id is the head
            Some(next_id) => current_id = next_id,
        }
    }

    Ok(chain)
}

/// Check whether a source is superseded (i.e., not the authoritative head).
///
/// This is a cheap, non-async check — it only looks at the source's own
/// `superseded_by` field without walking the chain.
pub fn is_superseded(source: &Source) -> bool {
    source.superseded_by.is_some()
}

/// Compute a de-rank multiplier for a source based on its position in the
/// supersession chain.
///
/// - Head (not superseded): multiplier = `1.0`
/// - One step from head: multiplier = `SUPERSESSION_DERANK_FACTOR`
/// - Further steps: multiplier = `SUPERSESSION_DERANK_FACTOR^depth`
///
/// This function walks the chain from `source_id` to the head and returns
/// the appropriate multiplier so callers can scale retrieval scores.
///
/// # Errors
/// Propagates errors from [`walk_chain`].
pub async fn derank_multiplier(
    store: &(impl TripleStore + ?Sized),
    source_id: SourceId,
) -> Result<f64> {
    let chain = walk_chain(store, source_id).await?;
    // chain[0] = source_id, chain[last] = head
    // depth = number of links from source_id to head
    let depth = chain.len() - 1;
    if depth == 0 {
        Ok(1.0)
    } else {
        Ok(SUPERSESSION_DERANK_FACTOR.powi(depth as i32))
    }
}

/// Compute the maximum de-rank multiplier across all sources supporting a triple.
///
/// Retrieval callers use this to scale confidence/relevance scores for triples
/// whose sources are superseded.  Returns the *highest* multiplier found (most
/// authoritative source wins).
///
/// If a triple has no sources, returns `1.0` (no penalty).
pub async fn triple_source_derank(
    store: &(impl TripleStore + ?Sized),
    triple_id: crate::models::TripleId,
) -> Result<f64> {
    let sources = store.get_sources_for_triple(triple_id).await?;
    if sources.is_empty() {
        return Ok(1.0);
    }

    let mut best = 0.0_f64;
    for source in &sources {
        let m = derank_multiplier(store, source.id).await?;
        if m > best {
            best = m;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Source, SourceType};
    use crate::storage::{MemoryStore, TripleStore};
    use crate::models::Triple;

    /// Helper: build a linear chain C <- B <- A (A is head, C is oldest).
    /// Returns (store, a_id, b_id, c_id).
    async fn build_linear_chain() -> (MemoryStore, SourceId, SourceId, SourceId) {
        let store = MemoryStore::new();

        // Create placeholder triples so sources have something to reference
        let n1 = store.find_or_create_node("N1").await.unwrap();
        let n2 = store.find_or_create_node("N2").await.unwrap();
        let tid = store.insert_triple(Triple::new(n1.id, "rel", n2.id)).await.unwrap();

        // C is the oldest (no superseded_by yet)
        let c = Source::new(vec![tid], SourceType::Conversation);
        let c_id = c.id;
        store.insert_source(c).await.unwrap();

        // B supersedes C
        let b = Source::new(vec![tid], SourceType::Conversation);
        let b_id = b.id;
        let mut b = store.get_source(b_id).await.unwrap();
        // Insert B first, then set C.superseded_by = B
        let b_raw = Source::new(vec![tid], SourceType::Conversation);
        let b_id = b_raw.id;
        store.insert_source(b_raw).await.unwrap();

        // Mutate C: superseded_by = Some(b_id)
        // MemoryStore holds a clone; we need to re-insert with updated field
        let c_updated = Source {
            id: c_id,
            triple_ids: vec![tid],
            source_type: SourceType::Conversation,
            reference: None,
            created_at: chrono::Utc::now(),
            metadata: None,
            superseded_by: Some(b_id),
        };
        // Re-insert overwrites because MemoryStore uses HashMap
        store.insert_source(c_updated).await.unwrap();

        // A supersedes B
        let a_raw = Source::new(vec![tid], SourceType::Conversation);
        let a_id = a_raw.id;
        store.insert_source(a_raw).await.unwrap();

        let b_updated = Source {
            id: b_id,
            triple_ids: vec![tid],
            source_type: SourceType::Conversation,
            reference: None,
            created_at: chrono::Utc::now(),
            metadata: None,
            superseded_by: Some(a_id),
        };
        store.insert_source(b_updated).await.unwrap();

        (store, a_id, b_id, c_id)
    }

    // ── simple linear chain ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_find_chain_head_from_oldest() {
        let (store, a_id, _b_id, c_id) = build_linear_chain().await;
        let head = find_chain_head(&store, c_id).await.unwrap();
        assert_eq!(head, a_id, "Chain head from C should be A");
    }

    #[tokio::test]
    async fn test_find_chain_head_from_middle() {
        let (store, a_id, b_id, _c_id) = build_linear_chain().await;
        let head = find_chain_head(&store, b_id).await.unwrap();
        assert_eq!(head, a_id, "Chain head from B should be A");
    }

    #[tokio::test]
    async fn test_find_chain_head_from_head() {
        let (store, a_id, _b_id, _c_id) = build_linear_chain().await;
        let head = find_chain_head(&store, a_id).await.unwrap();
        assert_eq!(head, a_id, "Chain head from A (already head) should be A");
    }

    #[tokio::test]
    async fn test_walk_chain_full_path() {
        let (store, a_id, b_id, c_id) = build_linear_chain().await;
        let chain = walk_chain(&store, c_id).await.unwrap();
        assert_eq!(chain, vec![c_id, b_id, a_id], "Chain from C should be [C, B, A]");
    }

    #[tokio::test]
    async fn test_walk_chain_from_head_is_singleton() {
        let (store, a_id, _b_id, _c_id) = build_linear_chain().await;
        let chain = walk_chain(&store, a_id).await.unwrap();
        assert_eq!(chain, vec![a_id]);
    }

    // ── is_superseded / derank_multiplier ────────────────────────────────────

    #[tokio::test]
    async fn test_is_superseded() {
        let (store, a_id, _b_id, c_id) = build_linear_chain().await;
        let a = store.get_source(a_id).await.unwrap().unwrap();
        let c = store.get_source(c_id).await.unwrap().unwrap();
        assert!(!is_superseded(&a), "A is the head, not superseded");
        assert!(is_superseded(&c), "C is superseded");
    }

    #[tokio::test]
    async fn test_derank_multiplier_head_is_1() {
        let (store, a_id, _b_id, _c_id) = build_linear_chain().await;
        let m = derank_multiplier(&store, a_id).await.unwrap();
        assert_eq!(m, 1.0);
    }

    #[tokio::test]
    async fn test_derank_multiplier_decreases_with_depth() {
        let (store, a_id, b_id, c_id) = build_linear_chain().await;
        let ma = derank_multiplier(&store, a_id).await.unwrap();
        let mb = derank_multiplier(&store, b_id).await.unwrap();
        let mc = derank_multiplier(&store, c_id).await.unwrap();
        assert_eq!(ma, 1.0);
        assert!(mb < ma, "B should have lower multiplier than A");
        assert!(mc < mb, "C should have lower multiplier than B");
        // Exact values
        assert!((mb - SUPERSESSION_DERANK_FACTOR).abs() < 1e-9);
        assert!((mc - SUPERSESSION_DERANK_FACTOR.powi(2)).abs() < 1e-9);
    }

    // ── triple de-rank ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_triple_source_derank_no_sources() {
        let store = MemoryStore::new();
        let n1 = store.find_or_create_node("X").await.unwrap();
        let n2 = store.find_or_create_node("Y").await.unwrap();
        let tid = store.insert_triple(Triple::new(n1.id, "rel", n2.id)).await.unwrap();
        let m = triple_source_derank(&store, tid).await.unwrap();
        assert_eq!(m, 1.0, "No sources -> no penalty");
    }

    #[tokio::test]
    async fn test_triple_source_derank_with_authoritative_source() {
        let (store, a_id, _b_id, _c_id) = build_linear_chain().await;
        // The triple is the one used in build_linear_chain
        let a = store.get_source(a_id).await.unwrap().unwrap();
        let tid = a.triple_ids[0];
        let m = triple_source_derank(&store, tid).await.unwrap();
        // A is the head (multiplier=1.0); other sources for the same triple are B and C
        // best multiplier across all sources = 1.0 (from A)
        assert_eq!(m, 1.0);
    }

    // ── branching chains ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_branching_chains_independent() {
        // Two independent chains:  A1<-B1   and  A2<-B2
        // Verifies that chains don't interfere.
        let store = MemoryStore::new();
        let n1 = store.find_or_create_node("P").await.unwrap();
        let n2 = store.find_or_create_node("Q").await.unwrap();
        let tid = store.insert_triple(Triple::new(n1.id, "r", n2.id)).await.unwrap();

        let a1 = Source::new(vec![tid], SourceType::Document);
        let a1_id = a1.id;
        store.insert_source(a1).await.unwrap();

        let b1 = Source {
            id: uuid::Uuid::new_v4(),
            triple_ids: vec![tid],
            source_type: SourceType::Document,
            reference: None,
            created_at: chrono::Utc::now(),
            metadata: None,
            superseded_by: Some(a1_id),
        };
        let b1_id = b1.id;
        store.insert_source(b1).await.unwrap();

        let a2 = Source::new(vec![tid], SourceType::Inference);
        let a2_id = a2.id;
        store.insert_source(a2).await.unwrap();

        let b2 = Source {
            id: uuid::Uuid::new_v4(),
            triple_ids: vec![tid],
            source_type: SourceType::Inference,
            reference: None,
            created_at: chrono::Utc::now(),
            metadata: None,
            superseded_by: Some(a2_id),
        };
        let b2_id = b2.id;
        store.insert_source(b2).await.unwrap();

        assert_eq!(find_chain_head(&store, b1_id).await.unwrap(), a1_id);
        assert_eq!(find_chain_head(&store, b2_id).await.unwrap(), a2_id);
        // Heads are different
        assert_ne!(a1_id, a2_id);
    }

    // ── circular protection ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_cycle_detection() {
        // Build a cycle: X.superseded_by = Y, Y.superseded_by = X
        let store = MemoryStore::new();
        let n1 = store.find_or_create_node("Cx").await.unwrap();
        let n2 = store.find_or_create_node("Cy").await.unwrap();
        let tid = store.insert_triple(Triple::new(n1.id, "r", n2.id)).await.unwrap();

        let x_id = uuid::Uuid::new_v4();
        let y_id = uuid::Uuid::new_v4();

        let x = Source {
            id: x_id,
            triple_ids: vec![tid],
            source_type: SourceType::UserInput,
            reference: None,
            created_at: chrono::Utc::now(),
            metadata: None,
            superseded_by: Some(y_id),
        };
        let y = Source {
            id: y_id,
            triple_ids: vec![tid],
            source_type: SourceType::UserInput,
            reference: None,
            created_at: chrono::Utc::now(),
            metadata: None,
            superseded_by: Some(x_id), // cycle!
        };
        store.insert_source(x).await.unwrap();
        store.insert_source(y).await.unwrap();

        let result = walk_chain(&store, x_id).await;
        assert!(result.is_err(), "Should detect cycle and return an error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cycle") || err.contains("Cycle"), "Error should mention cycle: {}", err);
    }
}
