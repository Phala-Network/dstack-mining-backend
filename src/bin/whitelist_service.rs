use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Whitelist {
    pub pubkeys: HashSet<String>,
}

#[derive(Debug, Deserialize)]
pub struct WhitelistQuery {
    pub pubkey: String,
}

#[derive(Debug, Serialize)]
pub struct WhitelistResponse {
    pub is_whitelisted: bool,
    pub pubkey: String,
}

#[derive(Clone)]
struct AppState {
    whitelist: Arc<Whitelist>,
}

async fn root_handler() -> &'static str {
    "Whitelist Service - Centralized Pubkey Verification"
}

async fn whitelist_handler(
    Query(query): Query<WhitelistQuery>,
    State(state): State<Arc<AppState>>,
) -> Json<WhitelistResponse> {
    let is_whitelisted = state.whitelist.pubkeys.contains(&query.pubkey);

    if is_whitelisted {
        info!("Pubkey {} is whitelisted", query.pubkey);
    } else {
        info!("Pubkey {} is NOT whitelisted", query.pubkey);
    }

    Json(WhitelistResponse {
        is_whitelisted,
        pubkey: query.pubkey,
    })
}

async fn list_handler(State(state): State<Arc<AppState>>) -> Json<Whitelist> {
    Json((*state.whitelist).clone())
}

async fn health_handler() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}

fn load_whitelist(whitelist_file: &PathBuf) -> Result<Whitelist, Box<dyn std::error::Error>> {
    if whitelist_file.exists() {
        info!("Loading whitelist from {:?}", whitelist_file);
        let content = fs::read_to_string(whitelist_file)?;
        let whitelist: Whitelist = serde_json::from_str(&content)?;
        info!("Loaded {} pubkeys from whitelist", whitelist.pubkeys.len());
        Ok(whitelist)
    } else {
        error!("Whitelist file not found at {:?}", whitelist_file);
        info!("Creating empty whitelist");

        let whitelist = Whitelist {
            pubkeys: HashSet::new(),
        };

        // Create parent directory if needed
        if let Some(parent) = whitelist_file.parent() {
            fs::create_dir_all(parent)?;
        }

        // Save empty whitelist
        let content = serde_json::to_string_pretty(&whitelist)?;
        fs::write(whitelist_file, content)?;

        Ok(whitelist)
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "whitelist_service=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Get configuration from environment variables or use defaults
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8082".to_string());
    let whitelist_file = PathBuf::from(
        std::env::var("WHITELIST_FILE").unwrap_or_else(|_| "./whitelist.json".to_string()),
    );

    info!("Starting Whitelist Service");
    info!("Listen address: {}", listen_addr);
    info!("Whitelist file: {:?}", whitelist_file);

    // Load whitelist
    let whitelist = load_whitelist(&whitelist_file).expect("Failed to load whitelist");

    info!("Whitelist loaded with {} pubkeys", whitelist.pubkeys.len());

    // Create shared state
    let state = Arc::new(AppState {
        whitelist: Arc::new(whitelist),
    });

    // Build application
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/api/whitelist", get(whitelist_handler))
        .route("/api/list", get(list_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Parse the listen address
    let addr: SocketAddr = listen_addr.parse().expect("Invalid listen address");

    info!("Whitelist service listening on {}", addr);

    // Run the server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
