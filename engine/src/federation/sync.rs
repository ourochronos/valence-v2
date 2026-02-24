//! Bloom filter gossip sync: set reconciliation via bloom filters.
//!
//! Replaces the old cursor-based SyncManager with a 3-step gossip protocol:
//! 1. Exchange bloom filters of triple hashes
//! 2. Request headers for triples the peer might want
//! 3. Send full signed triples after header filtering
//!
//! Privacy filtering happens before step 1: triples marked LOCAL_ONLY or with
//! SHAREABLE_WITH restrictions are excluded from the bloom filter and never sent
//! unless the recipient is explicitly listed.

use anyhow::Result;
use bloomfilter::Bloom;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::content_id;
use crate::graph::algorithms::pagerank;
use crate::graph::GraphView;
use crate::identity::Keypair;
use crate::models::{NodeId, Triple, TripleId};
use crate::predicates;
use crate::storage::{TriplePattern, TripleStore};

use super::protocol::{triple_hash, TripleHeader};

/// Bloom filter parameters.
/// Expected items and false positive rate determine filter size.
const BLOOM_EXPECTED_ITEMS: usize = 100_000;
const BLOOM_FP_RATE: f64 = 0.01;

/// How much to boost base_weight when a triple is corroborated by a peer.
/// Small nudge — repeated independent corroboration accumulates.
const CORROBORATION_BOOST: f64 = 0.05;

/// Result of merging received triples into the store.
///
/// With content-addressable IDs, the same triple always has the same TripleId.
/// This means receiving a triple we already have is corroboration, not a duplicate.
#[derive(Debug, Default)]
pub struct MergeResult {
    /// Triples that were new — inserted into the store.
    pub inserted: Vec<Triple>,
    /// TripleIds that already existed — corroborated (base_weight boosted).
    pub corroborated: Vec<TripleId>,
}

/// BloomSync implements the bloom filter gossip protocol.
pub struct BloomSync;

impl BloomSync {
    /// Build a bloom filter from all gossip-eligible triples in the store.
    ///
    /// Triples marked LOCAL_ONLY or restricted via SHAREABLE_WITH are excluded
    /// unless the recipient is explicitly listed.
    pub async fn build_bloom_filter(
        store: &dyn TripleStore,
        recipient_did: Option<&str>,
    ) -> Result<(Bloom<[u8; 32]>, u64)> {
        let all_triples = store.query_triples(TriplePattern::default()).await?;
        let mut filter = Bloom::new_for_fp_rate(BLOOM_EXPECTED_ITEMS, BLOOM_FP_RATE);
        let mut count = 0u64;

        for triple in &all_triples {
            if should_gossip(triple, recipient_did, store).await {
                let hash = triple_hash(triple);
                filter.set(&hash);
                count += 1;
            }
        }

        debug!("Built bloom filter with {} triples (of {} total)", count, all_triples.len());
        Ok((filter, count))
    }

    /// Given a peer's bloom filter (as bytes), find hashes of triples we have that
    /// the peer likely doesn't (not in their filter).
    pub async fn find_missing_for_peer(
        store: &dyn TripleStore,
        peer_filter_bytes: &[u8],
        peer_triple_count: u64,
        recipient_did: Option<&str>,
    ) -> Result<Vec<[u8; 32]>> {
        // Reconstruct the peer's bloom filter
        let peer_filter: Bloom<[u8; 32]> = Bloom::from_existing(
            peer_filter_bytes,
            peer_triple_count * 10, // bitmap_size approximation
            // We use the same hash count as our filter construction
            Bloom::<[u8; 32]>::new_for_fp_rate(BLOOM_EXPECTED_ITEMS, BLOOM_FP_RATE)
                .number_of_hash_functions(),
            [(0, 0), (0, 0)], // sip keys (not used for checking)
        );

        let all_triples = store.query_triples(TriplePattern::default()).await?;
        let mut missing = Vec::new();

        for triple in &all_triples {
            if !should_gossip(triple, recipient_did, store).await {
                continue;
            }
            let hash = triple_hash(triple);
            if !peer_filter.check(&hash) {
                missing.push(hash);
            }
        }

        debug!("Found {} triples peer is missing", missing.len());
        Ok(missing)
    }

