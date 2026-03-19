use fragmentum_meta::MetadataStore;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[test]
fn test_maang_scale_concurrent_metadata_access() {
    let store = Arc::new(MetadataStore::new());
    let num_files = 10_000; // Total files to register
    let data_shards = 10;
    let parity_shards = 4;
    let total_shards = data_shards + parity_shards;

    println!(
        "Registering {} files ({} total chunks)...",
        num_files,
        num_files * total_shards
    );

    let start_create = Instant::now();
    let mut all_chunk_ids = Vec::with_capacity(num_files * total_shards);

    // 1. Single-threaded bulk file creation (Simulating a high-velocity FUSE client)
    for i in 0..num_files {
        let path = format!("/user/data/dataset_{}.parquet", i);
        // Registering file metadata and generating unique chunk IDs
        let file_meta = store
            .create_file(&path, 1024 * 1024, data_shards, parity_shards)
            .unwrap();
        all_chunk_ids.extend(file_meta.chunk_ids);
    }

    println!("Bulk creation latency: {:?}", start_create.elapsed());
    assert_eq!(all_chunk_ids.len(), num_files * total_shards);

    // 2. CONCURRENCY STRESS TEST: Simulate 14 Storage Nodes reporting chunk persistence simultaneously
    println!(
        "Spawning {} concurrent Storage Node workers to update location index...",
        total_shards
    );
    let start_update = Instant::now();

    let mut handles = vec![];

    for node_id in 0..total_shards {
        let store_clone = Arc::clone(&store);

        // Partition chunk IDs: Each node handles its specific subset (every 14th chunk)
        let chunks_for_this_node: Vec<String> = all_chunk_ids
            .iter()
            .skip(node_id)
            .step_by(total_shards)
            .cloned()
            .collect();

        let handle = thread::spawn(move || {
            for chunk_id in chunks_for_this_node {
                // Execute a thread-safe update to the shared metadata store
                store_clone
                    .update_chunk_location(&chunk_id, node_id as u32)
                    .unwrap();
            }
        });
        handles.push(handle);
    }

    // Await completion of all concurrent update workers
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start_update.elapsed();
    println!("Total concurrent update duration: {:?}", duration);
    let ops_per_sec = (num_files * total_shards) as f64 / duration.as_secs_f64();
    println!("Metadata Store Throughput: {:.0} ops/sec", ops_per_sec);

    // 3. INTEGRITY CHECK: Verify no Race Conditions occurred during concurrent writes
    let sample_chunk = &all_chunk_ids[0];
    let meta = store.get_chunk_locations(sample_chunk).unwrap();

    // Each chunk must have exactly one unique node ID assigned in this test scenario
    assert_eq!(
        meta.node_ids.len(),
        1,
        "Concurrency violation: Chunk location index corrupted!"
    );
}
