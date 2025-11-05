use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tower::ServiceExt;

// Helper function to create test environment
fn setup_test_env() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

// Helper function to read JSON file
fn read_json_file(path: &PathBuf) -> Value {
    let content = fs::read_to_string(path).expect("Failed to read file");
    serde_json::from_str(&content).expect("Failed to parse JSON")
}

#[tokio::test]
async fn test_nostr_keypair_generation() {
    let temp_dir = setup_test_env();
    let data_dir = temp_dir.path().to_path_buf();
    let keys_file = data_dir.join("nostr_keys.json");

    // Set environment variable
    std::env::set_var("DATA_DIR", data_dir.to_str().unwrap());

    // First run: Generate keys
    let output1 = std::process::Command::new("cargo")
        .arg("run")
        .env("DATA_DIR", data_dir.to_str().unwrap())
        .env("LISTEN_ADDR", "127.0.0.1:18080")
        .env("DSTACK_URL", "http://localhost:19060")
        .spawn();

    // Give it time to start and generate keys
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Kill the process
    if let Ok(mut child) = output1 {
        let _ = child.kill();
    }

    // Verify keys file was created
    assert!(keys_file.exists(), "Nostr keys file should be created");

    let keys_content = fs::read_to_string(&keys_file).expect("Failed to read keys file");
    assert!(
        keys_content.starts_with("nsec"),
        "Keys file should contain a nostr secret key"
    );

    println!("Test: Nostr keypair generation - PASSED");
}

#[tokio::test]
async fn test_whitelist_file_creation() {
    let temp_dir = setup_test_env();
    let data_dir = temp_dir.path().to_path_buf();
    let whitelist_file = data_dir.join("whitelist.json");

    // Set environment variable
    std::env::set_var("DATA_DIR", data_dir.to_str().unwrap());

    // Run the application
    let output = std::process::Command::new("cargo")
        .arg("run")
        .env("DATA_DIR", data_dir.to_str().unwrap())
        .env("LISTEN_ADDR", "127.0.0.1:18081")
        .env("DSTACK_URL", "http://localhost:19060")
        .spawn();

    // Give it time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Kill the process
    if let Ok(mut child) = output {
        let _ = child.kill();
    }

    // Verify whitelist file was created
    assert!(
        whitelist_file.exists(),
        "Whitelist file should be created"
    );

    let whitelist_data = read_json_file(&whitelist_file);
    let pubkeys = whitelist_data["pubkeys"]
        .as_array()
        .expect("pubkeys should be an array");

    assert_eq!(
        pubkeys.len(),
        1,
        "Whitelist should contain exactly one pubkey"
    );
    assert!(
        pubkeys[0].as_str().unwrap().starts_with("npub"),
        "Pubkey should be in bech32 format"
    );

    println!("Test: Whitelist file creation - PASSED");
}

#[tokio::test]
async fn test_whitelist_api_endpoint() {
    // This test requires a running server, so we'll start one
    let temp_dir = setup_test_env();
    let data_dir = temp_dir.path().to_path_buf();

    // Start the server in the background
    let mut child = std::process::Command::new("cargo")
        .arg("run")
        .env("DATA_DIR", data_dir.to_str().unwrap())
        .env("LISTEN_ADDR", "127.0.0.1:18082")
        .env("DSTACK_URL", "http://localhost:19060")
        .spawn()
        .expect("Failed to start server");

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Read the generated pubkey from whitelist
    let whitelist_file = data_dir.join("whitelist.json");
    let whitelist_data = read_json_file(&whitelist_file);
    let pubkeys = whitelist_data["pubkeys"]
        .as_array()
        .expect("pubkeys should be an array");
    let valid_pubkey = pubkeys[0].as_str().unwrap();

    // Test 1: Valid pubkey (should be whitelisted)
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:18082/api/whitelist")
        .query(&[("pubkey", valid_pubkey)])
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["is_whitelisted"], true);
    assert_eq!(body["pubkey"], valid_pubkey);

    println!(
        "Test: Whitelist API with valid pubkey - PASSED (pubkey: {})",
        valid_pubkey
    );

    // Test 2: Invalid pubkey (should not be whitelisted)
    let invalid_pubkey = "npub1invalid1234567890abcdefghijklmnopqrstuvwxyz";
    let response = client
        .get("http://127.0.0.1:18082/api/whitelist")
        .query(&[("pubkey", invalid_pubkey)])
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["is_whitelisted"], false);
    assert_eq!(body["pubkey"], invalid_pubkey);

    println!("Test: Whitelist API with invalid pubkey - PASSED");

    // Kill the server
    let _ = child.kill();

    println!("Test: Whitelist API endpoint - ALL TESTS PASSED");
}

#[tokio::test]
async fn test_health_endpoint_contains_ip() {
    let temp_dir = setup_test_env();
    let data_dir = temp_dir.path().to_path_buf();

    // Start the server
    let mut child = std::process::Command::new("cargo")
        .arg("run")
        .env("DATA_DIR", data_dir.to_str().unwrap())
        .env("LISTEN_ADDR", "127.0.0.1:18083")
        .env("DSTACK_URL", "http://localhost:19060")
        .spawn()
        .expect("Failed to start server");

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Call health endpoint
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:18083/health")
        .send()
        .await
        .expect("Failed to send request");

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Check that ip_address field exists
    assert!(
        body.get("ip_address").is_some(),
        "Health response should contain ip_address field"
    );

    // Check that pubkeys are present and contain at least one entry
    let pubkeys = body["pubkeys"]
        .as_array()
        .expect("pubkeys should be an array");
    assert!(
        pubkeys.len() >= 1,
        "Health response should contain at least one pubkey"
    );

    println!("Test: Health endpoint contains IP address and pubkeys - PASSED");
    println!("Response: {}", serde_json::to_string_pretty(&body).unwrap());

    // Kill the server
    let _ = child.kill();
}

#[tokio::test]
async fn test_keypair_persistence() {
    let temp_dir = setup_test_env();
    let data_dir = temp_dir.path().to_path_buf();
    let keys_file = data_dir.join("nostr_keys.json");

    // First run: Generate keys
    let mut child1 = std::process::Command::new("cargo")
        .arg("run")
        .env("DATA_DIR", data_dir.to_str().unwrap())
        .env("LISTEN_ADDR", "127.0.0.1:18084")
        .env("DSTACK_URL", "http://localhost:19060")
        .spawn()
        .expect("Failed to start server");

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Read the keys
    let keys_content_1 = fs::read_to_string(&keys_file).expect("Failed to read keys file");

    // Kill first instance
    let _ = child1.kill();
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Second run: Should load existing keys
    let mut child2 = std::process::Command::new("cargo")
        .arg("run")
        .env("DATA_DIR", data_dir.to_str().unwrap())
        .env("LISTEN_ADDR", "127.0.0.1:18085")
        .env("DSTACK_URL", "http://localhost:19060")
        .spawn()
        .expect("Failed to start server");

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Read the keys again
    let keys_content_2 = fs::read_to_string(&keys_file).expect("Failed to read keys file");

    // Verify keys are the same
    assert_eq!(
        keys_content_1, keys_content_2,
        "Keys should persist across restarts"
    );

    println!("Test: Keypair persistence - PASSED");

    // Kill second instance
    let _ = child2.kill();
}