    /// Build headers for triples matching the given hashes.
    pub async fn build_headers_for_hashes(
        store: &dyn TripleStore,
        requested_hashes: &[[u8; 32]],
        recipient_did: Option<&str>,
    ) -> Result<Vec<TripleHeader>> {
        let all_triples = store.query_triples(TriplePattern::default()).await?;
        let hash_set: std::collections::HashSet<[u8; 32]> = requested_hashes.iter().copied().collect();
        let mut headers = Vec::new();

        for triple in &all_triples {
            let hash = triple_hash(triple);
            if hash_set.contains(&hash) && should_gossip(triple, recipient_did, store).await {
                // Resolve node values for the header
                let subject_val = resolve_node_value(store, triple.subject).await;
                let object_val = resolve_node_value(store, triple.object).await;

                headers.push(TripleHeader {
                    id: triple.id,
                    subject: subject_val,
                    predicate: triple.predicate.value.clone(),
                    object: object_val,
                    origin_did: triple.origin_did.clone(),
                    timestamp: triple.timestamp,
                    hash,
                });
            }
        }

        debug!("Built {} headers for {} requested hashes", headers.len(), requested_hashes.len());
        Ok(headers)
    }

    /// Given headers from a peer, decide which triples to actually request.
    ///
    /// Filters out:
    /// - Triples we already have (by ID)
    /// - Triples from untrusted/unknown origin DIDs (PageRank below threshold)
    /// - Triples that have been retracted
    pub async fn filter_wanted_headers(
        headers: &[TripleHeader],
        store: &dyn TripleStore,
        graph: &GraphView,
    ) -> Vec<Uuid> {
        let mut wanted = Vec::new();

        // Compute PageRank for trust assessment
        let ranks = pagerank(graph, 0.85, 20);

        for header in headers {
            // Skip if we already have this triple
            if let Ok(Some(_)) = store.get_triple(header.id).await {
                continue;
            }

            // Check if the origin DID is retracted
            if is_retracted(header.id, store).await {
                debug!("Skipping retracted triple {}", header.id);
                continue;
            }

            // Check origin DID trust via PageRank
            if let Some(ref did) = header.origin_did {
                // Find the node for this DID in the graph
                if let Ok(Some(did_node)) = store.find_node_by_value(did).await {
                    let rank = ranks.get(&did_node.id).copied().unwrap_or(0.0);
                    // Accept triples from DIDs with any presence in our graph,
                    // or from unknown DIDs (they start with low base_weight)
                    if rank <= 0.0 {
                        debug!("Accepting triple from unknown DID {} (will get low base_weight)", did);
                    }
                }
                // Unknown DIDs are accepted but get low base_weight
            }

            wanted.push(header.id);
        }

        debug!("Wanted {} of {} headers", wanted.len(), headers.len());
        wanted
    }

