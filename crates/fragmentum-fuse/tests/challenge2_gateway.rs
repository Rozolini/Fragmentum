use fragmentum_fuse::VfsGateway;
use fragmentum_meta::MetadataStore;
use fragmentum_storage::StorageNode;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

#[tokio::test]
async fn test_full_cluster_end_to_end_with_failures() {
    let base_dir = PathBuf::from("./temp_cluster");
    let _ = fs::remove_dir_all(&base_dir).await; // Clean up previous test state

    let data_shards = 10;
    let parity_shards = 4;
    let total_nodes = data_shards + parity_shards;

    // 1. Initialize the central metadata store
    let meta_store = Arc::new(MetadataStore::new());

    // 2. Spin up 14 isolated storage nodes (simulating physical servers)
    let mut storage_nodes = Vec::new();
    for i in 0..total_nodes {
        let node_dir = base_dir.join(format!("node_{}", i));
        let node = StorageNode::new(i as u32, &node_dir).await.unwrap();
        storage_nodes.push(Arc::new(node));
    }

    // 3. Initialize the VFS Gateway coordinator
    let gateway = VfsGateway::new(
        meta_store.clone(),
        storage_nodes.clone(),
        data_shards,
        parity_shards,
    )
    .expect("Failed to initialize Gateway");

    // 4. Ingest a mission-critical file into the cluster
    let file_path = "/mission_critical_doc.txt";
    let original_data =
        b"This is top secret MAANG architecture data. Do not lose a single byte!".to_vec();

    println!("-> Putting file into the cluster...");
    gateway
        .put_file(file_path, &original_data)
        .await
        .expect("Failed to put file");

    // 5. Data center disaster  simulation
    println!("-> Simulating failure: kill 3 nodes, corrupt 1 node...");

    // Completely wipe storage nodes 0, 3, and 5 (simulating permanent hardware death)
    let dead_nodes = [0, 3, 5];
    for &node_id in &dead_nodes {
        let node_dir = base_dir.join(format!("node_{}", node_id));
        let _ = fs::remove_dir_all(&node_dir).await;
    }

    // Inject Bit-Rot (silent data corruption) into node 8
    println!("-> Corrupting node 8 with Bit-Rot...");
    let node_8_dir = base_dir.join("node_8");
    let mut entries = fs::read_dir(&node_8_dir)
        .await
        .expect("Node 8 directory should exist!");
    while let Some(entry) = entries.next_entry().await.unwrap() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("chunk") {
            // Read raw chunk bytes from disk
            let mut corrupted = fs::read(entry.path()).await.unwrap();
            if !corrupted.is_empty() {
                // Invert the first byte (this invalidates the CRC32 checksum, causing StorageNode to reject reads)
                corrupted[0] ^= 0xFF;
            }
            // Write the corrupted payload back to disk
            fs::write(entry.path(), corrupted).await.unwrap();
        }
    }

    // 6. Retrieve the file via the Gateway
    println!("-> Reading file from the cluster...");
    let recovered_data = gateway
        .get_file(file_path)
        .await
        .expect("Failed to get file");

    // 7. Verify data integrity
    assert_eq!(
        original_data, recovered_data,
        "DATA CORRUPTION DETECTED! End-to-end recovery failed."
    );

    println!("-> SUCCESS! The Gateway successfully masked 4 hardware failures and returned perfect data.");

    let _ = fs::remove_dir_all(&base_dir).await; // Teardown test environment
}
