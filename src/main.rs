use alloy::primitives::Address;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use enum_tools::EnumTools;
use local_ip_address::local_ip;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, Uri as UnixUri};
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;

#[derive(Debug, Serialize, Deserialize)]
pub struct BackendInfo {
    pub version: String,
    pub topic: String,
    pub pubkeys: HashSet<String>,
    pub status: DephyWorkerRespondedStatus,
    pub metadata: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, EnumTools)]
#[enum_tools(Debug, Display, FromStr, TryFrom, Into)]
#[repr(i32)]
pub enum DephyWorkerRespondedStatus {
    Available = 1,
    Unavailable = 2,
}

#[derive(Debug, Deserialize)]
struct GpuInfo {
    slot: String,
    product_id: String,
    description: String,
    is_free: bool,
}

#[derive(Debug, Deserialize)]
struct DStackResponse {
    gpus: Vec<GpuInfo>,
    allow_attach_all: bool,
}

#[derive(Clone)]
enum DStackConnection {
    Http { url: String, client: reqwest::Client },
    UnixSocket { socket_path: String, client: Client<hyperlocal::UnixConnector, Empty<Bytes>> },
}

#[derive(Clone)]
struct AppState {
    connection: DStackConnection,
    nostr_pubkey: String,
    local_ip: Option<String>,
}

#[derive(Debug, Serialize)]
struct WorkerRegistration {
    pubkey: String,
    owner: String,
    node_type: String,
}

#[derive(Debug, Deserialize)]
struct PermissionResponse {
    write: PermissionDetail,
}

#[derive(Debug, Deserialize)]
struct PermissionDetail {
    mode: String,
}

async fn fetch_dstack_data(connection: &DStackConnection) -> Result<DStackResponse, String> {
    match connection {
        DStackConnection::Http { url, client } => {
            let full_url = format!("{}/prpc/ListGpus?json", url);
            info!("Checking dstack health via HTTP at: {}", full_url);

            let response = client.get(&full_url).send().await
                .map_err(|e| format!("HTTP request failed: {}", e))?;

            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()));
            }

            response.json::<DStackResponse>().await
                .map_err(|e| format!("Failed to parse JSON: {}", e))
        }
        DStackConnection::UnixSocket { socket_path, client } => {
            info!("Checking dstack health via Unix socket at: {}", socket_path);

            let uri: hyper::Uri = UnixUri::new(socket_path, "/prpc/ListGpus?json").into();
            let req = Request::builder()
                .uri(uri)
                .header("Host", "127.0.0.1")
                .body(Empty::<Bytes>::new())
                .map_err(|e| format!("Failed to build request: {}", e))?;

            let response = client.request(req).await
                .map_err(|e| format!("Unix socket request failed: {}", e))?;

            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()));
            }

            let body_bytes = response.into_body().collect().await
                .map_err(|e| format!("Failed to read response body: {}", e))?
                .to_bytes();

            serde_json::from_slice(&body_bytes)
                .map_err(|e| format!("Failed to parse JSON: {}", e))
        }
    }
}

async fn check_dstack_health(state: &AppState) -> BackendInfo {
    match fetch_dstack_data(&state.connection).await {
        Ok(dstack_data) => {
            let metadata = serde_json::json!({
                "gpu_count": dstack_data.gpus.len(),
                "gpus": dstack_data.gpus.iter().map(|gpu| {
                    serde_json::json!({
                        "slot": gpu.slot,
                        "product_id": gpu.product_id,
                        "description": gpu.description,
                        "is_free": gpu.is_free
                    })
                }).collect::<Vec<_>>(),
                "allow_attach_all": dstack_data.allow_attach_all
            });

            info!("DStack is available with {} GPUs", dstack_data.gpus.len());

            let mut pubkeys = HashSet::new();
            pubkeys.insert(state.nostr_pubkey.clone());

            BackendInfo {
                version: "1.0.0".to_string(),
                topic: "dstack-gpu-monitor".to_string(),
                pubkeys,
                status: DephyWorkerRespondedStatus::Available,
                metadata: Some(metadata.to_string()),
                ip_address: state.local_ip.clone(),
            }
        }
        Err(e) => {
            error!("Failed to connect to dstack: {}", e);
            let mut pubkeys = HashSet::new();
            pubkeys.insert(state.nostr_pubkey.clone());

            BackendInfo {
                version: "1.0.0".to_string(),
                topic: "dstack-gpu-monitor".to_string(),
                pubkeys,
                status: DephyWorkerRespondedStatus::Unavailable,
                metadata: Some(format!("Error: {}", e)),
                ip_address: state.local_ip.clone(),
            }
        }
    }
}