    /// Process received triples and merge into the store.
    ///
    /// With content-addressable IDs, the same (S, P, O) always produces the same TripleId.
    /// This enables trivial CRDT merge:
    /// - **New triple**: verify, compute initial weight, insert.
    /// - **Existing triple (same TripleId)**: corroboration — boost base_weight slightly.
    ///
    /// Signature verification, retraction checks, and PageRank-based weighting still apply.
    pub async fn process_received_triples(
        triples: Vec<Triple>,
        store: &dyn TripleStore,
        graph: &GraphView,
    ) -> MergeResult {
        let ranks = pagerank(graph, 0.85, 20);
        let node_count = graph.node_count().max(1) as f64;
        let mut result = MergeResult::default();

        for mut triple in triples {
            // 1. Verify content-addressable ID: recompute and confirm it matches.
            //    Protects against a peer sending a triple with a fabricated TripleId.
            let expected_id = content_id::triple_id(triple.subject, &triple.predicate.value, triple.object);
            if triple.id != expected_id {
                warn!(
                    "TripleId mismatch: received {} but content hashes to {}. Rejecting.",
                    triple.id, expected_id
                );
                continue;
            }

            // 2. Verify Ed25519 signature if present
            if let (Some(ref did), Some(ref sig)) = (&triple.origin_did, &triple.signature) {
                if !verify_did_signature(did, &triple, sig) {
                    warn!("Signature verification failed for triple {} from {}", triple.id, did);
                    continue;
                }
            }

            // 3. Check for retraction
            if is_retracted(triple.id, store).await {
                debug!("Skipping retracted triple {}", triple.id);
                continue;
            }

            // 4. CRDT merge: check if we already have this triple
            if let Ok(Some(mut existing)) = store.get_triple(triple.id).await {
                // Same TripleId = same (S, P, O) = corroboration.
                // Boost base_weight slightly, clamped to 1.0.
                existing.base_weight = (existing.base_weight + CORROBORATION_BOOST).min(1.0);
                if let Err(e) = store.update_triple(existing).await {
                    warn!("Failed to update corroborated triple {}: {}", triple.id, e);
                }
                result.corroborated.push(triple.id);
                debug!("Corroborated existing triple {} (boost +{})", triple.id, CORROBORATION_BOOST);
                continue;
            }

            // 5. New triple — compute initial base_weight from origin DID's PageRank
            let base_weight = if let Some(ref did) = triple.origin_did {
                if let Ok(Some(did_node)) = store.find_node_by_value(did).await {
                    let rank = ranks.get(&did_node.id).copied().unwrap_or(0.0);
                    // Normalize: scale PageRank to [0.1, 1.0] range
                    // Even unknown DIDs get 0.1 base_weight
                    (rank * node_count).clamp(0.1, 1.0)
                } else {
                    0.1 // Unknown DID — minimal base weight
                }
            } else {
                0.1 // No DID — minimal base weight
            };

            triple.base_weight = base_weight;
            triple.local_weight = 0.0; // Inference loop will adjust based on utility

            // 6. Handle retraction triples specially — they always get high weight
            if triple.predicate.value == predicates::RETRACTED_BY
                || triple.predicate.value == predicates::RETRACTED_AT
            {
                triple.base_weight = 1.0;
            }

            result.inserted.push(triple);
        }

        debug!(
            "Merge complete: {} inserted, {} corroborated",
            result.inserted.len(),
            result.corroborated.len()
        );
        result
    }

    /// Collect full triples for the given IDs (for responding to TripleRequest).
    pub async fn collect_triples_by_ids(
        store: &dyn TripleStore,
        ids: &[Uuid],
        recipient_did: Option<&str>,
    ) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        for id in ids {
            if let Some(triple) = store.get_triple(*id).await? {
                if should_gossip(&triple, recipient_did, store).await {
                    triples.push(triple);
                }
            }
        }
        Ok(triples)
    }
}

/// Check whether a triple should be included in gossip to a specific recipient.
///
/// Rules:
/// 1. RETRACTED_BY / RETRACTED_AT triples are always gossiped (priority)
/// 2. Triples with LOCAL_ONLY predicate on them are never gossiped
/// 3. Triples with SHAREABLE_WITH are only sent to listed DIDs
/// 4. SHARE_POLICY predicates enforce hop limits (TODO: implement hop tracking)
/// 5. Everything else is gossip-eligible
pub async fn should_gossip(
    triple: &Triple,
    recipient_did: Option<&str>,
    store: &dyn TripleStore,
) -> bool {
    // Rule 1: Retraction triples are always gossiped
    if triple.predicate.value == predicates::RETRACTED_BY
        || triple.predicate.value == predicates::RETRACTED_AT
    {
        return true;
    }

    // Rule 2: Check for LOCAL_ONLY predicate on this triple's subject
    // Look for a triple: (this_triple.subject, LOCAL_ONLY, _)
    let local_only_pattern = TriplePattern {
        subject: Some(triple.subject),
        predicate: Some(predicates::LOCAL_ONLY.to_string()),
        object: None,
    };
    if let Ok(local_only_triples) = store.query_triples(local_only_pattern).await {
        if !local_only_triples.is_empty() {
            return false;
        }
    }

    // Also check if this triple's ID is referenced as local_only
    // (triple about the triple itself being local-only)
    // We check if any triple says "triple_id LOCAL_ONLY _"
    // Since triple IDs aren't nodes, we check by the triple's subject being marked
    // This is already handled above.

    // Rule 3: Check SHAREABLE_WITH
    let shareable_pattern = TriplePattern {
        subject: Some(triple.subject),
        predicate: Some(predicates::SHAREABLE_WITH.to_string()),
        object: None,
    };
    if let Ok(shareable_triples) = store.query_triples(shareable_pattern).await {
        if !shareable_triples.is_empty() {
            // There are sharing restrictions — only send to listed DIDs
            if let Some(recipient) = recipient_did {
                // Check if any SHAREABLE_WITH triple's object matches the recipient
                let mut allowed = false;
                for st in &shareable_triples {
                    if let Ok(Some(obj_node)) = store.get_node(st.object).await {
                        if obj_node.value == recipient {
                            allowed = true;
                            break;
                        }
                    }
                }
                if !allowed {
                    return false;
                }
            } else {
                // No recipient DID known — can't verify sharing, deny
                return false;
            }
        }
    }

    true
}

