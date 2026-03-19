use fragmentum_math::reed_solomon::ReedSolomon;
use fragmentum_math::ErasureCoder;
use std::time::Instant;

#[test]
fn test_maang_scale_encoding_and_healing() {
    // Hyperscale configuration (e.g., Google Colossus, AWS S3): 10 data shards, 4 parity shards.
    let data_shards = 10;
    let parity_shards = 4;
    let total_shards = data_shards + parity_shards;

    let coder = ReedSolomon::new(data_shards, parity_shards).expect("Failed to initialize RS");

    // Chunk size: 10 MB.
    // Total data payload: 100 MB.
    // Total cluster allocation (with parity): 140 MB.
    let chunk_size = 10 * 1024 * 1024;

    println!(
        "Allocating {} MB of test data...",
        (total_shards * chunk_size) / 1024 / 1024
    );

    // Allocate memory for all 14 shards.
    let mut shards = vec![vec![0u8; chunk_size]; total_shards];

    // Generate 100 MB of pseudo-random data to prevent compiler optimizations on zeroed arrays.
    for i in 0..data_shards {
        for j in 0..chunk_size {
            shards[i][j] = ((i + j) % 255) as u8;
        }
    }

    // Retain a copy of the original chunk #3 for post-recovery integrity verification.
    let original_chunk_3 = shards[3].clone();

    // 1. ENCODING PHASE (Stress Test)
    {
        // Split the array into data and parity partitions to satisfy Rust's strict aliasing and borrowing rules.
        let (data_part, parity_part) = shards.split_at_mut(data_shards);

        let data_refs: Vec<&[u8]> = data_part.iter().map(|b| b.as_slice()).collect();
        let mut parity_refs: Vec<&mut [u8]> =
            parity_part.iter_mut().map(|b| b.as_mut_slice()).collect();

        println!("Starting heavy encoding process...");
        let start = Instant::now();

        coder
            .encode(&data_refs, &mut parity_refs)
            .expect("Encoding failed");

        let duration = start.elapsed();
        let mb_processed = (data_shards * chunk_size) as f64 / 1_048_576.0;

        println!("Encoded {:.2} MB in {:?}", mb_processed, duration);
        println!(
            "Write Throughput: {:.2} MB/s",
            mb_processed / duration.as_secs_f64()
        );
    }

    // 2. LARGE-SCALE DISASTER SIMULATION (4 Nodes Down)
    // Simulate the catastrophic loss of 2 data nodes and 2 parity nodes.
    let missing_indices = vec![3, 7, 10, 12];
    for &idx in &missing_indices {
        shards[idx].fill(0); // Simulate complete data loss on disk.
    }

    let mut present = vec![true; total_shards];
    for &idx in &missing_indices {
        present[idx] = false;
    }

    // 3. RECONSTRUCTION PHASE (Self-Healing under load)
    {
        let mut shard_refs: Vec<&mut [u8]> = shards.iter_mut().map(|b| b.as_mut_slice()).collect();

        println!("Starting heavy reconstruction process...");
        let start = Instant::now();

        coder
            .reconstruct(&mut shard_refs, &present)
            .expect("Reconstruction failed");

        let duration = start.elapsed();
        // Calculate throughput based on the actual amount of data successfully restored.
        let mb_restored = (missing_indices.len() * chunk_size) as f64 / 1_048_576.0;

        println!(
            "Restored {:.2} MB of lost data in {:?}",
            mb_restored, duration
        );
        println!(
            "Heal Throughput: {:.2} MB/s",
            mb_restored / duration.as_secs_f64()
        );
    }

    // 4. DATA INTEGRITY VERIFICATION
    assert_eq!(
        shards[3], original_chunk_3,
        "MAANG level integration test failed: Data corruption detected!"
    );
    println!("Data integrity verified. Self-healing successful.");
}
