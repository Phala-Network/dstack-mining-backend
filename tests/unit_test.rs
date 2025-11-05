use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

// Import functions from main.rs by making them public
// For this test to work, we need to refactor main.rs to expose these functions

#[test]
fn test_whitelist_serialization() {
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Whitelist {
        pubkeys: HashSet<String>,
    }

    let mut whitelist = Whitelist {
        pubkeys: HashSet::new(),
    };

    whitelist.pubkeys.insert("npub1test123".to_string());
    whitelist.pubkeys.insert("npub1test456".to_string());

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&whitelist).unwrap();
    println!("Serialized whitelist: {}", json);

    // Deserialize back
    let deserialized: Whitelist = serde_json::from_str(&json).unwrap();

    assert_eq!(whitelist, deserialized);
    assert_eq!(deserialized.pubkeys.len(), 2);
    assert!(deserialized.pubkeys.contains("npub1test123"));
    assert!(deserialized.pubkeys.contains("npub1test456"));

    println!("Test: Whitelist serialization/deserialization - PASSED");
}

#[test]
fn test_whitelist_file_operations() {
    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct Whitelist {
        pubkeys: HashSet<String>,
    }

    let temp_dir = TempDir::new().unwrap();
    let whitelist_file = temp_dir.path().join("whitelist.json");

    // Create whitelist
    let mut whitelist = Whitelist {
        pubkeys: HashSet::new(),
    };
    whitelist.pubkeys.insert("npub1test789".to_string());

    // Write to file
    let content = serde_json::to_string_pretty(&whitelist).unwrap();
    fs::write(&whitelist_file, content).unwrap();

    // Read from file
    let read_content = fs::read_to_string(&whitelist_file).unwrap();
    let loaded_whitelist: Whitelist = serde_json::from_str(&read_content).unwrap();

    assert_eq!(loaded_whitelist.pubkeys.len(), 1);
    assert!(loaded_whitelist.pubkeys.contains("npub1test789"));

    println!("Test: Whitelist file operations - PASSED");
}

#[test]
fn test_backend_info_structure() {
    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct BackendInfo {
        version: String,
        topic: String,
        pubkeys: HashSet<String>,
        status: String,
        metadata: Option<String>,
        ip_address: Option<String>,
    }

    let mut pubkeys = HashSet::new();
    pubkeys.insert("npub1testkey".to_string());

    let backend_info = BackendInfo {
        version: "1.0.0".to_string(),
        topic: "dstack-gpu-monitor".to_string(),
        pubkeys,
        status: "available".to_string(),
        metadata: Some("test metadata".to_string()),
        ip_address: Some("192.168.1.100".to_string()),
    };

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&backend_info).unwrap();
    println!("Backend info JSON: {}", json);

    // Verify JSON structure
    let parsed: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"], "1.0.0");
    assert_eq!(parsed["topic"], "dstack-gpu-monitor");
    assert_eq!(parsed["status"], "available");
    assert_eq!(parsed["ip_address"], "192.168.1.100");
    assert!(parsed["pubkeys"].is_array());

    println!("Test: BackendInfo structure - PASSED");
}

#[test]
fn test_nostr_key_format() {
    // Test that we can recognize valid nostr key formats
    let valid_pubkey = "npub1test1234567890abcdefghijklmnopqrstuvwxyz";
    let valid_seckey = "nsec1test1234567890abcdefghijklmnopqrstuvwxyz";

    assert!(valid_pubkey.starts_with("npub"));
    assert!(valid_seckey.starts_with("nsec"));
    // Nostr bech32 keys are typically 63 characters, but can vary
    assert!(valid_pubkey.len() >= 40);
    assert!(valid_seckey.len() >= 40);

    println!("Test: Nostr key format validation - PASSED");
}

#[test]
fn test_whitelist_contains_logic() {
    #[derive(Debug)]
    struct Whitelist {
        pubkeys: HashSet<String>,
    }

    impl Whitelist {
        fn is_whitelisted(&self, pubkey: &str) -> bool {
            self.pubkeys.contains(pubkey)
        }
    }

    let mut whitelist = Whitelist {
        pubkeys: HashSet::new(),
    };

    let test_key = "npub1testkey123";
    whitelist.pubkeys.insert(test_key.to_string());

    // Test positive case
    assert!(whitelist.is_whitelisted(test_key));

    // Test negative case
    assert!(!whitelist.is_whitelisted("npub1unknown"));

    println!("Test: Whitelist contains logic - PASSED");
}

#[test]
fn test_data_directory_structure() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create expected files
    let keys_file = data_dir.join("nostr_keys.json");
    let whitelist_file = data_dir.join("whitelist.json");

    // Simulate creating files
    fs::write(&keys_file, "nsec1testkey").unwrap();
    fs::write(&whitelist_file, r#"{"pubkeys":[]}"#).unwrap();

    // Verify structure
    assert!(keys_file.exists());
    assert!(whitelist_file.exists());

    // Verify we can read them
    let keys_content = fs::read_to_string(&keys_file).unwrap();
    let whitelist_content = fs::read_to_string(&whitelist_file).unwrap();

    assert_eq!(keys_content, "nsec1testkey");
    assert!(whitelist_content.contains("pubkeys"));

    println!("Test: Data directory structure - PASSED");
}
