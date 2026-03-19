#![allow(clippy::needless_range_loop)]

use crate::gf;
use crate::matrix::Matrix;
use crate::{ErasureCoder, ErasureError};

/// Implementation of Reed-Solomon erasure coding over Galois Field GF(2^8).
pub struct ReedSolomon {
    data_shards: usize,
    parity_shards: usize,
    /// The encoding matrix of size (data + parity) x data.
    encoding_matrix: Matrix,
}

impl ReedSolomon {
    /// Initializes a new Reed-Solomon codec.
    /// Constructs a Vandermonde matrix and transforms it into a systematic encoding matrix.
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self, &'static str> {
        let total_shards = data_shards + parity_shards;
        if total_shards > 255 {
            return Err("Total shards cannot exceed 255 in GF(2^8)");
        }

        let mut m = Matrix::new(total_shards, data_shards);

        // 1. Construct the standard Vandermonde matrix.
        for r in 0..total_shards {
            for c in 0..data_shards {
                m.set(r, c, Self::gf_pow(r as u8, c as u8));
            }
        }

        // 2. Make the matrix systematic:
        // Extract the top KxK submatrix, invert it, and multiply the entire matrix by this inverse.
        // This ensures the top K rows form an identity matrix, meaning original data is kept intact.
        let mut top = Matrix::new(data_shards, data_shards);
        for r in 0..data_shards {
            for c in 0..data_shards {
                top.set(r, c, m.get(r, c));
            }
        }

        let top_inv = top.invert()?;
        let encoding_matrix = m.multiply(&top_inv);

        Ok(Self {
            data_shards,
            parity_shards,
            encoding_matrix,
        })
    }

    /// Helper function for exponentiation in GF(2^8).
    fn gf_pow(a: u8, n: u8) -> u8 {
        if n == 0 {
            return 1;
        }
        if a == 0 {
            return 0;
        }
        let mut res = 1;
        for _ in 0..n {
            res = gf::mul(res, a);
        }
        res
    }
}

impl ErasureCoder for ReedSolomon {
    fn data_shards(&self) -> usize {
        self.data_shards
    }
    fn parity_shards(&self) -> usize {
        self.parity_shards
    }

    fn encode(&self, data: &[&[u8]], parity: &mut [&mut [u8]]) -> Result<(), ErasureError> {
        if data.len() != self.data_shards {
            return Err(ErasureError::InvalidDataChunkCount {
                expected: self.data_shards,
                got: data.len(),
            });
        }
        if parity.len() != self.parity_shards {
            return Err(ErasureError::InvalidParityChunkCount {
                expected: self.parity_shards,
                got: parity.len(),
            });
        }

        let chunk_size = data[0].len();
        for d in data {
            if d.len() != chunk_size {
                return Err(ErasureError::InconsistentChunkLengths);
            }
        }
        for p in parity.iter() {
            if p.len() != chunk_size {
                return Err(ErasureError::InconsistentChunkLengths);
            }
        }

        // Compute each parity block via the dot product of the encoding matrix row and the data vector.
        for p_idx in 0..self.parity_shards {
            let row = self.data_shards + p_idx;
            for i in 0..chunk_size {
                let mut val = 0;
                for d_idx in 0..self.data_shards {
                    val = gf::add(
                        val,
                        gf::mul(self.encoding_matrix.get(row, d_idx), data[d_idx][i]),
                    );
                }
                parity[p_idx][i] = val;
            }
        }
        Ok(())
    }

