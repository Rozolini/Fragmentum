use fragmentum_storage::{StorageError, StorageNode};
use std::path::PathBuf;
use tokio::fs;

#[tokio::test]
async fn test_storage_node_bit_rot_detection() {
    let temp_dir = PathBuf::from("./temp_storage_test");
    let _ = fs::remove_dir_all(&temp_dir).await; // Cleanup before execution

    let node = StorageNode::new(1, &temp_dir)
        .await
        .expect("Failed to initialize Storage Node");

    let chunk_id = "chunk-xyz-123";
    let original_data = b"Critical MAANG financial data".to_vec();

    // 1. Persistence Phase
    println!("1. Storing chunk securely with CRC32 checksum...");
    node.store_chunk(chunk_id, &original_data)
        .await
        .expect("Initial store failed");

    // 2. Verification Phase (Standard Read)
    println!("2. Performing initial read and integrity verification...");
    let read_data = node
        .read_chunk(chunk_id)
        .await
        .expect("Initial read failed");
    assert_eq!(read_data, original_data);

    // 3. Silent data corruption simulation (Bit-Rot)
    // Simulating a hardware failure where a single byte is flipped on the physical medium (SSD/HDD).
    println!("3. Simulating hardware bit-rot (corrupting 1 byte directly on disk)...");
    let chunk_path = temp_dir.join(format!("{}.chunk", chunk_id));
    let mut corrupted_data = original_data.clone();
    corrupted_data[5] = b'X'; // Modify a single byte bypassing the storage system logic.
    fs::write(&chunk_path, corrupted_data)
        .await
        .expect("Failed to manually corrupt the file");

    // 4. Background Scrubbing Phase
    // The scrubber must identify that the data on disk no longer matches the stored checksum.
    println!("4. Running background integrity scrub...");
    let is_healthy = node
        .verify_chunk(chunk_id)
        .await
        .expect("Scrubbing process failed");
    assert!(
        !is_healthy,
        "CRITICAL FAILURE: Bit-rot went undetected! CRC failed to catch corruption."
    );

    // 5. Error Enforcement Phase
    // Ensure that any attempt to retrieve corrupted data results in a hard error rather than silent failure.
    println!("5. Attempting to retrieve corrupted data...");
    let read_result = node.read_chunk(chunk_id).await;

    assert!(
        matches!(read_result, Err(StorageError::Corrupted { .. })),
        "Node returned corrupted payload to the user! Data integrity was compromised."
    );

    println!("Success! Bit-rot was perfectly identified. The node is ready to signal the cluster for self-healing.");

    // Final Teardown
    let _ = fs::remove_dir_all(&temp_dir).await;
}
