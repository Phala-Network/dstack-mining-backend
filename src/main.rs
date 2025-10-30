use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Placeholder for PublicKey type
type PublicKey = String;

#[derive(Debug, Serialize, Deserialize)]
pub struct BackendInfo {
    pub version: String,
    pub topic: String,
    pub pubkeys: HashSet<PublicKey>,
    pub status: DephyWorkerRespondedStatus,
    pub metadata: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DephyWorkerRespondedStatus {
    Available = 1,
    Unavailable = 2,
}

impl std::fmt::Display for DephyWorkerRespondedStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DephyWorkerRespondedStatus::Available => write!(f, "available"),
            DephyWorkerRespondedStatus::Unavailable => write!(f, "unavailable"),
        }
    }
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

                        BackendInfo {
                            version: "1.0.0".to_string(),
                            topic: "dstack-gpu-monitor".to_string(),
                            pubkeys: HashSet::new(),
                            status: DephyWorkerRespondedStatus::Available,
                            metadata: Some(metadata.to_string()),
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse dstack response: {}", e);
                        BackendInfo {
                            version: "1.0.0".to_string(),
                            topic: "dstack-gpu-monitor".to_string(),
                            pubkeys: HashSet::new(),
                            status: DephyWorkerRespondedStatus::Unavailable,
                            metadata: Some(format!("Parse error: {}", e)),
                        }
                    }
                }
            } else {
                error!("DStack returned error status: {}", response.status());
                BackendInfo {
                    version: "1.0.0".to_string(),
                    topic: "dstack-gpu-monitor".to_string(),
                    pubkeys: HashSet::new(),
                    status: DephyWorkerRespondedStatus::Unavailable,
                    metadata: Some(format!("HTTP error: {}", response.status())),
                }
            }
        }
        Err(e) => {
            error!("Failed to connect to dstack: {}", e);
            BackendInfo {
                version: "1.0.0".to_string(),
                topic: "dstack-gpu-monitor".to_string(),
                pubkeys: HashSet::new(),
                status: DephyWorkerRespondedStatus::Unavailable,
                metadata: Some(format!("Connection error: {}", e)),
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

    info!("Starting DStack Backend Monitor");
    info!("Listen address: {}", listen_addr);
    info!("DStack URL: {}", dstack_url);

    // Create shared state
    let state = Arc::new(AppState {
        dstack_url,
        client: reqwest::Client::new(),
    });

    // Build our application with routes
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Parse the listen address
    let addr: SocketAddr = listen_addr.parse().expect("Invalid listen address");

    info!("Listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