    fn reconstruct(&self, shards: &mut [&mut [u8]], present: &[bool]) -> Result<(), ErasureError> {
        if shards.len() != self.total_shards() || present.len() != self.total_shards() {
            return Err(ErasureError::InvalidDataChunkCount {
                expected: self.total_shards(),
                got: shards.len(),
            });
        }

        let valid_count = present.iter().filter(|&&p| p).count();
        if valid_count < self.data_shards {
            return Err(ErasureError::TooManyMissingChunks {
                available: valid_count,
                required: self.data_shards,
            });
        }

        let data_alive = present[0..self.data_shards].iter().all(|&p| p);
        if data_alive {
            // TODO (Optimization): If all data shards are intact, we only need to recompute parity.
            // A direct call to `encode()` here would bypass the matrix inversion overhead.
        }

        // 1. Construct the decoding matrix by selecting K valid rows from the original encoding matrix.
        let mut decode_matrix = Matrix::new(self.data_shards, self.data_shards);
        let mut valid_indices = Vec::with_capacity(self.data_shards);

        let mut r = 0;
        for (i, &is_present) in present.iter().enumerate() {
            if is_present {
                valid_indices.push(i);
                for c in 0..self.data_shards {
                    decode_matrix.set(r, c, self.encoding_matrix.get(i, c));
                }
                r += 1;
                if r == self.data_shards {
                    break;
                }
            }
        }

        // 2. Invert the decoding matrix to find the reconstruction coefficients.
        let inverted = decode_matrix
            .invert()
            .map_err(|_| ErasureError::TooManyMissingChunks {
                available: valid_count,
                required: self.data_shards,
            })?;

        // 3. Reconstruct the original K data blocks into a temporary buffer.
        let chunk_size = shards[0].len();
        let mut recovered_data = vec![vec![0u8; chunk_size]; self.data_shards];

        for c_idx in 0..chunk_size {
            for r_idx in 0..self.data_shards {
                let mut val = 0;
                for (v_idx, &shard_idx) in valid_indices.iter().enumerate() {
                    val = gf::add(
                        val,
                        gf::mul(inverted.get(r_idx, v_idx), shards[shard_idx][c_idx]),
                    );
                }
                recovered_data[r_idx][c_idx] = val;
            }
        }

        // 4. Write the recovered data back into the original shard array.
        for i in 0..self.data_shards {
            if !present[i] {
                shards[i].copy_from_slice(&recovered_data[i]);
            }
        }

        // 5. Recompute any missing parity shards.
        for p_idx in 0..self.parity_shards {
            let shard_idx = self.data_shards + p_idx;
            if !present[shard_idx] {
                for i in 0..chunk_size {
                    let mut val = 0;
                    for d_idx in 0..self.data_shards {
                        val = gf::add(
                            val,
                            gf::mul(self.encoding_matrix.get(shard_idx, d_idx), shards[d_idx][i]),
                        );
                    }
                    shards[shard_idx][i] = val;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reed_solomon_encode_and_reconstruct() {
        let rs = ReedSolomon::new(4, 2).unwrap(); // 4 data shards, 2 parity shards (Total: 6)
        let chunk_size = 5;

        // Initial data payload: 4 chunks, 5 bytes each.
        let mut d1 = vec![1, 2, 3, 4, 5];
        let mut d2 = vec![6, 7, 8, 9, 10];
        let mut d3 = vec![11, 12, 13, 14, 15];
        let mut d4 = vec![16, 17, 18, 19, 20];

        let mut p1 = vec![0; chunk_size];
        let mut p2 = vec![0; chunk_size];

        // 1. Encode Phase
        {
            let data: &[&[u8]] = &[&d1, &d2, &d3, &d4];
            let mut parity: Vec<&mut [u8]> = vec![&mut p1, &mut p2];
            rs.encode(data, &mut parity).unwrap();
        }

        // Simulate catastrophic data loss (destroy shards d2 and d4).
        d2.fill(0);
        d4.fill(0);

        // Boolean mask representing surviving shards.
        let present = vec![true, false, true, false, true, true];

        // 2. Reconstruction Phase
        {
            let mut all_shards: Vec<&mut [u8]> =
                vec![&mut d1, &mut d2, &mut d3, &mut d4, &mut p1, &mut p2];
            rs.reconstruct(&mut all_shards, &present).unwrap();
        }

        // 3. Verification Phase
        assert_eq!(d2, vec![6, 7, 8, 9, 10], "Shard 2 failed to reconstruct");
        assert_eq!(
            d4,
            vec![16, 17, 18, 19, 20],
            "Shard 4 failed to reconstruct"
        );
    }
}