async fn health_handler(State(state): State<Arc<AppState>>) -> (StatusCode, Json<BackendInfo>) {
    let backend_info = check_dstack_health(&state).await;

    let status_code = match backend_info.status {
        DephyWorkerRespondedStatus::Available => StatusCode::OK,
        DephyWorkerRespondedStatus::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(backend_info))
}

async fn root_handler() -> &'static str {
    "DStack Backend Health Monitor"
}

fn get_local_ip() -> Option<String> {
    match local_ip() {
        Ok(ip) => {
            info!("Detected local IP: {}", ip);
            Some(ip.to_string())
        }
        Err(e) => {
            error!("Failed to get local IP: {}", e);
            None
        }
    }
}

fn load_or_create_nostr_keypair(data_dir: &PathBuf) -> Result<Keys, Box<dyn std::error::Error>> {
    let keys_file = data_dir.join("key");

    if keys_file.exists() {
        info!("Loading existing Nostr keypair from {:?}", keys_file);
        let content = fs::read_to_string(&keys_file)?;
        let keys = Keys::parse(&content)?;
        Ok(keys)
    } else {
        info!("Generating new Nostr keypair");
        let keys = Keys::generate();

        // Create data directory if it doesn't exist
        fs::create_dir_all(data_dir)?;

        // Save the secret key
        let secret_key = keys.secret_key().to_secret_hex();
        fs::write(&keys_file, secret_key)?;

        info!("Saved new Nostr keypair to {:?}", keys_file);
        info!("Public key: {}", keys.public_key().to_hex());

        Ok(keys)
    }
}

async fn check_worker_registered(
    client: &reqwest::Client,
    registry_url: &str,
    pubkey: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let url = format!("{}/permissions/{}", registry_url, pubkey);

    info!("Checking worker registration status at: {}", url);

    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<PermissionResponse>().await {
                    Ok(perm_response) => {
                        let is_registered = perm_response.write.mode == "AllowAll";
                        info!("Worker registration status: registered={} (mode={})", is_registered, perm_response.write.mode);
                        Ok(is_registered)
                    }
                    Err(e) => {
                        error!("Failed to parse permission response: {}", e);
                        // If we can't parse, assume not registered
                        Ok(false)
                    }
                }
            } else {
                info!("Worker not registered (status: {})", response.status());
                Ok(false)
            }
        }
        Err(e) => {
            error!("Failed to check registration status: {}", e);
            Err(Box::new(e))
        }
    }
}

async fn register_worker(
    client: &reqwest::Client,
    registry_url: &str,
    pubkey: &str,
    owner: &str,
    node_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/workers", registry_url);

    let registration = WorkerRegistration {
        pubkey: pubkey.to_string(),
        owner: owner.to_string(),
        node_type: node_type.to_string(),
    };

    info!("Registering worker at: {}", url);
    info!("Registration data: pubkey={}, owner={}, node_type={}", pubkey, owner, node_type);

    match client.post(&url).json(&registration).send().await {
        Ok(response) => {
            if response.status().is_success() {
                info!("Worker registered successfully");
                Ok(())
            } else {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                error!("Failed to register worker: status={}, error={}", status, error_text);
                Err(format!("Registration failed: {} - {}", status, error_text).into())
            }
        }
        Err(e) => {
            error!("Failed to send registration request: {}", e);
            Err(Box::new(e))
        }
    }
}

