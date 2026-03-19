//! Matrix operations over the Galois Field GF(2^8).
//! Essential for generating the encoding matrix and its inversion during data reconstruction (via Gauss-Jordan elimination).

use crate::gf;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<u8>,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0; rows * cols],
        }
    }

    pub fn identity(size: usize) -> Self {
        let mut m = Self::new(size, size);
        for i in 0..size {
            m.set(i, i, 1);
        }
        m
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> u8 {
        self.data[r * self.cols + c]
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize, val: u8) {
        self.data[r * self.cols + c] = val;
    }

    pub fn multiply(&self, other: &Matrix) -> Self {
        assert_eq!(
            self.cols, other.rows,
            "Incompatible matrices for multiplication"
        );
        let mut result = Self::new(self.rows, other.cols);
        for r in 0..self.rows {
            for c in 0..other.cols {
                let mut val = 0;
                for i in 0..self.cols {
                    val = gf::add(val, gf::mul(self.get(r, i), other.get(i, c)));
                }
                result.set(r, c, val);
            }
        }
        result
    }

    /// Inverts the matrix using Gauss-Jordan elimination over GF(2^8).
    /// This mathematical operation is critical for reconstructing missing shards.
    pub fn invert(&self) -> Result<Self, &'static str> {
        assert_eq!(self.rows, self.cols, "Matrix must be square to invert");
        let size = self.rows;
        let mut work = Self::new(size, size * 2);

        // Construct the augmented matrix [A | I]
        for r in 0..size {
            for c in 0..size {
                work.set(r, c, self.get(r, c));
            }
            work.set(r, size + r, 1);
        }

        // Forward elimination phase
        for r in 0..size {
            if work.get(r, r) == 0 {
                // Find a row to swap if the diagonal element (pivot) is zero
                let mut swap_row = r + 1;
                while swap_row < size && work.get(swap_row, r) == 0 {
                    swap_row += 1;
                }
                if swap_row == size {
                    return Err("Matrix is singular (not invertible)");
                }
                // Swap the rows
                for c in 0..size * 2 {
                    let tmp = work.get(r, c);
                    work.set(r, c, work.get(swap_row, c));
                    work.set(swap_row, c, tmp);
                }
            }

            // Normalize the pivot row (divide by the pivot element)
            let pivot = work.get(r, r);
            for c in 0..size * 2 {
                work.set(r, c, gf::div(work.get(r, c), pivot));
            }

            // Eliminate elements below the pivot
            for i in (r + 1)..size {
                let factor = work.get(i, r);
                if factor != 0 {
                    for c in 0..size * 2 {
                        let val = gf::add(work.get(i, c), gf::mul(factor, work.get(r, c)));
                        work.set(i, c, val);
                    }
                }
            }
        }

        // Backward substitution phase
        for r in (0..size).rev() {
            for i in 0..r {
                let factor = work.get(i, r);
                if factor != 0 {
                    for c in 0..size * 2 {
                        let val = gf::add(work.get(i, c), gf::mul(factor, work.get(r, c)));
                        work.set(i, c, val);
                    }
                }
            }
        }

        // Extract the right half of the augmented matrix (the inverted matrix)
        let mut result = Self::new(size, size);
        for r in 0..size {
            for c in 0..size {
                result.set(r, c, work.get(r, size + c));
            }
        }

        Ok(result)
    }
}
