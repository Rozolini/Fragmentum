use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Metadata representing a single file in the cluster namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub path: String,
    pub size_bytes: u64,
    /// Ordered list of chunk IDs comprising the file.
    /// Order is mathematically critical for Reed-Solomon reconstruction.
    pub chunk_ids: Vec<String>,
}

/// Metadata for an individual data or parity chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMeta {
    pub chunk_id: String,
    /// List of Storage Node IDs where this chunk is currently physically stored.
    pub node_ids: Vec<u32>,
    /// Indicates whether this chunk holds parity data (true) or original payload data (false).
    pub is_parity: bool,
}

/// Thread-safe, in-memory metadata index for the storage cluster.
/// Utilizes a BTreeMap for the namespace hierarchy and a HashMap for O(1) chunk lookups.
pub struct MetadataStore {
    files: Arc<RwLock<BTreeMap<String, FileMeta>>>,
    chunks: Arc<RwLock<HashMap<String, ChunkMeta>>>,
}

impl Default for MetadataStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataStore {
    /// Initializes an empty metadata store.
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(BTreeMap::new())),
            chunks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a new file in the namespace and pre-allocates metadata for its data and parity chunks.
    pub fn create_file(
        &self,
        path: &str,
        size_bytes: u64,
        data_shards: usize,
        parity_shards: usize,
    ) -> Result<FileMeta, &'static str> {
        let mut files_guard = self.files.write().unwrap();

        if files_guard.contains_key(path) {
            return Err("File already exists in the namespace");
        }

        let total_shards = data_shards + parity_shards;
        let mut chunk_ids = Vec::with_capacity(total_shards);
        let mut chunks_guard = self.chunks.write().unwrap();

        for i in 0..total_shards {
            let chunk_id = Uuid::new_v4().to_string();
            chunk_ids.push(chunk_id.clone());

            chunks_guard.insert(
                chunk_id.clone(),
                ChunkMeta {
                    chunk_id,
                    node_ids: Vec::new(), // Initially empty; Storage Nodes must confirm successful physical writes to populate this.
                    is_parity: i >= data_shards,
                },
            );
        }

        let file_meta = FileMeta {
            path: path.to_string(),
            size_bytes,
            chunk_ids,
        };

        files_guard.insert(path.to_string(), file_meta.clone());
        Ok(file_meta)
    }

    /// Retrieves file metadata by its absolute path.
    pub fn get_file(&self, path: &str) -> Option<FileMeta> {
        let guard = self.files.read().unwrap();
        guard.get(path).cloned()
    }

    /// Invoked by Storage Nodes upon successful physical chunk persistence to update the location index.
    pub fn update_chunk_location(&self, chunk_id: &str, node_id: u32) -> Result<(), &'static str> {
        let mut guard = self.chunks.write().unwrap();
        if let Some(chunk) = guard.get_mut(chunk_id) {
            if !chunk.node_ids.contains(&node_id) {
                chunk.node_ids.push(node_id);
            }
            Ok(())
        } else {
            Err("Chunk metadata not found")
        }
    }

    /// Retrieves the physical locations (Storage Node IDs) of a specific chunk.
    pub fn get_chunk_locations(&self, chunk_id: &str) -> Option<ChunkMeta> {
        let guard = self.chunks.read().unwrap();
        guard.get(chunk_id).cloned()
    }
}