async fn ensure_worker_registered(
    client: &reqwest::Client,
    registry_url: &str,
    pubkey: &str,
    owner: &str,
    node_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Ensuring worker is registered...");

    // Check if already registered
    match check_worker_registered(client, registry_url, pubkey).await {
        Ok(is_registered) => {
            if is_registered {
                info!("Worker is already registered, skipping registration");
                return Ok(());
            } else {
                info!("Worker is not registered, proceeding with registration");
                // Continue to registration
            }
        }
        Err(e) => {
            error!("Failed to check registration status, attempting registration anyway: {}", e);
            // Continue to registration even if check fails
        }
    }

    // Register worker
    register_worker(client, registry_url, pubkey, owner, node_type).await
}


#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dstack_backend=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Get configuration from environment variables or use defaults
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let dstack_url_config = std::env::var("DSTACK_URL")
        .or_else(|_| std::env::var("DSTACK_BACKEND_DSTACK_URL"))
        .unwrap_or_else(|_| "http://localhost:19060".to_string());
    let dstack_url_config = dstack_url_config.trim().to_string();
    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));

    // Worker registration configuration (required)
    let registry_url = std::env::var("REGISTRY_URL")
        .expect("REGISTRY_URL environment variable is required for worker registration");
    let owner_address_str = std::env::var("OWNER_ADDRESS")
        .expect("OWNER_ADDRESS environment variable is required for worker registration");

    // Parse the owner address using alloy to ensure correct format
    let owner_address: Address = owner_address_str
        .parse()
        .expect("OWNER_ADDRESS must be a valid Ethereum address");
    let owner_address_formatted = owner_address.to_string();

    let node_type = std::env::var("NODE_TYPE").unwrap_or_else(|_| "node-H100x1".to_string());

    info!("Starting DStack Backend Monitor");
    info!("Listen address: {}", listen_addr);
    info!("DStack URL config: {}", dstack_url_config);
    info!("Data directory: {:?}", data_dir);
    info!("Registry URL: {}", registry_url);
    info!("Owner address: {}", owner_address_formatted);
    info!("Node type: {}", node_type);

    // Parse DSTACK_URL to determine connection type
    let connection = if dstack_url_config.starts_with("unix://") {
        let socket_path = dstack_url_config.strip_prefix("unix://").unwrap().to_string();
        info!("Using Unix socket connection: {}", socket_path);
        DStackConnection::UnixSocket {
            socket_path,
            client: Client::unix(),
        }
    } else {
        info!("Using HTTP connection: {}", dstack_url_config);
        DStackConnection::Http {
            url: dstack_url_config,
            client: reqwest::Client::new(),
        }
    };

    // Get local IP address
    let local_ip = get_local_ip();

    // Load or create Nostr keypair
    let keys = load_or_create_nostr_keypair(&data_dir)
        .expect("Failed to load or create Nostr keypair");

    let nostr_pubkey = keys
        .public_key()
        .to_hex();

    info!("Nostr public key: {}", nostr_pubkey);

    // Create HTTP client for registration
    let http_client = reqwest::Client::new();

    // Register worker (required for communication with message network)
    info!("Worker registration is required to communicate with the message network");
    match ensure_worker_registered(
        &http_client,
        &registry_url,
        &nostr_pubkey,
        &owner_address_formatted,
        &node_type,
    )
    .await
    {
        Ok(_) => {
            info!("Worker registration completed successfully");
            info!("Worker is now authorized to communicate with the message network");
        }
        Err(e) => {
            error!("Worker registration failed: {}", e);
            error!("Cannot start service without successful registration");
            error!("Worker must be registered to communicate with the message network");
            std::process::exit(1);
        }
    }

    // Create shared state
    let state = Arc::new(AppState {
        connection,
        nostr_pubkey,
        local_ip,
    });

    // Build application
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Parse the listen address
    let addr: SocketAddr = listen_addr.parse().expect("Invalid listen address");

    info!("Backend listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
