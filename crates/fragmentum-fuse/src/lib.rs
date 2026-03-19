#![allow(clippy::needless_range_loop)]

use fragmentum_math::reed_solomon::ReedSolomon;
use fragmentum_math::ErasureCoder;
use fragmentum_meta::MetadataStore;
use fragmentum_storage::StorageNode;
use std::sync::Arc;

/// The Virtual File System (VFS) Gateway acts as the cluster coordinator.
/// It intercepts POSIX-like I/O requests, manages erasure coding pipelines,
/// and orchestrates chunk distribution and self-healing across storage nodes.
pub struct VfsGateway {
    meta_store: Arc<MetadataStore>,
    storage_nodes: Vec<Arc<StorageNode>>,
    coder: Arc<ReedSolomon>,
    data_shards: usize,
    parity_shards: usize,
}

impl VfsGateway {
    /// Initializes a new VFS Gateway instance.
    /// Fails if the number of available storage nodes is insufficient for the specified erasure coding config.
    pub fn new(
        meta_store: Arc<MetadataStore>,
        storage_nodes: Vec<Arc<StorageNode>>,
        data_shards: usize,
        parity_shards: usize,
    ) -> Result<Self, &'static str> {
        if storage_nodes.len() < data_shards + parity_shards {
            return Err("Not enough storage nodes for the given erasure coding config");
        }
        let coder = ReedSolomon::new(data_shards, parity_shards)?;
        Ok(Self {
            meta_store,
            storage_nodes,
            coder: Arc::new(coder),
            data_shards,
            parity_shards,
        })
    }

    /// Writes a file to the cluster.
    /// Handles chunk alignment, Reed-Solomon encoding, metadata registration, and node distribution.
    pub async fn put_file(&self, path: &str, data: &[u8]) -> Result<(), String> {
        let size_bytes = data.len() as u64;

        // Register the file in the metadata store namespace.
        let file_meta = self
            .meta_store
            .create_file(path, size_bytes, self.data_shards, self.parity_shards)
            .map_err(|e| e.to_string())?;

        // Calculate aligned chunk size to ensure uniform shard dimensions for matrix operations.
        let chunk_size = data.len().div_ceil(self.data_shards);
        let mut padded_data = data.to_vec();
        padded_data.resize(chunk_size * self.data_shards, 0); // Pad with zeros to meet alignment.

        // Prepare memory buffers for the erasure coder.
        let mut shards = vec![vec![0u8; chunk_size]; self.data_shards + self.parity_shards];

        // Distribute the original data across the first K data shards.
        for i in 0..self.data_shards {
            let start = i * chunk_size;
            let end = start + chunk_size;
            shards[i].copy_from_slice(&padded_data[start..end]);
        }

        // Generate parity shards via Reed-Solomon encoding.
        {
            let (data_part, parity_part) = shards.split_at_mut(self.data_shards);
            let data_refs: Vec<&[u8]> = data_part.iter().map(|s| s.as_slice()).collect();
            let mut parity_refs: Vec<&mut [u8]> =
                parity_part.iter_mut().map(|s| s.as_mut_slice()).collect();

            self.coder
                .encode(&data_refs, &mut parity_refs)
                .map_err(|e| e.to_string())?;
        }

        // Distribute shards across storage nodes and update metadata chunk locations.
        for (i, chunk_id) in file_meta.chunk_ids.iter().enumerate() {
            let node_idx = i % self.storage_nodes.len();
            let node = &self.storage_nodes[node_idx];

            node.store_chunk(chunk_id, &shards[i])
                .await
                .map_err(|e| format!("{:?}", e))?;
            self.meta_store
                .update_chunk_location(chunk_id, node.node_id)
                .unwrap();
        }

        Ok(())
    }

    /// Reads a file from the cluster.
    /// Transparently handles missing or corrupted chunks by reconstructing them on the fly (Self-Healing).
    pub async fn get_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let file_meta = self.meta_store.get_file(path).ok_or("File not found")?;

        let total_shards = self.data_shards + self.parity_shards;

        // Determine uniform chunk size based on stored file metadata.
        let chunk_size = (file_meta.size_bytes as usize).div_ceil(self.data_shards);

        let mut shards = vec![vec![0u8; chunk_size]; total_shards];
        let mut present = vec![false; total_shards];

        // Attempt to fetch all available shards from storage nodes.
        for (i, chunk_id) in file_meta.chunk_ids.iter().enumerate() {
            let chunk_meta = self
                .meta_store
                .get_chunk_locations(chunk_id)
                .ok_or("Chunk metadata missing")?;

            // Retrieve the primary node holding this chunk.
            if let Some(&node_id) = chunk_meta.node_ids.first() {
                // Resolve the node instance in the cluster.
                if let Some(node) = self.storage_nodes.iter().find(|n| n.node_id == node_id) {
                    // Note: Corrupted or missing chunks (e.g., bit-rot) return an error here.
                    // We gracefully ignore the error, leaving `present[i] = false` to trigger mathematical self-healing.
                    if let Ok(data) = node.read_chunk(chunk_id).await {
                        shards[i] = data;
                        present[i] = true;
                    }
                }
            }
        }

        // Verify data shard integrity. Trigger self-healing if any data shard is compromised.
        let data_alive = present[0..self.data_shards].iter().all(|&p| p);
        if !data_alive {
            let mut shard_refs: Vec<&mut [u8]> =
                shards.iter_mut().map(|s| s.as_mut_slice()).collect();
            self.coder
                .reconstruct(&mut shard_refs, &present)
                .map_err(|e| e.to_string())?;
        }

        // Reassemble the original file from the strictly validated data shards.
        let mut file_data = Vec::with_capacity(chunk_size * self.data_shards);
        for i in 0..self.data_shards {
            file_data.extend_from_slice(&shards[i]);
        }

        // Truncate zero-padding added during the chunking phase.
        file_data.truncate(file_meta.size_bytes as usize);

        Ok(file_data)
    }
}
