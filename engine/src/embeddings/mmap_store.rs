//! Memory-mapped embedding store for persistent, O(1) embedding access.
//!
//! Uses a fixed-size record format so that any node's embedding can be located
//! at `node_index * RECORD_SIZE` without scanning. An in-memory `node_id -> node_index`
//! HashMap is rebuilt on load by scanning the file.
//!
//! Record layout (per node):
//! ```text
//! [node_id      (16 bytes)]        // Uuid bytes
//! [spring       (dims * 4 bytes)]  // spring embedding (f32s)
//! [node2vec     (dims * 4 bytes)]  // node2vec embedding (f32s)
//! [spectral     (dims * 4 bytes)]  // spectral embedding (f32s)
//! [spring_ts    (8 bytes)]         // last spring update (unix millis i64)
//! [n2v_ts       (8 bytes)]         // last node2vec update (unix millis i64)
//! [spectral_ts  (8 bytes)]         // last spectral update (unix millis i64)
//! ```

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use memmap2::MmapMut;
use uuid::Uuid;

use crate::models::NodeId;
use super::EmbeddingStore;
use super::spring::EmbeddingStrategy;

/// Default number of embedding dimensions.
const DEFAULT_DIMENSIONS: usize = 64;

/// Default initial capacity (number of node slots pre-allocated).
const DEFAULT_INITIAL_CAPACITY: usize = 1024;

/// Sentinel: all-zero UUID means an empty/unused slot.
const EMPTY_UUID: [u8; 16] = [0u8; 16];

/// Configuration for the memory-mapped embedding store.
#[derive(Debug, Clone)]
pub struct MmapConfig {
    /// Number of embedding dimensions per strategy.
    pub dimensions: usize,
    /// Path to the backing file.
    pub file_path: PathBuf,
    /// Initial number of node slots to allocate.
    pub initial_capacity: usize,
}

impl MmapConfig {
    pub fn new(file_path: impl Into<PathBuf>) -> Self {
        Self {
            dimensions: DEFAULT_DIMENSIONS,
            file_path: file_path.into(),
            initial_capacity: DEFAULT_INITIAL_CAPACITY,
        }
    }

    pub fn with_dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = dimensions;
        self
    }

    pub fn with_initial_capacity(mut self, capacity: usize) -> Self {
        self.initial_capacity = capacity;
        self
    }

    /// Size of a single record in bytes.
    fn record_size(&self) -> usize {
        // 16 (uuid) + 3 * (dims * 4) (embeddings) + 3 * 8 (timestamps)
        16 + 3 * (self.dimensions * 4) + 3 * 8
    }
}

/// Memory-mapped embedding store.
///
/// Provides O(1) lookup for any node's embeddings via `node_index * record_size`.
/// Supports all three embedding strategies (spring, node2vec, spectral) with
/// per-strategy timestamps.
pub struct MmapEmbeddingStore {
    config: MmapConfig,
    mmap: MmapMut,
    file: File,
    /// node_id -> index in the mmap file (0-based slot number)
    index: HashMap<NodeId, usize>,
    /// Number of occupied slots.
    len: usize,
    /// Total number of slots currently allocated in the file.
    capacity: usize,
}

impl MmapEmbeddingStore {
    /// Open or create a memory-mapped embedding store.
    ///
    /// If the file exists, scans it to rebuild the node_id -> index map.
    /// If the file does not exist, creates it with `initial_capacity` slots.
    pub fn open(config: MmapConfig) -> Result<Self> {
        let record_size = config.record_size();
        let exists = config.file_path.exists();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&config.file_path)
            .with_context(|| format!("Failed to open mmap file: {:?}", config.file_path))?;