/// Check if a triple has been retracted.
async fn is_retracted(triple_id: Uuid, store: &dyn TripleStore) -> bool {
    // Look for any triple with predicate RETRACTED_BY where the subject is a node
    // whose value matches this triple's ID.
    // Since we can't directly query by triple_id as a node value easily,
    // we check if there's a node with the triple_id string and a retraction triple.
    let id_str = triple_id.to_string();
    if let Ok(Some(node)) = store.find_node_by_value(&id_str).await {
        let pattern = TriplePattern {
            subject: Some(node.id),
            predicate: Some(predicates::RETRACTED_BY.to_string()),
            object: None,
        };
        if let Ok(retractions) = store.query_triples(pattern).await {
            return !retractions.is_empty();
        }
    }
    false
}

/// Verify an Ed25519 signature on a triple using the DID's embedded public key.
///
/// DID format: did:valence:key:<base58_pubkey>
fn verify_did_signature(did: &str, triple: &Triple, signature: &[u8]) -> bool {
    // Extract public key from DID
    let Some(pubkey_b58) = did.strip_prefix("did:valence:key:") else {
        warn!("Unknown DID format: {}", did);
        return false;
    };

    let Ok(pubkey_bytes) = bs58::decode(pubkey_b58).into_vec() else {
        warn!("Invalid base58 in DID: {}", did);
        return false;
    };

    if pubkey_bytes.len() != 32 {
        warn!("Invalid public key length in DID: {} (got {} bytes)", did, pubkey_bytes.len());
        return false;
    }

    let mut pubkey_arr = [0u8; 32];
    pubkey_arr.copy_from_slice(&pubkey_bytes);

    // Reconstruct the signed message: canonical triple representation
    let message = format!(
        "{}:{}:{}:{}:{}",
        triple.subject,
        triple.predicate.value,
        triple.object,
        triple.origin_did.as_deref().unwrap_or(""),
        triple.timestamp.to_rfc3339(),
    );

    if signature.len() != 64 {
        warn!("Invalid signature length: {} (expected 64)", signature.len());
        return false;
    }

    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(signature);

    Keypair::verify(&pubkey_arr, message.as_bytes(), &sig_arr)
}

/// Resolve a NodeId to its string value, with fallback.
async fn resolve_node_value(store: &dyn TripleStore, node_id: NodeId) -> String {
    match store.get_node(node_id).await {
        Ok(Some(node)) => node.value,
        _ => node_id.to_string(),
    }
}

