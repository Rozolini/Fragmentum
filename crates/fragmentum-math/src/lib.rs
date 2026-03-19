#![forbid(unsafe_code)] // Ensure memory safety; unsafe blocks will be added later only if SIMD optimization is required.

pub mod gf;
pub mod matrix;
pub mod reed_solomon;

use std::fmt;

/// Errors that can occur during the encoding or reconstruction phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErasureError {
    /// The lengths of all shards (data and parity) must be strictly identical.
    InconsistentChunkLengths,
    /// The number of provided data shards does not match the encoder's configuration.
    InvalidDataChunkCount { expected: usize, got: usize },
    /// The number of provided parity shards does not match the encoder's configuration.
    InvalidParityChunkCount { expected: usize, got: usize },
    /// Too many shards are missing to successfully reconstruct the original data.
    TooManyMissingChunks { available: usize, required: usize },
}

impl fmt::Display for ErasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InconsistentChunkLengths => {
                write!(f, "All shards must have the exact same length")
            }
            Self::InvalidDataChunkCount { expected, got } => {
                write!(f, "Expected {} data shards, got {}", expected, got)
            }
            Self::InvalidParityChunkCount { expected, got } => {
                write!(f, "Expected {} parity shards, got {}", expected, got)
            }
            Self::TooManyMissingChunks {
                available,
                required,
            } => write!(
                f,
                "Cannot reconstruct: available {} shards, but need at least {}",
                available, required
            ),
        }
    }
}

impl std::error::Error for ErasureError {}

/// The core trait defining the contract for any Erasure Coding engine within Fragmentum.
pub trait ErasureCoder {
    /// Returns the number of data shards ($k$).
    fn data_shards(&self) -> usize;

    /// Returns the number of parity shards ($m$).
    fn parity_shards(&self) -> usize;

    /// Returns the total number of shards ($n = k + m$).
    fn total_shards(&self) -> usize {
        self.data_shards() + self.parity_shards()
    }

    /// Encodes the input data shards and computes the corresponding parity shards.
    ///
    /// Note: This method enforces zero-allocation by taking `&[&[u8]]` for read-only data
    /// and `&mut [&mut [u8]]` for mutable parity buffers where the result is written directly.
    fn encode(&self, data: &[&[u8]], parity: &mut [&mut [u8]]) -> Result<(), ErasureError>;

    /// Recovers missing shards (both data and parity) in-place within the provided slices.
    ///
    /// * `shards` - A mutable array of all slices ($n = k + m$).
    /// * `present` - A boolean array indicating which shards are currently valid and which need to be overwritten/reconstructed.
    fn reconstruct(&self, shards: &mut [&mut [u8]], present: &[bool]) -> Result<(), ErasureError>;
}