        let (capacity, len, index) = if exists {
            let file_len = file.metadata()?.len() as usize;
            if file_len == 0 {
                // Empty file — initialize
                let cap = config.initial_capacity;
                file.set_len((cap * record_size) as u64)?;
                (cap, 0, HashMap::new())
            } else {
                // Existing file — scan to rebuild index
                let cap = file_len / record_size;
                let mmap = unsafe { MmapMut::map_mut(&file)? };
                let (len, index) = Self::scan_records(&mmap, record_size, cap);
                drop(mmap);
                (cap, len, index)
            }
        } else {
            let cap = config.initial_capacity;
            file.set_len((cap * record_size) as u64)?;
            (cap, 0, HashMap::new())
        };

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            config,
            mmap,
            file,
            index,
            len,
            capacity,
        })
    }

    /// Scan all records to rebuild the node_id -> index HashMap.
    fn scan_records(
        mmap: &[u8],
        record_size: usize,
        capacity: usize,
    ) -> (usize, HashMap<NodeId, usize>) {
        let mut index = HashMap::new();
        let mut len = 0;

        for slot in 0..capacity {
            let offset = slot * record_size;
            if offset + 16 > mmap.len() {
                break;
            }
            let uuid_bytes: [u8; 16] = mmap[offset..offset + 16]
                .try_into()
                .expect("slice is 16 bytes");
            if uuid_bytes != EMPTY_UUID {
                let node_id = Uuid::from_bytes(uuid_bytes);
                index.insert(node_id, slot);
                len += 1;
            }
        }

        (len, index)
    }

    /// Get the byte offset for a given slot index.
    fn slot_offset(&self, slot: usize) -> usize {
        slot * self.config.record_size()
    }

    /// Ensure there is room for at least one more node. Grows the file if needed.
    fn ensure_capacity(&mut self) -> Result<()> {
        if self.len < self.capacity {
            return Ok(());
        }

        // Double capacity
        let new_capacity = self.capacity * 2;
        let new_size = new_capacity * self.config.record_size();
        self.file.set_len(new_size as u64)?;
        self.mmap = unsafe { MmapMut::map_mut(&self.file)? };
        self.capacity = new_capacity;
        Ok(())
    }

    /// Find the next free slot. Appends at position `self.len` for simple append-only growth.
    /// Scans from len forward to handle any gaps from potential future compaction.
    fn next_free_slot(&self) -> usize {
        let record_size = self.config.record_size();
        for slot in 0..self.capacity {
            let offset = slot * record_size;
            let uuid_bytes: [u8; 16] = self.mmap[offset..offset + 16]
                .try_into()
                .expect("slice is 16 bytes");
            if uuid_bytes == EMPTY_UUID {
                return slot;
            }
        }
        // Should not happen if ensure_capacity was called
        self.capacity
    }

    /// Write a node_id at the given slot.
    fn write_uuid(&mut self, slot: usize, node_id: NodeId) {
        let offset = self.slot_offset(slot);
        self.mmap[offset..offset + 16].copy_from_slice(node_id.as_bytes());
    }

    /// Byte offset of the embedding for a given strategy within a record.
    fn strategy_offset(&self, strategy: EmbeddingStrategy) -> usize {
        let dim_bytes = self.config.dimensions * 4;
        match strategy {
            EmbeddingStrategy::Spring => 16,
            EmbeddingStrategy::Node2Vec => 16 + dim_bytes,
            EmbeddingStrategy::Spectral => 16 + 2 * dim_bytes,
        }
    }

    /// Byte offset of the timestamp for a given strategy within a record.
    fn timestamp_offset(&self, strategy: EmbeddingStrategy) -> usize {
        let dim_bytes = self.config.dimensions * 4;
        let ts_base = 16 + 3 * dim_bytes;
        match strategy {
            EmbeddingStrategy::Spring => ts_base,
            EmbeddingStrategy::Node2Vec => ts_base + 8,
            EmbeddingStrategy::Spectral => ts_base + 16,
        }
    }

    /// Write an embedding vector for a strategy at the given slot.
    fn write_embedding(&mut self, slot: usize, strategy: EmbeddingStrategy, vector: &[f32]) {
        let base = self.slot_offset(slot) + self.strategy_offset(strategy);
        for (i, &val) in vector.iter().enumerate() {
            let off = base + i * 4;
            self.mmap[off..off + 4].copy_from_slice(&val.to_le_bytes());
        }

        // Write timestamp (current unix millis)
        let ts = chrono::Utc::now().timestamp_millis();
        let ts_off = self.slot_offset(slot) + self.timestamp_offset(strategy);
        self.mmap[ts_off..ts_off + 8].copy_from_slice(&ts.to_le_bytes());
    }

    /// Read an embedding vector for a strategy at the given slot.
    /// Returns None if the timestamp is 0 (meaning no embedding stored for this strategy).
    fn read_embedding(&self, slot: usize, strategy: EmbeddingStrategy) -> Option<Vec<f32>> {
        // Check timestamp first
        let ts_off = self.slot_offset(slot) + self.timestamp_offset(strategy);
        let ts_bytes: [u8; 8] = self.mmap[ts_off..ts_off + 8]
            .try_into()
            .expect("slice is 8 bytes");
        let ts = i64::from_le_bytes(ts_bytes);
        if ts == 0 {
            return None;
        }

        let base = self.slot_offset(slot) + self.strategy_offset(strategy);
        let dims = self.config.dimensions;
        let mut vector = Vec::with_capacity(dims);
        for i in 0..dims {
            let off = base + i * 4;
            let bytes: [u8; 4] = self.mmap[off..off + 4]
                .try_into()
                .expect("slice is 4 bytes");
            vector.push(f32::from_le_bytes(bytes));
        }
        Some(vector)
    }

    /// Read the timestamp (unix millis) for a strategy at the given slot.
    pub fn read_timestamp(&self, slot: usize, strategy: EmbeddingStrategy) -> i64 {
        let ts_off = self.slot_offset(slot) + self.timestamp_offset(strategy);
        let ts_bytes: [u8; 8] = self.mmap[ts_off..ts_off + 8]
            .try_into()
            .expect("slice is 8 bytes");
        i64::from_le_bytes(ts_bytes)
    }

    /// Store an embedding for a specific strategy.
    pub fn store_strategy(
        &mut self,
        node_id: NodeId,
        strategy: EmbeddingStrategy,
        vector: &[f32],
    ) -> Result<()> {
        if vector.len() != self.config.dimensions {
            bail!(
                "Dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                vector.len()
            );
        }

        let slot = if let Some(&existing) = self.index.get(&node_id) {
            existing
        } else {
            self.ensure_capacity()?;
            let slot = self.next_free_slot();
            self.write_uuid(slot, node_id);
            self.index.insert(node_id, slot);
            self.len += 1;
            slot
        };

        self.write_embedding(slot, strategy, vector);
        self.mmap.flush()?;
        Ok(())
    }

    /// Get embedding for a specific strategy.
    pub fn get_strategy(&self, node_id: NodeId, strategy: EmbeddingStrategy) -> Option<Vec<f32>> {
        let &slot = self.index.get(&node_id)?;
        self.read_embedding(slot, strategy)
    }

    /// Get all embeddings for all strategies for a node.
    pub fn get_all_strategies(
        &self,
        node_id: NodeId,
    ) -> Option<HashMap<EmbeddingStrategy, Vec<f32>>> {
        let &slot = self.index.get(&node_id)?;
        let mut result = HashMap::new();
        for strategy in &[
            EmbeddingStrategy::Spring,
            EmbeddingStrategy::Node2Vec,
            EmbeddingStrategy::Spectral,
        ] {
            if let Some(vec) = self.read_embedding(slot, *strategy) {
                result.insert(*strategy, vec);
            }
        }
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Flush mmap to disk.
    pub fn flush(&self) -> Result<()> {
        self.mmap.flush()?;
        Ok(())
    }

    /// Get the dimensions configured for this store.
    pub fn dimensions(&self) -> usize {
        self.config.dimensions
    }

    /// Get the backing file path.
    pub fn file_path(&self) -> &Path {
        &self.config.file_path
    }

    /// Number of nodes stored.
    pub fn node_count(&self) -> usize {
        self.len
    }

    /// Current capacity (number of slots).
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// EmbeddingStore implementation using spring embeddings as primary
/// (matching MultiEmbeddingStore convention).
impl EmbeddingStore for MmapEmbeddingStore {
    fn store(&mut self, node_id: NodeId, vector: Vec<f32>) -> Result<()> {
        self.store_strategy(node_id, EmbeddingStrategy::Spring, &vector)
    }

    fn get(&self, node_id: NodeId) -> Option<&Vec<f32>> {
        // Cannot return a reference into the mmap because the trait requires &Vec<f32>.
        // This is a fundamental limitation: the data lives in the mmap as raw bytes,
        // not as a Vec<f32>. We return None here and users should prefer get_strategy()
        // which returns an owned Vec<f32>.
        //
        // NOTE: For full EmbeddingStore compatibility we would need interior caching.
        // For now, callers needing mmap should use get_strategy() directly.
        let _ = node_id;
        None
    }

    fn query_nearest(&self, query: &[f32], k: usize) -> Result<Vec<(NodeId, f32)>> {
        if query.len() != self.config.dimensions {
            bail!(
                "Query dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                query.len()
            );
        }

        let mut similarities: Vec<(NodeId, f32)> = self
            .index
            .iter()
            .filter_map(|(&node_id, &slot)| {
                self.read_embedding(slot, EmbeddingStrategy::Spring)
                    .map(|vec| {
                        let sim = cosine_similarity(query, &vec);
                        (node_id, sim)
                    })
            })
            .collect();

        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        similarities.truncate(k);
        Ok(similarities)
    }

    fn all_embeddings(&self) -> HashMap<NodeId, Vec<f32>> {
        self.index
            .iter()
            .filter_map(|(&node_id, &slot)| {
                self.read_embedding(slot, EmbeddingStrategy::Spring)
                    .map(|vec| (node_id, vec))
            })
            .collect()
    }

    fn len(&self) -> usize {
        // Count nodes with spring embeddings
        self.index
            .iter()
            .filter(|(_, &slot)| self.read_embedding(slot, EmbeddingStrategy::Spring).is_some())
            .count()
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut mag_a = 0.0f32;
    let mut mag_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }

    let mag_a = mag_a.sqrt();
    let mag_b = mag_b.sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn test_config(dir: &TempDir) -> MmapConfig {
        MmapConfig::new(dir.path().join("embeddings.mmap"))
            .with_dimensions(4)
            .with_initial_capacity(8)
    }

    #[test]
    fn test_store_and_retrieve_spring() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node = Uuid::new_v4();
        let vec = vec![1.0, 2.0, 3.0, 4.0];

        store
            .store_strategy(node, EmbeddingStrategy::Spring, &vec)
            .unwrap();

        let retrieved = store.get_strategy(node, EmbeddingStrategy::Spring).unwrap();
        assert_eq!(retrieved, vec);
    }

    #[test]
    fn test_store_multiple_strategies() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node = Uuid::new_v4();
        let spring_vec = vec![1.0, 0.0, 0.0, 0.0];
        let n2v_vec = vec![0.0, 1.0, 0.0, 0.0];
        let spectral_vec = vec![0.0, 0.0, 1.0, 0.0];

        store
            .store_strategy(node, EmbeddingStrategy::Spring, &spring_vec)
            .unwrap();
        store
            .store_strategy(node, EmbeddingStrategy::Node2Vec, &n2v_vec)
            .unwrap();
        store
            .store_strategy(node, EmbeddingStrategy::Spectral, &spectral_vec)
            .unwrap();

        assert_eq!(
            store.get_strategy(node, EmbeddingStrategy::Spring).unwrap(),
            spring_vec
        );
        assert_eq!(
            store.get_strategy(node, EmbeddingStrategy::Node2Vec).unwrap(),
            n2v_vec
        );
        assert_eq!(
            store.get_strategy(node, EmbeddingStrategy::Spectral).unwrap(),
            spectral_vec
        );

        // get_all_strategies should return all three
        let all = store.get_all_strategies(node).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_persistence_across_reopen() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("persist.mmap");

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let vec1 = vec![1.0, 2.0, 3.0, 4.0];
        let vec2 = vec![5.0, 6.0, 7.0, 8.0];

        // Write data
        {
            let config = MmapConfig::new(&file_path)
                .with_dimensions(4)
                .with_initial_capacity(8);
            let mut store = MmapEmbeddingStore::open(config).unwrap();
            store
                .store_strategy(node1, EmbeddingStrategy::Spring, &vec1)
                .unwrap();
            store
                .store_strategy(node2, EmbeddingStrategy::Node2Vec, &vec2)
                .unwrap();
            store.flush().unwrap();
        }

        // Reopen and verify
        {
            let config = MmapConfig::new(&file_path)
                .with_dimensions(4)
                .with_initial_capacity(8);
            let store = MmapEmbeddingStore::open(config).unwrap();

            assert_eq!(store.node_count(), 2);
            assert_eq!(
                store.get_strategy(node1, EmbeddingStrategy::Spring).unwrap(),
                vec1
            );
            assert_eq!(
                store.get_strategy(node2, EmbeddingStrategy::Node2Vec).unwrap(),
                vec2
            );
            // node1 should not have node2vec
            assert!(store.get_strategy(node1, EmbeddingStrategy::Node2Vec).is_none());
        }
    }

    #[test]
    fn test_o1_lookup() {
        let dir = TempDir::new().unwrap();
        let config = MmapConfig::new(dir.path().join("o1.mmap"))
            .with_dimensions(4)
            .with_initial_capacity(16);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        // Insert several nodes
        let mut nodes = Vec::new();
        for i in 0..10 {
            let node = Uuid::new_v4();
            let vec = vec![i as f32, 0.0, 0.0, 0.0];
            store
                .store_strategy(node, EmbeddingStrategy::Spring, &vec)
                .unwrap();
            nodes.push((node, vec));
        }

        // Verify O(1) lookup: each node's slot offset = index * record_size
        for (node, expected_vec) in &nodes {
            let slot = store.index[node];
            let offset = slot * store.config.record_size();
            // Read UUID directly from mmap
            let uuid_bytes: [u8; 16] = store.mmap[offset..offset + 16]
                .try_into()
                .unwrap();
            assert_eq!(Uuid::from_bytes(uuid_bytes), *node);

            // Read embedding via API
            let retrieved = store.get_strategy(*node, EmbeddingStrategy::Spring).unwrap();
            assert_eq!(&retrieved, expected_vec);
        }
    }

    #[test]
    fn test_dimension_validation() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir); // 4 dimensions
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node = Uuid::new_v4();

        // Wrong dimensions should fail
        let result = store.store_strategy(node, EmbeddingStrategy::Spring, &[1.0, 2.0, 3.0]);
        assert!(result.is_err());

        let result = store.store_strategy(
            node,
            EmbeddingStrategy::Spring,
            &[1.0, 2.0, 3.0, 4.0, 5.0],
        );
        assert!(result.is_err());

        // Correct dimensions should succeed
        let result = store.store_strategy(node, EmbeddingStrategy::Spring, &[1.0, 2.0, 3.0, 4.0]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_capacity_growth() {
        let dir = TempDir::new().unwrap();
        let config = MmapConfig::new(dir.path().join("grow.mmap"))
            .with_dimensions(4)
            .with_initial_capacity(4); // small initial capacity
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        assert_eq!(store.capacity(), 4);

        // Insert more nodes than initial capacity
        for i in 0..10 {
            let node = Uuid::new_v4();
            let vec = vec![i as f32, 0.0, 0.0, 0.0];
            store
                .store_strategy(node, EmbeddingStrategy::Spring, &vec)
                .unwrap();
        }

        assert_eq!(store.node_count(), 10);
        assert!(store.capacity() >= 10);
    }

    #[test]
    fn test_update_existing_node() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node = Uuid::new_v4();
        let vec1 = vec![1.0, 0.0, 0.0, 0.0];
        let vec2 = vec![0.0, 1.0, 0.0, 0.0];

        store
            .store_strategy(node, EmbeddingStrategy::Spring, &vec1)
            .unwrap();
        assert_eq!(
            store.get_strategy(node, EmbeddingStrategy::Spring).unwrap(),
            vec1
        );

        // Update same node, same strategy
        store
            .store_strategy(node, EmbeddingStrategy::Spring, &vec2)
            .unwrap();
        assert_eq!(
            store.get_strategy(node, EmbeddingStrategy::Spring).unwrap(),
            vec2
        );

        // Should still be one node, not two
        assert_eq!(store.node_count(), 1);
    }

    #[test]
    fn test_query_nearest_spring() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let node3 = Uuid::new_v4();

        store
            .store_strategy(node1, EmbeddingStrategy::Spring, &[1.0, 0.0, 0.0, 0.0])
            .unwrap();
        store
            .store_strategy(node2, EmbeddingStrategy::Spring, &[0.9, 0.1, 0.0, 0.0])
            .unwrap();
        store
            .store_strategy(node3, EmbeddingStrategy::Spring, &[0.0, 0.0, 0.0, 1.0])
            .unwrap();

        let results = store.query_nearest(&[1.0, 0.0, 0.0, 0.0], 3).unwrap();
        assert_eq!(results.len(), 3);

        // node1 should be first (identical vector)
        assert_eq!(results[0].0, node1);
        assert!((results[0].1 - 1.0).abs() < 0.001);

        // node2 should be second (very similar)
        assert_eq!(results[1].0, node2);
        assert!(results[1].1 > 0.9);
    }

    #[test]
    fn test_embedding_store_trait() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node = Uuid::new_v4();
        let vec = vec![1.0, 2.0, 3.0, 4.0];

        // Use EmbeddingStore trait methods
        EmbeddingStore::store(&mut store, node, vec.clone()).unwrap();

        // get() returns None (mmap limitation)
        assert!(EmbeddingStore::get(&store, node).is_none());

        // But get_strategy works
        assert_eq!(
            store.get_strategy(node, EmbeddingStrategy::Spring).unwrap(),
            vec
        );

        assert_eq!(EmbeddingStore::len(&store), 1);
        assert!(!store.is_empty());

        let all = store.all_embeddings();
        assert_eq!(all.len(), 1);
        assert_eq!(all[&node], vec);
    }

    #[test]
    fn test_get_nonexistent_node() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let store = MmapEmbeddingStore::open(config).unwrap();

        let fake = Uuid::new_v4();
        assert!(store.get_strategy(fake, EmbeddingStrategy::Spring).is_none());
        assert!(store.get_all_strategies(fake).is_none());
    }

    #[test]
    fn test_timestamps_updated() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let mut store = MmapEmbeddingStore::open(config).unwrap();

        let node = Uuid::new_v4();
        store
            .store_strategy(node, EmbeddingStrategy::Spring, &[1.0, 0.0, 0.0, 0.0])
            .unwrap();

        let slot = store.index[&node];

        // Spring timestamp should be non-zero
        let spring_ts = store.read_timestamp(slot, EmbeddingStrategy::Spring);
        assert!(spring_ts > 0, "Spring timestamp should be set");

        // Node2Vec timestamp should be zero (not stored)
        let n2v_ts = store.read_timestamp(slot, EmbeddingStrategy::Node2Vec);
        assert_eq!(n2v_ts, 0, "Node2Vec timestamp should be zero");
    }

    #[test]
    fn test_empty_store() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let store = MmapEmbeddingStore::open(config).unwrap();

        assert_eq!(store.node_count(), 0);
        assert!(store.is_empty());

        let results = store.query_nearest(&[1.0, 0.0, 0.0, 0.0], 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_record_size_calculation() {
        // 4 dimensions: 16 + 3*(4*4) + 3*8 = 16 + 48 + 24 = 88
        let config = MmapConfig::new("/tmp/test.mmap").with_dimensions(4);
        assert_eq!(config.record_size(), 88);

        // 64 dimensions: 16 + 3*(64*4) + 3*8 = 16 + 768 + 24 = 808
        let config = MmapConfig::new("/tmp/test.mmap").with_dimensions(64);
        assert_eq!(config.record_size(), 808);
    }
}
