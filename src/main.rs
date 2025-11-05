use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
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
struct AppState {
    dstack_url: String,
    client: reqwest::Client,
    nostr_pubkey: String,
    local_ip: Option<String>,
}

async fn check_dstack_health(state: &AppState) -> BackendInfo {
    let url = format!("{}/prpc/ListGpus?json", state.dstack_url);

    info!("Checking dstack health at: {}", url);

    match state.client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<DStackResponse>().await {
                    Ok(dstack_data) => {
                        let metadata = serde_json::json!({
                            "gpu_count": dstack_data.gpus.len(),
                            "gpus": dstack_data.gpus.iter().map(|gpu| {
                                serde_json::json!({
                                    "slot": gpu.slot,
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
                        error!("Failed to parse dstack response: {}", e);
                        let mut pubkeys = HashSet::new();
                        pubkeys.insert(state.nostr_pubkey.clone());

                        BackendInfo {
                            version: "1.0.0".to_string(),
                            topic: "dstack-gpu-monitor".to_string(),
                            pubkeys,
                            status: DephyWorkerRespondedStatus::Unavailable,
                            metadata: Some(format!("Parse error: {}", e)),
                            ip_address: state.local_ip.clone(),
                        }
                    }
                }
            } else {
                error!("DStack returned error status: {}", response.status());
                let mut pubkeys = HashSet::new();
                pubkeys.insert(state.nostr_pubkey.clone());

                BackendInfo {
                    version: "1.0.0".to_string(),
                    topic: "dstack-gpu-monitor".to_string(),
                    pubkeys,
                    status: DephyWorkerRespondedStatus::Unavailable,
                    metadata: Some(format!("HTTP error: {}", response.status())),
                    ip_address: state.local_ip.clone(),
                }
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
                metadata: Some(format!("Connection error: {}", e)),
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
        let secret_key = keys.secret_key().to_bech32()?;
        fs::write(&keys_file, secret_key)?;

        info!("Saved new Nostr keypair to {:?}", keys_file);
        info!("Public key: {}", keys.public_key().to_bech32()?);

        Ok(keys)
    }
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
    let dstack_url = std::env::var("DSTACK_URL").unwrap_or_else(|_| "http://localhost:19060".to_string());
    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));

    info!("Starting DStack Backend Monitor");
    info!("Listen address: {}", listen_addr);
    info!("DStack URL: {}", dstack_url);
    info!("Data directory: {:?}", data_dir);

    // Get local IP address
    let local_ip = get_local_ip();

    // Load or create Nostr keypair
    let keys = load_or_create_nostr_keypair(&data_dir)
        .expect("Failed to load or create Nostr keypair");

    let nostr_pubkey = keys
        .public_key()
        .to_bech32()
        .expect("Failed to convert public key to bech32");

    info!("Nostr public key: {}", nostr_pubkey);

    // Create shared state
    let state = Arc::new(AppState {
        dstack_url,
        client: reqwest::Client::new(),
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
