mod handlers;
mod resilience;
mod stellar_client;

use std::sync::Arc;
use std::time::Duration;

use axum::routing::{get, post};
use axum::Router;
use dotenvy::dotenv;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{health, register_attestation, revoke_attestation, verify_attestation, ApiDoc, AppState};
use crate::resilience::CircuitBreaker;
use crate::stellar_client::{StellarCliClient, StellarConfig};

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gateway_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = StellarConfig {
        rpc_url: std::env::var("STELLAR_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string()),
        network_passphrase: std::env::var("STELLAR_NETWORK_PASSPHRASE")
            .unwrap_or_else(|_| "Test SDF Network ; September 2015".to_string()),
        contract_id: std::env::var("STELLAR_CONTRACT_ID").unwrap_or_default(),
        source_account: std::env::var("STELLAR_SOURCE_ACCOUNT").unwrap_or_default(),
    };

    let breaker = CircuitBreaker::new(
        env_u32("CB_FAILURE_THRESHOLD", 5),
        Duration::from_secs(env_u64("CB_OPEN_TIMEOUT_SECS", 30)),
    );
    let client = StellarCliClient::new(
        config,
        breaker.clone(),
        env_u32("RETRY_MAX_ATTEMPTS", 3),
        Duration::from_millis(env_u64("RETRY_BASE_DELAY_MS", 500)),
    );

    let state = AppState {
        gateway: Arc::new(client),
        breaker,
    };

    let app = Router::new()
        .route("/api/v1/attestations", post(register_attestation))
        .route("/api/v1/verify/{doc_hash}", get(verify_attestation))
        .route(
            "/api/v1/attestations/{doc_hash}/revoke",
            post(revoke_attestation),
        )
        .route("/health", get(health))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let bind = std::env::var("GATEWAY_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("failed to bind gateway listener");
    tracing::info!("gateway-api listening on {bind}");
    axum::serve(listener, app)
        .await
        .expect("gateway server error");
}
