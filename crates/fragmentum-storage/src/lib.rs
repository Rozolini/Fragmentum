use crc32fast::Hasher;
use std::io::Error as IoError;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Storage-specific errors, focusing on I/O and data integrity.
#[derive(Debug)]
pub enum StorageError {
    Io(IoError),
    /// Critical error: Bit-rot (silent data corruption) detected during verification.
    Corrupted {
        expected_crc: u32,
        actual_crc: u32,
    },
    NotFound,
}

impl From<IoError> for StorageError {
    fn from(e: IoError) -> Self {
        StorageError::Io(e)
    }
}

/// Represents a single physical storage node in the cluster.
/// Responsible for asynchronous chunk persistence and local integrity checks.
pub struct StorageNode {
    pub node_id: u32,
    pub base_dir: PathBuf,
}

impl StorageNode {
    /// Initializes a storage node and ensures the underlying data directory exists on disk.
    pub async fn new(node_id: u32, base_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = base_dir.as_ref().to_path_buf();
        if !path.exists() {
            fs::create_dir_all(&path).await?;
        }
        Ok(Self {
            node_id,
            base_dir: path,
        })
    }

    /// Internal helper to generate the absolute path for a data chunk.
    fn get_chunk_path(&self, chunk_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.chunk", chunk_id))
    }

    /// Internal helper to generate the absolute path for a chunk's CRC32 checksum file.
    fn get_crc_path(&self, chunk_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.crc", chunk_id))
    }

    /// Persists a chunk to disk while calculating and storing its CRC32 checksum in parallel.
    pub async fn store_chunk(&self, chunk_id: &str, data: &[u8]) -> Result<(), StorageError> {
        let chunk_path = self.get_chunk_path(chunk_id);
        let crc_path = self.get_crc_path(chunk_id);

        // Calculate the checksum using a high-performance CRC32 hasher.
        let mut hasher = Hasher::new();
        hasher.update(data);
        let checksum = hasher.finalize();

        // Perform asynchronous data write.
        let mut file = File::create(&chunk_path).await?;
        file.write_all(data).await?;

        // Execute fsync to ensure data is physically committed to the hardware medium.
        file.sync_all().await?;

        // Persist the checksum in Big-Endian format.
        fs::write(&crc_path, checksum.to_be_bytes()).await?;

        Ok(())
    }

    /// Reads a chunk and validates its mathematical integrity on the fly.
    /// Returns a `Corrupted` error if the stored CRC32 does not match the computed one.
    pub async fn read_chunk(&self, chunk_id: &str) -> Result<Vec<u8>, StorageError> {
        let chunk_path = self.get_chunk_path(chunk_id);
        let crc_path = self.get_crc_path(chunk_id);

        if !chunk_path.exists() || !crc_path.exists() {
            return Err(StorageError::NotFound);
        }

        let mut data = Vec::new();
        let mut file = File::open(&chunk_path).await?;
        file.read_to_end(&mut data).await?;

        // Retrieve the expected CRC value.
        let crc_bytes = fs::read(&crc_path).await?;
        let expected_crc = u32::from_be_bytes(crc_bytes.try_into().map_err(|_| {
            IoError::new(std::io::ErrorKind::InvalidData, "Invalid CRC file length")
        })?);

        // Compute the actual CRC of the data retrieved from disk.
        let mut hasher = Hasher::new();
        hasher.update(&data);
        let actual_crc = hasher.finalize();

        // Verify data integrity to detect bit-rot.
        if expected_crc != actual_crc {
            return Err(StorageError::Corrupted {
                expected_crc,
                actual_crc,
            });
        }

        Ok(data)
    }

    /// Background Scrubber: Verifies chunk integrity without returning the payload.
    /// Useful for periodic health checks of the storage medium.
    pub async fn verify_chunk(&self, chunk_id: &str) -> Result<bool, StorageError> {
        match self.read_chunk(chunk_id).await {
            Ok(_) => Ok(true),
            Err(StorageError::Corrupted { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