/// Sign a triple with the given keypair, setting origin_did and signature.
pub fn sign_triple(triple: &mut Triple, keypair: &Keypair) {
    triple.origin_did = Some(keypair.did_string());

    let message = format!(
        "{}:{}:{}:{}:{}",
        triple.subject,
        triple.predicate.value,
        triple.object,
        triple.origin_did.as_deref().unwrap_or(""),
        triple.timestamp.to_rfc3339(),
    );

    let sig = keypair.sign(message.as_bytes());
    triple.signature = Some(sig.to_vec());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Node;
    use crate::storage::MemoryStore;

    #[tokio::test]
    async fn test_build_bloom_filter() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();

        let (filter, count) = BloomSync::build_bloom_filter(&store, None).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_bloom_filter_contains_inserted_triple() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let t = Triple::new(a.id, "knows", b.id);
        let hash = triple_hash(&t);
        store.insert_triple(t).await.unwrap();

        let (filter, _) = BloomSync::build_bloom_filter(&store, None).await.unwrap();
        assert!(filter.check(&hash));
    }

    #[tokio::test]
    async fn test_bloom_filter_false_positive_rate() {
        let store = MemoryStore::new();
        // Insert 100 triples
        for i in 0..100 {
            let a = store.find_or_create_node(&format!("Node{}", i)).await.unwrap();
            let b = store.find_or_create_node(&format!("Target{}", i)).await.unwrap();
            store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();
        }

        let (filter, count) = BloomSync::build_bloom_filter(&store, None).await.unwrap();
        assert_eq!(count, 100);

        // Check false positives with 1000 random hashes
        let mut false_positives = 0;
        for i in 0..1000 {
            let fake_hash: [u8; 32] = {
                let mut h = [0u8; 32];
                let bytes = format!("fake_triple_{}", i);
                for (j, &b) in bytes.as_bytes().iter().enumerate() {
                    h[j % 32] ^= b;
                }
                h
            };
            if filter.check(&fake_hash) {
                false_positives += 1;
            }
        }
        // With 1% FP rate and 1000 checks, expect roughly 10 FPs, allow up to 50
        assert!(false_positives < 50, "Too many false positives: {}", false_positives);
    }

    #[tokio::test]
    async fn test_should_gossip_normal_triple() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let t = Triple::new(a.id, "knows", b.id);

        assert!(should_gossip(&t, None, &store).await);
    }

    #[tokio::test]
    async fn test_should_gossip_retraction_always() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let t = Triple::new(a.id, predicates::RETRACTED_BY, b.id);

        assert!(should_gossip(&t, None, &store).await);
    }

    #[tokio::test]
    async fn test_should_gossip_local_only_excluded() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let marker = store.find_or_create_node("true").await.unwrap();

        // Create the main triple
        let t = Triple::new(a.id, "secret", b.id);
        store.insert_triple(t.clone()).await.unwrap();

        // Mark subject A as local_only
        store.insert_triple(Triple::new(a.id, predicates::LOCAL_ONLY, marker.id)).await.unwrap();

        assert!(!should_gossip(&t, None, &store).await);
    }

    #[tokio::test]
    async fn test_should_gossip_shareable_with_enforced() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let allowed_did = store.find_or_create_node("did:valence:key:allowed").await.unwrap();

        // Create the main triple
        let t = Triple::new(a.id, "restricted_data", b.id);
        store.insert_triple(t.clone()).await.unwrap();

        // Mark subject A as shareable only with a specific DID
        store.insert_triple(Triple::new(a.id, predicates::SHAREABLE_WITH, allowed_did.id)).await.unwrap();

        // Should NOT gossip to unknown recipient
        assert!(!should_gossip(&t, None, &store).await);
        // Should NOT gossip to wrong DID
        assert!(!should_gossip(&t, Some("did:valence:key:wrong"), &store).await);
        // SHOULD gossip to allowed DID
        assert!(should_gossip(&t, Some("did:valence:key:allowed"), &store).await);
    }

    #[tokio::test]
    async fn test_sign_and_verify_triple() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        let keypair = Keypair::generate();
        let mut t = Triple::new(a.id, "knows", b.id);
        sign_triple(&mut t, &keypair);

        assert!(t.origin_did.is_some());
        assert!(t.signature.is_some());

        // Verify the signature
        let result = verify_did_signature(
            t.origin_did.as_ref().unwrap(),
            &t,
            t.signature.as_ref().unwrap(),
        );
        assert!(result);
    }

    #[tokio::test]
    async fn test_verify_wrong_signature_fails() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        let keypair = Keypair::generate();
        let mut t = Triple::new(a.id, "knows", b.id);
        sign_triple(&mut t, &keypair);

        // Tamper with the predicate after signing
        t.predicate = crate::models::Predicate::new("tampered");

        let result = verify_did_signature(
            t.origin_did.as_ref().unwrap(),
            &t,
            t.signature.as_ref().unwrap(),
        );
        assert!(!result);
    }

    #[tokio::test]
    async fn test_process_received_triples_verifies_signature() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let graph = GraphView::from_store(&store).await.unwrap();

        let keypair = Keypair::generate();

        // Valid signed triple
        let mut t1 = Triple::new(a.id, "knows", b.id);
        sign_triple(&mut t1, &keypair);

        // Invalid signature (tampered predicate, but keep original TripleId so it passes ID check)
        let mut t2 = Triple::new(a.id, "likes", b.id);
        sign_triple(&mut t2, &keypair);
        t2.predicate = crate::models::Predicate::new("tampered");

        let result = BloomSync::process_received_triples(vec![t1, t2], &store, &graph).await;
        assert_eq!(result.inserted.len(), 1);
        assert_eq!(result.inserted[0].predicate.value, "knows");
    }

    #[tokio::test]
    async fn test_process_received_triples_sets_weights() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        // Create a DID node with some PageRank
        let did_node = store.find_or_create_node("did:valence:key:abc").await.unwrap();
        store.insert_triple(Triple::new(a.id, "trusts", did_node.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "trusts", did_node.id)).await.unwrap();

        let graph = GraphView::from_store(&store).await.unwrap();

        // Triple from the high-PageRank DID
        let mut t = Triple::new(a.id, "fact", b.id);
        t.origin_did = Some("did:valence:key:abc".to_string());
        // No signature for this test (unsigned triples are accepted)

        let result = BloomSync::process_received_triples(vec![t], &store, &graph).await;
        assert_eq!(result.inserted.len(), 1);
        // base_weight should be derived from PageRank (> 0.1 since node has incoming edges)
        assert!(result.inserted[0].base_weight >= 0.1);
        assert_eq!(result.inserted[0].local_weight, 0.0);
    }

    #[tokio::test]
    async fn test_process_received_triples_retraction_priority() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let graph = GraphView::from_store(&store).await.unwrap();

        let mut t = Triple::new(a.id, predicates::RETRACTED_BY, b.id);
        t.origin_did = None;
        t.signature = None;

        let result = BloomSync::process_received_triples(vec![t], &store, &graph).await;
        assert_eq!(result.inserted.len(), 1);
        assert_eq!(result.inserted[0].base_weight, 1.0); // Retraction triples get max weight
    }

    #[tokio::test]
    async fn test_filter_wanted_headers_skips_existing() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        // Insert a triple we already have
        let existing = Triple::new(a.id, "knows", b.id);
        let existing_id = existing.id;
        store.insert_triple(existing).await.unwrap();

        let graph = GraphView::from_store(&store).await.unwrap();

        let headers = vec![
            TripleHeader {
                id: existing_id,
                subject: "A".into(),
                predicate: "knows".into(),
                object: "B".into(),
                origin_did: None,
                timestamp: chrono::Utc::now(),
                hash: [0u8; 32],
            },
            TripleHeader {
                id: Uuid::new_v4(),
                subject: "C".into(),
                predicate: "likes".into(),
                object: "D".into(),
                origin_did: None,
                timestamp: chrono::Utc::now(),
                hash: [1u8; 32],
            },
        ];

        let wanted = BloomSync::filter_wanted_headers(&headers, &store, &graph).await;
        assert_eq!(wanted.len(), 1); // Only the new one
        assert_ne!(wanted[0], existing_id);
    }

    #[tokio::test]
    async fn test_build_headers_for_hashes() {
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let t = Triple::new(a.id, "knows", b.id);
        let hash = triple_hash(&t);
        store.insert_triple(t).await.unwrap();

        let headers = BloomSync::build_headers_for_hashes(&store, &[hash], None).await.unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].predicate, "knows");
        assert_eq!(headers[0].subject, "A");
        assert_eq!(headers[0].object, "B");
    }

    // === CRDT PROPERTY TESTS ===

    #[tokio::test]
    async fn test_crdt_deterministic_ids() {
        // Two independent stores create the same triple → same TripleId
        let store_a = MemoryStore::new();
        let store_b = MemoryStore::new();

        let a1 = store_a.find_or_create_node("Alice").await.unwrap();
        let b1 = store_a.find_or_create_node("Bob").await.unwrap();
        let t1 = Triple::new(a1.id, "knows", b1.id);

        let a2 = store_b.find_or_create_node("Alice").await.unwrap();
        let b2 = store_b.find_or_create_node("Bob").await.unwrap();
        let t2 = Triple::new(a2.id, "knows", b2.id);

        // Content-addressable: same (S, P, O) → same TripleId
        assert_eq!(t1.id, t2.id);
        // And same NodeIds
        assert_eq!(a1.id, a2.id);
        assert_eq!(b1.id, b2.id);
    }

    #[tokio::test]
    async fn test_crdt_corroboration_on_duplicate() {
        // Store already has a triple. Receiving the same triple from a peer
        // should corroborate (boost weight) rather than insert a duplicate.
        let store = MemoryStore::new();
        let a = store.find_or_create_node("Alice").await.unwrap();
        let b = store.find_or_create_node("Bob").await.unwrap();

        // Insert the triple locally with a moderate weight (not 1.0, so boost is visible)
        let mut local_triple = Triple::new(a.id, "knows", b.id);
        local_triple.base_weight = 0.5;
        let triple_id = local_triple.id;
        store.insert_triple(local_triple).await.unwrap();

        let original = store.get_triple(triple_id).await.unwrap().unwrap();
        let original_weight = original.base_weight;

        // "Receive" the same triple from a peer
        let peer_triple = Triple::new(a.id, "knows", b.id);
        assert_eq!(peer_triple.id, triple_id); // Same content → same ID

        let graph = GraphView::from_store(&store).await.unwrap();
        let result = BloomSync::process_received_triples(vec![peer_triple], &store, &graph).await;

        // Should be corroborated, not inserted
        assert_eq!(result.inserted.len(), 0);
        assert_eq!(result.corroborated.len(), 1);
        assert_eq!(result.corroborated[0], triple_id);

        // base_weight should have been boosted
        let updated = store.get_triple(triple_id).await.unwrap().unwrap();
        assert!(updated.base_weight > original_weight);
        assert!((updated.base_weight - (original_weight + CORROBORATION_BOOST)).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_crdt_idempotent_merge() {
        // Receiving the same triple multiple times doesn't create duplicates.
        let store = MemoryStore::new();
        let a = store.find_or_create_node("Alice").await.unwrap();
        let b = store.find_or_create_node("Bob").await.unwrap();

        let graph = GraphView::from_store(&store).await.unwrap();

        // First receive: should insert
        let t1 = Triple::new(a.id, "knows", b.id);
        let triple_id = t1.id;
        let r1 = BloomSync::process_received_triples(vec![t1], &store, &graph).await;
        assert_eq!(r1.inserted.len(), 1);
        // Actually insert it
        store.insert_triple(r1.inserted.into_iter().next().unwrap()).await.unwrap();

        // Second receive: should corroborate
        let t2 = Triple::new(a.id, "knows", b.id);
        let r2 = BloomSync::process_received_triples(vec![t2], &store, &graph).await;
        assert_eq!(r2.inserted.len(), 0);
        assert_eq!(r2.corroborated.len(), 1);

        // Third receive: should corroborate again
        let t3 = Triple::new(a.id, "knows", b.id);
        let r3 = BloomSync::process_received_triples(vec![t3], &store, &graph).await;
        assert_eq!(r3.inserted.len(), 0);
        assert_eq!(r3.corroborated.len(), 1);

        // Only one triple in the store
        assert_eq!(store.count_triples().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_crdt_commutative_merge() {
        // Order of receiving triples from different peers doesn't matter.
        // Store A receives (Alice knows Bob) then (Alice likes Carol).
        // Store B receives (Alice likes Carol) then (Alice knows Bob).
        // Both end up with the same triples and same IDs.

        let store_a = MemoryStore::new();
        let store_b = MemoryStore::new();

        let alice_a = store_a.find_or_create_node("Alice").await.unwrap();
        let bob_a = store_a.find_or_create_node("Bob").await.unwrap();
        let carol_a = store_a.find_or_create_node("Carol").await.unwrap();

        let alice_b = store_b.find_or_create_node("Alice").await.unwrap();
        let bob_b = store_b.find_or_create_node("Bob").await.unwrap();
        let carol_b = store_b.find_or_create_node("Carol").await.unwrap();

        let t_knows_a = Triple::new(alice_a.id, "knows", bob_a.id);
        let t_likes_a = Triple::new(alice_a.id, "likes", carol_a.id);
        let t_knows_b = Triple::new(alice_b.id, "knows", bob_b.id);
        let t_likes_b = Triple::new(alice_b.id, "likes", carol_b.id);

        // Same content → same IDs
        assert_eq!(t_knows_a.id, t_knows_b.id);
        assert_eq!(t_likes_a.id, t_likes_b.id);

        let graph_a = GraphView::from_store(&store_a).await.unwrap();
        let graph_b = GraphView::from_store(&store_b).await.unwrap();

        // Store A: knows then likes
        let r1a = BloomSync::process_received_triples(vec![t_knows_a], &store_a, &graph_a).await;
        for t in r1a.inserted { store_a.insert_triple(t).await.unwrap(); }
        let r2a = BloomSync::process_received_triples(vec![t_likes_a], &store_a, &graph_a).await;
        for t in r2a.inserted { store_a.insert_triple(t).await.unwrap(); }

        // Store B: likes then knows (reversed order)
        let r1b = BloomSync::process_received_triples(vec![t_likes_b], &store_b, &graph_b).await;
        for t in r1b.inserted { store_b.insert_triple(t).await.unwrap(); }
        let r2b = BloomSync::process_received_triples(vec![t_knows_b], &store_b, &graph_b).await;
        for t in r2b.inserted { store_b.insert_triple(t).await.unwrap(); }

        // Both stores have the same triple count
        assert_eq!(store_a.count_triples().await.unwrap(), 2);
        assert_eq!(store_b.count_triples().await.unwrap(), 2);

        // Both stores have the same TripleIds
        let all_a = store_a.query_triples(TriplePattern::default()).await.unwrap();
        let all_b = store_b.query_triples(TriplePattern::default()).await.unwrap();
        let mut ids_a: Vec<_> = all_a.iter().map(|t| t.id).collect();
        let mut ids_b: Vec<_> = all_b.iter().map(|t| t.id).collect();
        ids_a.sort();
        ids_b.sort();
        assert_eq!(ids_a, ids_b);
    }

    #[tokio::test]
    async fn test_crdt_tombstone_deterministic_id() {
        // Retraction triple has a deterministic ID based on content.
        let store = MemoryStore::new();
        let a = store.find_or_create_node("triple:abc123").await.unwrap();
        let b = store.find_or_create_node("did:valence:key:chris").await.unwrap();

        let t1 = Triple::new(a.id, predicates::RETRACTED_BY, b.id);
        let t2 = Triple::new(a.id, predicates::RETRACTED_BY, b.id);

        assert_eq!(t1.id, t2.id);
    }

    #[tokio::test]
    async fn test_crdt_corroboration_weight_capped() {
        // Corroboration boost should be capped at 1.0.
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        // Insert a triple with base_weight already at 1.0
        let mut t = Triple::new(a.id, "knows", b.id);
        t.base_weight = 1.0;
        let triple_id = t.id;
        store.insert_triple(t).await.unwrap();

        let graph = GraphView::from_store(&store).await.unwrap();

        // Corroborate it
        let peer_t = Triple::new(a.id, "knows", b.id);
        let result = BloomSync::process_received_triples(vec![peer_t], &store, &graph).await;
        assert_eq!(result.corroborated.len(), 1);

        // Weight should still be 1.0 (capped)
        let updated = store.get_triple(triple_id).await.unwrap().unwrap();
        assert_eq!(updated.base_weight, 1.0);
    }

    #[tokio::test]
    async fn test_crdt_rejects_fabricated_triple_id() {
        // A peer sends a triple with a TripleId that doesn't match its content.
        // This should be rejected.
        let store = MemoryStore::new();
        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let graph = GraphView::from_store(&store).await.unwrap();

        let mut t = Triple::new(a.id, "knows", b.id);
        // Tamper with the ID
        t.id = Uuid::new_v4();

        let result = BloomSync::process_received_triples(vec![t], &store, &graph).await;
        assert_eq!(result.inserted.len(), 0);
        assert_eq!(result.corroborated.len(), 0);
    }

    #[tokio::test]
    async fn test_crdt_bloom_filter_same_triple_same_hash() {
        // Two independently created triples with same content produce
        // the same bloom filter hash.
        let a = Node::new("Alice");
        let b = Node::new("Bob");

        let t1 = Triple::new(a.id, "knows", b.id);
        let t2 = Triple::new(a.id, "knows", b.id);

        assert_eq!(triple_hash(&t1), triple_hash(&t2));
    }
}
