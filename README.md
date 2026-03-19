# Fragmentum

Fragmentum is a distributed, erasure-coded block storage engine written in Rust. It provides the foundational primitives for a fault-tolerant storage cluster, handling data sharding, mathematical reconstruction, metadata concurrency, and silent data corruption detection.

## Architecture

The project is structured as a Cargo workspace with four highly decoupled crates:

### 1. `fragmentum-math` (Erasure Coding)
Implements Reed-Solomon erasure coding over the Galois Field $GF(2^8)$.
*   Utilizes a systematic encoding matrix (Vandermonde matrix transformed via Gauss-Jordan elimination).
*   Configured for a 10+4 scheme (10 data shards, 4 parity shards), allowing the system to tolerate the loss of any 4 distinct storage nodes.
*   Uses precomputed logarithm and exponent lookup tables to optimize matrix multiplication throughput without external C dependencies.

### 2. `fragmentum-meta` (Metadata Management)
A thread-safe, in-memory metadata index responsible for tracking file namespaces and chunk locations.
*   Employs a `BTreeMap` for file hierarchy and namespace resolution.
*   Employs a `HashMap` for $O(1)$ chunk location lookups.
*   State is protected by granular `Arc<RwLock>` implementations to support high-throughput concurrent reads and writes from multiple storage nodes.

### 3. `fragmentum-storage` (Storage Node Engine)
An asynchronous local storage daemon built on `tokio`.
*   Handles asynchronous I/O for reading and writing chunk data to disk.
*   **Data Integrity:** Computes CRC32 checksums on the fly during write operations.
*   **Bit-Rot Detection:** Validates checksums on read and during background scrubbing. Corrupted chunks are strictly rejected, forcing the gateway to reconstruct data.

### 4. `fragmentum-fuse` (VFS Gateway / Coordinator)
The entry point that bridges user I/O requests with the distributed cluster.
*   Splits incoming files into padded blocks.
*   Coordinates with `fragmentum-meta` to update namespace and chunk mapping.
*   Distributes encoded shards across `fragmentum-storage` nodes.
*   **Self-Healing:** Transparently handles `StorageError::Corrupted` and `StorageError::NotFound` during read operations by dynamically inverting the surviving shard matrix and reconstructing missing data on the fly.

## Getting Started

### Prerequisites
*   Rust toolchain (stable)

### Running the Test Suite
Due to the decoupled nature of the engine, the primary way to interact with the system is through its integration tests, which simulate cluster behavior, hardware failures, and high-concurrency loads.

Run the test suite in release mode to enable compiler optimizations for matrix operations:

```bash
cargo test --release --workspace -- --nocapture
```
### Key Integration Tests

To run specific load and failure scenarios, use the following commands:

#### 1. Full Cluster End-to-End Recovery

Simulates an end-to-end file write, forcefully deletes 3 storage node directories, injects bit-rot into a 4th node, and validates successful file reconstruction:
```bash
cargo test test_full_cluster_end_to_end_with_failures --release -- --nocapture
```

#### 2. Hyperscale Encoding & Healing Throughput

Benchmarks the Reed-Solomon encoder/decoder throughput (MB/s) on a 140MB data allocation simulating node drops:

```bash
cargo test test_maang_scale_encoding_and_healing --release -- --nocapture
```

#### 3. Concurrent Metadata Access

Benchmarks RwLock contention by spawning 14 threads that concurrently process high-volume chunk location updates:

```bash
cargo test test_maang_scale_concurrent_metadata_access --release -- --nocapture
```

#### 4. Bit-Rot Detection & Scrubbing

Simulates silent data corruption by manually flipping bytes on the disk and verifies that the background scrubber correctly identifies the integrity breach via CRC32 validation:

```bash
cargo test test_storage_node_bit_rot_detection --release -- --nocapture
```

## Design Considerations

* **Memory Safety:** Strict adherence to safe Rust. No unsafe blocks are used.
* **Fault Domain:** The current architecture simulates local storage nodes via isolated directories. Network transport (e.g., gRPC/Tonic) is intentionally abstracted out to focus purely on state management and mathematical fault tolerance.

## License

This project is licensed under the MIT License. You are free to use, modify, and distribute this software. See the [LICENSE](LICENSE) file for more details.