//! Finite Galois Field GF(2^8) arithmetic for Reed-Solomon encoding.
//! Utilizes precomputed logarithm and exponent tables to achieve O(1) multiplication throughput.

const GF_POLYNOMIAL: u16 = 0x11D; // x^8 + x^4 + x^3 + x^2 + 1 (standard polynomial for Reed-Solomon)

pub struct GfTables {
    pub exp: [u8; 512],
    pub log: [u8; 256],
}

/// Generates lookup tables at compile time via `const fn` to eliminate runtime initialization overhead.
const fn generate_tables() -> GfTables {
    let mut exp = [0u8; 512];
    let mut log = [0u8; 256];

    let mut x = 1u16;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        exp[i + 255] = x as u8; // Duplicate the exponent table to avoid the modulo 255 operation during multiplication
        log[x as usize] = i as u8;

        x <<= 1;
        if x & 0x100 != 0 {
            x ^= GF_POLYNOMIAL;
        }
        i += 1;
    }
    log[0] = 0; // Log(0) is mathematically undefined; handled explicitly in mul/div functions

    GfTables { exp, log }
}

pub static TABLES: GfTables = generate_tables();

/// Addition in GF(2^8) is evaluated as a bitwise XOR.
#[inline]
pub fn add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Subtraction in GF(2^8) is identical to addition (bitwise XOR).
#[inline]
pub fn sub(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Multiplies two numbers in GF(2^8) using precomputed logarithm tables.
#[inline]
pub fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        0
    } else {
        let log_a = TABLES.log[a as usize] as usize;
        let log_b = TABLES.log[b as usize] as usize;
        TABLES.exp[log_a + log_b]
    }
}

/// Divides two numbers in GF(2^8) using precomputed logarithm tables.
#[inline]
pub fn div(a: u8, b: u8) -> u8 {
    if a == 0 {
        0
    } else if b == 0 {
        panic!("Divide by zero in GF(2^8)");
    } else {
        let log_a = TABLES.log[a as usize] as isize;
        let log_b = TABLES.log[b as usize] as isize;
        let mut diff = log_a - log_b;
        if diff < 0 {
            diff += 255;
        }
        TABLES.exp[diff as usize]
    }
}
