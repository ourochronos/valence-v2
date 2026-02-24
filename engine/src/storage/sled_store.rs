use std::collections::HashSet;
use std::path::Path;
use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::models::{Triple, TripleId, Node, NodeId, Source, SourceId};
use super::traits::{TripleStore, TriplePattern};

/// Embedded persistent storage backend using sled.
///
/// Uses multiple sled trees for SPO/POS/OSP index orderings, enabling efficient
/// queries by subject, predicate, or object. Data is persisted to disk and
/// survives restarts.
///
/// # Trees
///
/// - `triples` — primary: triple_uuid bytes -> bincode(Triple)
/// - `nodes` — node_uuid bytes -> bincode(Node)
/// - `node_values` — node value string bytes -> node_uuid bytes
/// - `spo` — [subject(16) | predicate_hash(16) | object(16)] -> triple_uuid
/// - `pos` — [predicate_hash(16) | object(16) | subject(16)] -> triple_uuid
/// - `osp` — [object(16) | subject(16) | predicate_hash(16)] -> triple_uuid
/// - `sources` — source_uuid bytes -> bincode(Source)
/// - `triple_sources` — [triple_uuid(16) | source_uuid(16)] -> ()
#[derive(Clone)]
pub struct SledStore {
    db: sled::Db,
    triples: sled::Tree,
    nodes: sled::Tree,
    node_values: sled::Tree,
    spo: sled::Tree,
    pos: sled::Tree,
    osp: sled::Tree,
    sources: sled::Tree,
    triple_sources: sled::Tree,
}

/// Convert a UUID to its 16-byte representation.
fn uuid_bytes(id: &uuid::Uuid) -> [u8; 16] {
    *id.as_bytes()
}

/// Reconstruct a UUID from a 16-byte slice.
fn uuid_from_bytes(bytes: &[u8]) -> uuid::Uuid {
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes[..16]);
    uuid::Uuid::from_bytes(arr)
}

/// Hash a predicate string to 16 bytes for use as an index key component.
/// We use a simple approach: take the first 16 bytes of the string padded/truncated,
/// combined with a length byte. For proper prefix scanning we need deterministic mapping.
/// Using a hash ensures fixed-size keys.
fn predicate_key(predicate: &str) -> [u8; 16] {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    predicate.hash(&mut hasher);
    let h1 = hasher.finish();
    // Hash again with a different seed for more bits
    predicate.len().hash(&mut hasher);
    let h2 = hasher.finish();
    let mut key = [0u8; 16];
    key[..8].copy_from_slice(&h1.to_be_bytes());
    key[8..16].copy_from_slice(&h2.to_be_bytes());
    key
}

/// Build a 48-byte composite key from three 16-byte components.
fn composite_key(a: &[u8; 16], b: &[u8; 16], c: &[u8; 16]) -> [u8; 48] {
    let mut key = [0u8; 48];
    key[..16].copy_from_slice(a);
    key[16..32].copy_from_slice(b);
    key[32..48].copy_from_slice(c);
    key
}

/// Build a 16-byte prefix for scanning by the first component.
fn prefix_16(a: &[u8; 16]) -> [u8; 16] {
    *a
}

/// Build a 32-byte prefix for scanning by the first two components.
fn prefix_32(a: &[u8; 16], b: &[u8; 16]) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[..16].copy_from_slice(a);
    key[16..32].copy_from_slice(b);
    key
}

impl SledStore {
    /// Open or create a sled database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let db = sled::open(path)
            .with_context(|| format!("Failed to open sled database at {:?}", path))?;

        let triples = db.open_tree("triples")
            .context("Failed to open triples tree")?;
        let nodes = db.open_tree("nodes")
            .context("Failed to open nodes tree")?;
        let node_values = db.open_tree("node_values")
            .context("Failed to open node_values tree")?;
        let spo = db.open_tree("spo")
            .context("Failed to open spo tree")?;
        let pos = db.open_tree("pos")
            .context("Failed to open pos tree")?;
        let osp = db.open_tree("osp")
            .context("Failed to open osp tree")?;
        let sources = db.open_tree("sources")
            .context("Failed to open sources tree")?;
        let triple_sources = db.open_tree("triple_sources")
            .context("Failed to open triple_sources tree")?;

