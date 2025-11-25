use alloy::primitives::Address;
use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use enum_tools::EnumTools;
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyperlocal::{UnixClientExt, Uri as UnixUri};
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
enum DStackConnection {
    Http {
        url: String,
        client: reqwest::Client,
    },
    UnixSocket {
        socket_path: String,
        client: Client<hyperlocal::UnixConnector, Empty<Bytes>>,
    },
}

#[derive(Clone)]
struct AppState {
    connection: DStackConnection,
    nostr_pubkey: String,
    local_ip: Option<String>,
}

async fn fetch_dstack_data(connection: &DStackConnection) -> Result<DStackResponse, String> {
    match connection {
        DStackConnection::Http { url, client } => {
            let full_url = format!("{}/prpc/ListGpus?json", url);
            info!("Checking dstack health via HTTP at: {}", full_url);

            let response = client
                .get(&full_url)
                .send()
                .await
                .map_err(|e| format!("HTTP request failed: {}", e))?;

            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()));
            }

            response
                .json::<DStackResponse>()
                .await
                .map_err(|e| format!("Failed to parse JSON: {}", e))
        }
        DStackConnection::UnixSocket {
            socket_path,
            client,
        } => {
            info!("Checking dstack health via Unix socket at: {}", socket_path);

            let uri: hyper::Uri = UnixUri::new(socket_path, "/prpc/ListGpus?json").into();
            let req = Request::builder()
                .uri(uri)
                .header("Host", "127.0.0.1")
                .body(Empty::<Bytes>::new())
                .map_err(|e| format!("Failed to build request: {}", e))?;

            let response = client
                .request(req)
                .await
                .map_err(|e| format!("Unix socket request failed: {}", e))?;

            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()));
            }

            let body_bytes = response
                .into_body()
                .collect()
                .await
                .map_err(|e| format!("Failed to read response body: {}", e))?
                .to_bytes();

            serde_json::from_slice(&body_bytes).map_err(|e| format!("Failed to parse JSON: {}", e))
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

            info!("dstack is available with {} GPUs", dstack_data.gpus.len());

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
    "dstack Backend Health Monitor"
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

fn determine_node_type(dstack_response: &DStackResponse) -> String {
    let gpu_count = dstack_response.gpus.len();
    if gpu_count == 0 {
        return "CPU".to_string();
    }

    let first_gpu = &dstack_response.gpus[0];
    let model = if first_gpu.description.contains("H200") {
        "H200"
    } else if first_gpu.description.contains("H100") {
        "H100"
    } else if first_gpu.description.contains("B200") {
        "B200"
    } else {
        return "Unknown".to_string();
    };

    format!("node-{}x{}", model, gpu_count)
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
    let data_dir =
        PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));

    let owner_address_str = std::env::var("OWNER_ADDRESS")
        .expect("OWNER_ADDRESS environment variable is required for worker registration");

    // Parse the owner address using alloy to ensure correct format
    let owner_address: Address = owner_address_str
        .parse()
        .expect("OWNER_ADDRESS must be a valid Ethereum address");
    let owner_address_formatted = owner_address.to_string();

    info!("Starting dstack Backend Monitor");
    info!("Listen address: {}", listen_addr);
    info!("dstack URL config: {}", dstack_url_config);
    info!("Data directory: {:?}", data_dir);

    info!("Owner address: {}", owner_address_formatted);

    // Parse DSTACK_URL to determine connection type
    let connection = if dstack_url_config.starts_with("unix://") {
        let socket_path = dstack_url_config
            .strip_prefix("unix://")
            .unwrap()
            .to_string();
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
    let keys =
        load_or_create_nostr_keypair(&data_dir).expect("Failed to load or create Nostr keypair");

    let nostr_pubkey = keys.public_key().to_hex();

    info!("Nostr public key: {}", nostr_pubkey);

    // Fetch dstack data to determine node type
    let mut node_type = "Unknown".to_string();
    info!("Connecting to dstack to determine node type...");

    // Simple retry loop for dstack connection
    for i in 0..5 {
        match fetch_dstack_data(&connection).await {
            Ok(data) => {
                node_type = determine_node_type(&data);
                info!("Successfully determined node type: {}", node_type);
                break;
            }
            Err(e) => {
                error!("Failed to fetch dstack data (attempt {}/5): {}", i + 1, e);
                if i < 4 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    if node_type == "Unknown" {
        error!("Could not determine node type from dstack. Defaulting to 'Unknown'.");
        error!("Please ensure dstack is running and accessible.");
    }

    // Log registration information for manual registration
    info!("==================================================================");
    info!("MANUAL REGISTRATION REQUIRED");
    info!("Please provide the following information to the administrator:");
    info!("Nostr Public Key: {}", nostr_pubkey);
    info!("Owner Address:    {}", owner_address_formatted);
    info!("Node Type:        {}", node_type);
    info!("==================================================================");

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