        Ok(Self {
            db,
            triples,
            nodes,
            node_values,
            spo,
            pos,
            osp,
            sources,
            triple_sources,
        })
    }

    /// Insert index entries for a triple into SPO, POS, and OSP trees.
    fn insert_indices(&self, triple: &Triple) -> Result<()> {
        let tid = uuid_bytes(&triple.id);
        let sid = uuid_bytes(&triple.subject);
        let pid = predicate_key(&triple.predicate.value);
        let oid = uuid_bytes(&triple.object);

        self.spo.insert(composite_key(&sid, &pid, &oid), &tid)
            .context("Failed to insert SPO index")?;
        self.pos.insert(composite_key(&pid, &oid, &sid), &tid)
            .context("Failed to insert POS index")?;
        self.osp.insert(composite_key(&oid, &sid, &pid), &tid)
            .context("Failed to insert OSP index")?;

        Ok(())
    }

    /// Remove index entries for a triple from SPO, POS, and OSP trees.
    fn remove_indices(&self, triple: &Triple) -> Result<()> {
        let sid = uuid_bytes(&triple.subject);
        let pid = predicate_key(&triple.predicate.value);
        let oid = uuid_bytes(&triple.object);

        self.spo.remove(composite_key(&sid, &pid, &oid))
            .context("Failed to remove SPO index")?;
        self.pos.remove(composite_key(&pid, &oid, &sid))
            .context("Failed to remove POS index")?;
        self.osp.remove(composite_key(&oid, &sid, &pid))
            .context("Failed to remove OSP index")?;

        Ok(())
    }

    /// Look up triple IDs from an index tree by scanning a prefix.
    fn scan_index_prefix(&self, tree: &sled::Tree, prefix: &[u8]) -> Result<Vec<TripleId>> {
        let mut ids = Vec::new();
        for result in tree.scan_prefix(prefix) {
            let (_key, value) = result.context("Failed to scan index")?;
            ids.push(uuid_from_bytes(&value));
        }
        Ok(ids)
    }

    /// Retrieve multiple triples by their IDs.
    fn get_triples_by_ids(&self, ids: &[TripleId]) -> Result<Vec<Triple>> {
        let mut triples = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(bytes) = self.triples.get(uuid_bytes(id)).context("Failed to get triple")? {
                let triple: Triple = bincode::deserialize(&bytes)
                    .context("Failed to deserialize triple")?;
                triples.push(triple);
            }
        }
        Ok(triples)
    }
}

#[async_trait]
impl TripleStore for SledStore {
    async fn insert_node(&self, node: Node) -> Result<NodeId> {
        let id = node.id;
        let value = node.value.clone();
        let bytes = bincode::serialize(&node)
            .context("Failed to serialize node")?;

        self.nodes.insert(uuid_bytes(&id), bytes)
            .context("Failed to insert node")?;
        self.node_values.insert(value.as_bytes(), &uuid_bytes(&id))
            .context("Failed to insert node value index")?;

        Ok(id)
    }

    async fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        match self.nodes.get(uuid_bytes(&id)).context("Failed to get node")? {
            Some(bytes) => {
                let node: Node = bincode::deserialize(&bytes)
                    .context("Failed to deserialize node")?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    async fn find_node_by_value(&self, value: &str) -> Result<Option<Node>> {
        match self.node_values.get(value.as_bytes()).context("Failed to look up node by value")? {
            Some(id_bytes) => {
                let id = uuid_from_bytes(&id_bytes);
                self.get_node(id).await
            }
            None => Ok(None),
        }
    }

    async fn find_or_create_node(&self, value: &str) -> Result<Node> {
        if let Some(node) = self.find_node_by_value(value).await? {
            return Ok(node);
        }
        let node = Node::new(value);
        self.insert_node(node.clone()).await?;
        Ok(node)
    }

    async fn insert_triple(&self, triple: Triple) -> Result<TripleId> {
        let id = triple.id;
        let bytes = bincode::serialize(&triple)
            .context("Failed to serialize triple")?;

        self.triples.insert(uuid_bytes(&id), bytes)
            .context("Failed to insert triple")?;
        self.insert_indices(&triple)?;

        Ok(id)
    }

    async fn get_triple(&self, id: TripleId) -> Result<Option<Triple>> {
        match self.triples.get(uuid_bytes(&id)).context("Failed to get triple")? {
            Some(bytes) => {
                let triple: Triple = bincode::deserialize(&bytes)
                    .context("Failed to deserialize triple")?;
                Ok(Some(triple))
            }
            None => Ok(None),
        }
    }

    async fn update_triple(&self, triple: Triple) -> Result<()> {
        // Remove old indices if the triple exists
        if let Some(old_bytes) = self.triples.get(uuid_bytes(&triple.id)).context("Failed to get old triple")? {
            let old_triple: Triple = bincode::deserialize(&old_bytes)
                .context("Failed to deserialize old triple")?;
            self.remove_indices(&old_triple)?;
        }

        let bytes = bincode::serialize(&triple)
            .context("Failed to serialize triple")?;
        self.triples.insert(uuid_bytes(&triple.id), bytes)
            .context("Failed to update triple")?;
        self.insert_indices(&triple)?;

        Ok(())
    }

    async fn query_triples(&self, pattern: TriplePattern) -> Result<Vec<Triple>> {
        match (pattern.subject, pattern.predicate.as_deref(), pattern.object) {
            // All three specified — exact lookup via SPO
            (Some(s), Some(p), Some(o)) => {
                let sid = uuid_bytes(&s);
                let pid = predicate_key(p);
                let oid = uuid_bytes(&o);
                let key = composite_key(&sid, &pid, &oid);
                let ids = self.scan_index_prefix(&self.spo, &key)?;
                self.get_triples_by_ids(&ids)
            }
            // Subject + predicate — scan SPO with 32-byte prefix
            (Some(s), Some(p), None) => {
                let sid = uuid_bytes(&s);
                let pid = predicate_key(p);
                let prefix = prefix_32(&sid, &pid);
                let ids = self.scan_index_prefix(&self.spo, &prefix)?;
                self.get_triples_by_ids(&ids)
            }
            // Subject only — scan SPO with 16-byte prefix
            (Some(s), None, None) => {
                let sid = uuid_bytes(&s);
                let prefix = prefix_16(&sid);
                let ids = self.scan_index_prefix(&self.spo, &prefix)?;
                self.get_triples_by_ids(&ids)
            }
            // Predicate + object — scan POS with 32-byte prefix
            (None, Some(p), Some(o)) => {
                let pid = predicate_key(p);
                let oid = uuid_bytes(&o);
                let prefix = prefix_32(&pid, &oid);
                let ids = self.scan_index_prefix(&self.pos, &prefix)?;
                self.get_triples_by_ids(&ids)
            }
            // Predicate only — scan POS with 16-byte prefix
            (None, Some(p), None) => {
                let pid = predicate_key(p);
                let prefix = prefix_16(&pid);
                let ids = self.scan_index_prefix(&self.pos, &prefix)?;
                self.get_triples_by_ids(&ids)
            }
            // Object only — scan OSP with 16-byte prefix
            (None, None, Some(o)) => {
                let oid = uuid_bytes(&o);
                let prefix = prefix_16(&oid);
                let ids = self.scan_index_prefix(&self.osp, &prefix)?;
                self.get_triples_by_ids(&ids)
            }
            // Subject + object (no predicate) — scan SPO by subject, filter by object
            (Some(s), None, Some(o)) => {
                let sid = uuid_bytes(&s);
                let prefix = prefix_16(&sid);
                let ids = self.scan_index_prefix(&self.spo, &prefix)?;
                let triples = self.get_triples_by_ids(&ids)?;
                Ok(triples.into_iter().filter(|t| t.object == o).collect())
            }
            // Wildcard — scan all triples
            (None, None, None) => {
                let mut triples = Vec::new();
                for result in self.triples.iter() {
                    let (_key, value) = result.context("Failed to iterate triples")?;
                    let triple: Triple = bincode::deserialize(&value)
                        .context("Failed to deserialize triple")?;
                    triples.push(triple);
                }
                Ok(triples)
            }
        }
    }

    async fn touch_triple(&self, id: TripleId) -> Result<()> {
        let key = uuid_bytes(&id);
        if let Some(bytes) = self.triples.get(key).context("Failed to get triple for touch")? {
            let mut triple: Triple = bincode::deserialize(&bytes)
                .context("Failed to deserialize triple for touch")?;
            triple.touch();
            let new_bytes = bincode::serialize(&triple)
                .context("Failed to serialize touched triple")?;
            self.triples.insert(key, new_bytes)
                .context("Failed to update touched triple")?;
        }
        Ok(())
    }

    async fn delete_triple(&self, id: TripleId) -> Result<()> {
        let key = uuid_bytes(&id);
        if let Some(bytes) = self.triples.get(key).context("Failed to get triple for deletion")? {
            let triple: Triple = bincode::deserialize(&bytes)
                .context("Failed to deserialize triple for deletion")?;
            self.remove_indices(&triple)?;
            self.triples.remove(key)
                .context("Failed to remove triple")?;
            // Clean up source mappings
            let prefix = prefix_16(&uuid_bytes(&id));
            let source_keys: Vec<_> = self.triple_sources.scan_prefix(prefix)
                .filter_map(|r| r.ok().map(|(k, _)| k))
                .collect();
            for k in source_keys {
                self.triple_sources.remove(k)
                    .context("Failed to remove triple-source mapping")?;
            }
        }
        Ok(())
    }

    async fn insert_source(&self, source: Source) -> Result<SourceId> {
        let id = source.id;
        let triple_ids = source.triple_ids.clone();
        let bytes = bincode::serialize(&source)
            .context("Failed to serialize source")?;

        self.sources.insert(uuid_bytes(&id), bytes)
            .context("Failed to insert source")?;

        // Update triple -> source junction
        for triple_id in &triple_ids {
            let tid = uuid_bytes(triple_id);
            let sid = uuid_bytes(&id);
            let mut junction_key = [0u8; 32];
            junction_key[..16].copy_from_slice(&tid);
            junction_key[16..32].copy_from_slice(&sid);
            self.triple_sources.insert(junction_key, &[])
                .context("Failed to insert triple-source junction")?;
        }

        Ok(id)
    }

    async fn get_sources_for_triple(&self, triple_id: TripleId) -> Result<Vec<Source>> {
        let prefix = uuid_bytes(&triple_id);
        let mut sources = Vec::new();

        for result in self.triple_sources.scan_prefix(prefix) {
            let (key, _) = result.context("Failed to scan triple-source junction")?;
            if key.len() >= 32 {
                let source_id = uuid_from_bytes(&key[16..32]);
                if let Some(bytes) = self.sources.get(uuid_bytes(&source_id))
                    .context("Failed to get source")? {
                    let source: Source = bincode::deserialize(&bytes)
                        .context("Failed to deserialize source")?;
                    sources.push(source);
                }
            }
        }

        Ok(sources)
    }

    async fn neighbors(&self, node_id: NodeId, depth: u32) -> Result<Vec<Triple>> {
        if depth == 0 {
            return Ok(Vec::new());
        }
        if depth > 10 {
            anyhow::bail!("Depth cannot exceed 10 (too expensive)");
        }

        let nid = uuid_bytes(&node_id);

        // Depth 1: find all triples where node is subject (SPO) or object (OSP)
        let mut outgoing_ids = self.scan_index_prefix(&self.spo, &prefix_16(&nid))?;
        let incoming_ids = self.scan_index_prefix(&self.osp, &prefix_16(&nid))?;
        outgoing_ids.extend(incoming_ids);

        // Deduplicate
        let mut seen_triples: HashSet<TripleId> = HashSet::new();
        let mut result = Vec::new();
        for id in &outgoing_ids {
            if seen_triples.insert(*id) {
                if let Some(bytes) = self.triples.get(uuid_bytes(id)).context("Failed to get triple")? {
                    let triple: Triple = bincode::deserialize(&bytes)
                        .context("Failed to deserialize triple")?;
                    result.push(triple);
                }
            }
        }

        if depth > 1 {
            let mut seen_nodes: HashSet<NodeId> = HashSet::new();
            seen_nodes.insert(node_id);

            let mut current_level = result.clone();
            for _ in 1..depth {
                let mut next_level = Vec::new();
                for triple in &current_level {
                    for &conn_node in &[triple.subject, triple.object] {
                        if seen_nodes.insert(conn_node) {
                            let cid = uuid_bytes(&conn_node);
                            let mut ids = self.scan_index_prefix(&self.spo, &prefix_16(&cid))?;
                            ids.extend(self.scan_index_prefix(&self.osp, &prefix_16(&cid))?);

                            for id in ids {
                                if seen_triples.insert(id) {
                                    if let Some(bytes) = self.triples.get(uuid_bytes(&id))
                                        .context("Failed to get triple")? {
                                        let t: Triple = bincode::deserialize(&bytes)
                                            .context("Failed to deserialize triple")?;
                                        next_level.push(t.clone());
                                        result.push(t);
                                    }
                                }
                            }
                        }
                    }
                }
                current_level = next_level;
            }
        }

        Ok(result)
    }

    async fn count_triples(&self) -> Result<u64> {
        Ok(self.triples.len() as u64)
    }

    async fn count_nodes(&self) -> Result<u64> {
        Ok(self.nodes.len() as u64)
    }

    async fn decay(&self, factor: f64, min_weight: f64) -> Result<u64> {
        if !(0.0..=1.0).contains(&factor) {
            anyhow::bail!("Decay factor must be between 0.0 and 1.0");
        }
        if min_weight < 0.0 {
            anyhow::bail!("Min weight cannot be negative");
        }

        let mut decayed_count = 0u64;
        let mut batch = sled::Batch::default();

        for result in self.triples.iter() {
            let (key, value) = result.context("Failed to iterate triples for decay")?;
            let mut triple: Triple = bincode::deserialize(&value)
                .context("Failed to deserialize triple for decay")?;
            triple.local_weight *= factor;
            let bytes = bincode::serialize(&triple)
                .context("Failed to serialize decayed triple")?;
            batch.insert(key, bytes);
            decayed_count += 1;
        }

        self.triples.apply_batch(batch)
            .context("Failed to apply decay batch")?;

        Ok(decayed_count)
    }

    async fn evict_below_weight(&self, threshold: f64) -> Result<u64> {
        if threshold < 0.0 {
            anyhow::bail!("Eviction threshold cannot be negative");
        }

        let mut to_evict = Vec::new();

        for result in self.triples.iter() {
            let (key, value) = result.context("Failed to iterate triples for eviction")?;
            let triple: Triple = bincode::deserialize(&value)
                .context("Failed to deserialize triple for eviction")?;
            if triple.local_weight < threshold {
                to_evict.push((key, triple));
            }
        }

        let evicted = to_evict.len() as u64;

        for (key, triple) in to_evict {
            self.remove_indices(&triple)?;
            self.triples.remove(key)
                .context("Failed to remove evicted triple")?;
            // Clean up source mappings
            let prefix = prefix_16(&uuid_bytes(&triple.id));
            let source_keys: Vec<_> = self.triple_sources.scan_prefix(prefix)
                .filter_map(|r| r.ok().map(|(k, _)| k))
                .collect();
            for k in source_keys {
                self.triple_sources.remove(k)
                    .context("Failed to remove evicted triple-source mapping")?;
            }
        }

        Ok(evicted)
    }

    async fn flush(&self) -> Result<()> {
        self.db.flush_async().await
            .context("Failed to flush sled database")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Node, Triple, Source, SourceType};
    use tempfile::TempDir;

    fn create_store() -> (SledStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = SledStore::open(dir.path()).unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn test_insert_and_retrieve_node() {
        let (store, _dir) = create_store();
        let node = Node::new("test_value");
        let id = store.insert_node(node.clone()).await.unwrap();

        let retrieved = store.get_node(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, "test_value");
    }

    #[tokio::test]
    async fn test_find_node_by_value() {
        let (store, _dir) = create_store();
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
        let (store, _dir) = create_store();

        let node1 = store.find_or_create_node("test").await.unwrap();
        let node2 = store.find_or_create_node("test").await.unwrap();

        assert_eq!(node1.id, node2.id);
        assert_eq!(store.count_nodes().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_insert_and_query_triples() {
        let (store, _dir) = create_store();

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
    async fn test_spo_index() {
        let (store, _dir) = create_store();

        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let carol = store.find_or_create_node("Carol").await.unwrap();

        store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        store.insert_triple(Triple::new(alice.id, "likes", carol.id)).await.unwrap();
        store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();

        // SPO: query by subject Alice
        let pattern = TriplePattern {
            subject: Some(alice.id),
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.subject == alice.id));

        // SPO: query by subject + predicate
        let pattern = TriplePattern {
            subject: Some(alice.id),
            predicate: Some("knows".to_string()),
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].object, bob.id);
    }

    #[tokio::test]
    async fn test_pos_index() {
        let (store, _dir) = create_store();

        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let carol = store.find_or_create_node("Carol").await.unwrap();

        store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        store.insert_triple(Triple::new(carol.id, "knows", bob.id)).await.unwrap();
        store.insert_triple(Triple::new(alice.id, "likes", bob.id)).await.unwrap();

        // POS: query by predicate "knows"
        let pattern = TriplePattern {
            subject: None,
            predicate: Some("knows".to_string()),
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.predicate.value == "knows"));

        // POS: query by predicate + object
        let pattern = TriplePattern {
            subject: None,
            predicate: Some("knows".to_string()),
            object: Some(bob.id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_osp_index() {
        let (store, _dir) = create_store();

        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let carol = store.find_or_create_node("Carol").await.unwrap();

        store.insert_triple(Triple::new(alice.id, "knows", carol.id)).await.unwrap();
        store.insert_triple(Triple::new(bob.id, "likes", carol.id)).await.unwrap();
        store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();

        // OSP: query by object Carol
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: Some(carol.id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.object == carol.id));
    }

    #[tokio::test]
    async fn test_persistence() {
        let dir = TempDir::new().unwrap();

        let triple_id;
        let node_id;

        // Phase 1: insert data
        {
            let store = SledStore::open(dir.path()).unwrap();

            let node = Node::new("persistent_value");
            node_id = store.insert_node(node).await.unwrap();

            let subj = store.find_or_create_node("A").await.unwrap();
            let obj = store.find_or_create_node("B").await.unwrap();
            let triple = Triple::new(subj.id, "rel", obj.id);
            triple_id = store.insert_triple(triple).await.unwrap();

            // Explicitly flush
            store.db.flush().unwrap();
        }

        // Phase 2: reopen and verify
        {
            let store = SledStore::open(dir.path()).unwrap();

            let node = store.get_node(node_id).await.unwrap();
            assert!(node.is_some());
            assert_eq!(node.unwrap().value, "persistent_value");

            let triple = store.get_triple(triple_id).await.unwrap();
            assert!(triple.is_some());
            assert_eq!(triple.unwrap().predicate.value, "rel");

            assert_eq!(store.count_triples().await.unwrap(), 1);
            assert_eq!(store.count_nodes().await.unwrap(), 3); // persistent_value + A + B
        }
    }

    #[tokio::test]
    async fn test_decay_and_eviction() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        let triple = Triple::new(a.id, "rel", b.id);
        store.insert_triple(triple.clone()).await.unwrap();

        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.local_weight, 1.0);

        store.decay(0.5, 0.0).await.unwrap();
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.local_weight, 0.5);

        store.decay(0.5, 0.0).await.unwrap();
        let t = store.get_triple(triple.id).await.unwrap().unwrap();
        assert_eq!(t.local_weight, 0.25);

        let evicted = store.evict_below_weight(0.3).await.unwrap();
        assert_eq!(evicted, 1);
        assert_eq!(store.count_triples().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_neighbors() {
        let (store, _dir) = create_store();

        let alice = store.find_or_create_node("Alice").await.unwrap();
        let bob = store.find_or_create_node("Bob").await.unwrap();
        let carol = store.find_or_create_node("Carol").await.unwrap();

        store.insert_triple(Triple::new(alice.id, "knows", bob.id)).await.unwrap();
        store.insert_triple(Triple::new(bob.id, "knows", carol.id)).await.unwrap();

        // Depth 1: Alice knows Bob
        let neighbors = store.neighbors(alice.id, 1).await.unwrap();
        assert_eq!(neighbors.len(), 1);

        // Depth 2: Alice -> Bob -> Carol
        let neighbors = store.neighbors(alice.id, 2).await.unwrap();
        assert_eq!(neighbors.len(), 2);
    }

    #[tokio::test]
    async fn test_neighbors_depth_zero() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        store.insert_triple(Triple::new(a.id, "knows", b.id)).await.unwrap();

        let neighbors = store.neighbors(a.id, 0).await.unwrap();
        assert_eq!(neighbors.len(), 0);
    }

    #[tokio::test]
    async fn test_touch_triple() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        let triple = Triple::new(a.id, "rel", b.id);
        let triple_id = triple.id;
        store.insert_triple(triple).await.unwrap();

        let before = store.get_triple(triple_id).await.unwrap().unwrap();
        let access_count_before = before.access_count;

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store.touch_triple(triple_id).await.unwrap();

        let after = store.get_triple(triple_id).await.unwrap().unwrap();
        assert_eq!(after.access_count, access_count_before + 1);
        assert!(after.last_accessed > before.last_accessed);
        assert_eq!(after.local_weight, 1.0);
    }

    #[tokio::test]
    async fn test_delete_triple() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        let triple = Triple::new(a.id, "rel", b.id);
        let triple_id = triple.id;
        store.insert_triple(triple).await.unwrap();

        assert_eq!(store.count_triples().await.unwrap(), 1);

        store.delete_triple(triple_id).await.unwrap();
        assert_eq!(store.count_triples().await.unwrap(), 0);

        // Indices should be cleaned up too
        let pattern = TriplePattern {
            subject: Some(a.id),
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_triple() {
        let (store, _dir) = create_store();

        let fake_id = uuid::Uuid::new_v4();
        let result = store.delete_triple(fake_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_source_tracking() {
        let (store, _dir) = create_store();

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

    #[tokio::test]
    async fn test_query_all_wildcard() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();

        store.insert_triple(Triple::new(a.id, "rel1", b.id)).await.unwrap();
        store.insert_triple(Triple::new(b.id, "rel2", c.id)).await.unwrap();
        store.insert_triple(Triple::new(c.id, "rel3", a.id)).await.unwrap();

        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_large_batch_insert() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();

        let mut triple_ids = Vec::new();
        for i in 0..1000 {
            let node = store.find_or_create_node(&format!("Node_{}", i)).await.unwrap();
            let triple = Triple::new(a.id, "connects_to", node.id);
            let id = store.insert_triple(triple).await.unwrap();
            triple_ids.push(id);
        }

        assert_eq!(store.count_triples().await.unwrap(), 1000);

        // Query by subject should use SPO index
        let pattern = TriplePattern {
            subject: Some(a.id),
            predicate: None,
            object: None,
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1000);

        // Verify individual retrieval
        for id in triple_ids.iter().take(10) {
            let triple = store.get_triple(*id).await.unwrap();
            assert!(triple.is_some());
        }
    }

    #[tokio::test]
    async fn test_decay_invalid_factor() {
        let (store, _dir) = create_store();

        let result = store.decay(1.5, 0.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_evict_threshold_zero() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();

        store.insert_triple(Triple::new(a.id, "rel", b.id)).await.unwrap();

        store.decay(0.001, 0.0).await.unwrap();

        let evicted = store.evict_below_weight(0.0).await.unwrap();
        assert_eq!(evicted, 0);
        assert_eq!(store.count_triples().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_update_triple() {
        let (store, _dir) = create_store();

        let a = store.find_or_create_node("A").await.unwrap();
        let b = store.find_or_create_node("B").await.unwrap();
        let c = store.find_or_create_node("C").await.unwrap();

        let mut triple = Triple::new(a.id, "knows", b.id);
        let triple_id = store.insert_triple(triple.clone()).await.unwrap();

        // Update: change object from B to C
        triple.object = c.id;
        store.update_triple(triple).await.unwrap();

        // Old index should be gone
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: Some(b.id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 0);

        // New index should work
        let pattern = TriplePattern {
            subject: None,
            predicate: None,
            object: Some(c.id),
        };
        let results = store.query_triples(pattern).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, triple_id);
    }
}
