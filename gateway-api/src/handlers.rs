use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::resilience::CircuitState;
use crate::stellar_client::{AttestationRecordDto, StellarClientError, StellarGateway};

#[derive(Clone)]
pub struct AppState {
    pub gateway: Arc<dyn StellarGateway>,
    pub breaker: crate::resilience::CircuitBreaker,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterAttestationRequest {
    pub issuer: String,
    pub doc_hash: String,
    pub domain: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RevokeAttestationRequest {
    pub issuer: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub circuit_breaker: CircuitState,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(register_attestation, verify_attestation, revoke_attestation, health),
    components(schemas(RegisterAttestationRequest, RevokeAttestationRequest, HealthResponse, ErrorResponse, AttestationRecordDto, CircuitState)),
    tags((name = "stellartrust", description = "StellarTrust Gateway API"))
)]
pub struct ApiDoc;

fn map_error(err: StellarClientError) -> (StatusCode, Json<ErrorResponse>) {
    let (status, msg) = match &err {
        StellarClientError::InvalidDocHash => (StatusCode::BAD_REQUEST, err.to_string()),
        StellarClientError::AlreadyRegistered(_) => (StatusCode::CONFLICT, err.to_string()),
        StellarClientError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        StellarClientError::Unauthorized(_) => (StatusCode::FORBIDDEN, err.to_string()),
        StellarClientError::Transient(_) => (StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
        StellarClientError::Cli(_) | StellarClientError::UnexpectedOutput(_) => {
            (StatusCode::BAD_GATEWAY, err.to_string())
        }
    };
    (status, Json(ErrorResponse { error: msg }))
}

#[utoipa::path(
    post,
    path = "/api/v1/attestations",
    request_body = RegisterAttestationRequest,
    responses(
        (status = 201, description = "Attestation registered"),
        (status = 400, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
        (status = 503, body = ErrorResponse),
    ),
    tag = "stellartrust"
)]
pub async fn register_attestation(
    State(state): State<AppState>,
    Json(payload): Json<RegisterAttestationRequest>,
) -> impl IntoResponse {
    match state
        .gateway
        .register(&payload.issuer, &payload.doc_hash, &payload.domain)
        .await
    {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(err) => map_error(err).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/verify/{doc_hash}",
    params(("doc_hash" = String, Path, description = "SHA-256 hash hex")),
    responses(
        (status = 200, body = AttestationRecordDto),
        (status = 404, body = ErrorResponse),
        (status = 400, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
        (status = 503, body = ErrorResponse),
    ),
    tag = "stellartrust"
)]
pub async fn verify_attestation(
    State(state): State<AppState>,
    Path(doc_hash): Path<String>,
) -> impl IntoResponse {
    match state.gateway.verify(&doc_hash).await {
        Ok(Some(record)) => (StatusCode::OK, Json(record)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "attestation not found".to_string(),
            }),
        )
            .into_response(),
        Err(err) => map_error(err).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/attestations/{doc_hash}/revoke",
    params(("doc_hash" = String, Path, description = "SHA-256 hash hex")),
    request_body = RevokeAttestationRequest,
    responses(
        (status = 200, description = "Attestation revoked"),
        (status = 400, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 502, body = ErrorResponse),
        (status = 503, body = ErrorResponse),
    ),
    tag = "stellartrust"
)]
pub async fn revoke_attestation(
    State(state): State<AppState>,
    Path(doc_hash): Path<String>,
    Json(payload): Json<RevokeAttestationRequest>,
) -> impl IntoResponse {
    match state.gateway.revoke(&payload.issuer, &doc_hash).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(err) => map_error(err).into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, body = HealthResponse)),
    tag = "stellartrust"
)]
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        circuit_breaker: state.breaker.state(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use std::sync::Arc;
    use tower::ServiceExt;

    struct MockGateway;

    #[async_trait::async_trait]
    impl StellarGateway for MockGateway {
        async fn register(&self, _issuer: &str, _doc_hash: &str, _domain: &str) -> Result<(), StellarClientError> {
            Ok(())
        }
        async fn verify(&self, doc_hash: &str) -> Result<Option<AttestationRecordDto>, StellarClientError> {
            if doc_hash.starts_with("00") {
                return Ok(None);
            }
            Ok(Some(AttestationRecordDto {
                issuer: "GISSUER".into(),
                doc_hash: doc_hash.into(),
                domain: "ufc-certificates".into(),
                timestamp: 123,
                revoked: false,
            }))
        }
        async fn revoke(&self, _issuer: &str, _doc_hash: &str) -> Result<(), StellarClientError> {
            Ok(())
        }
    }

    fn app() -> Router {
        let breaker = crate::resilience::CircuitBreaker::new(5, std::time::Duration::from_secs(30));
        let state = AppState { gateway: Arc::new(MockGateway), breaker };
        Router::new()
            .route("/api/v1/attestations", post(register_attestation))
            .route("/api/v1/verify/{doc_hash}", get(verify_attestation))
            .route("/api/v1/attestations/{doc_hash}/revoke", post(revoke_attestation))
            .route("/health", get(health))
            .with_state(state)
    }

    #[tokio::test]
    async fn health_returns_closed_breaker() {
        let res = app().oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["circuit_breaker"], "Closed");
    }

    #[tokio::test]
    async fn verify_returns_404_when_missing() {
        let hash = "00".repeat(32);
        let res = app().oneshot(Request::builder().uri(format!("/api/v1/verify/{hash}")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn register_returns_201() {
        let hash = "11".repeat(32);
        let body = serde_json::json!({"issuer":"GISSUER","doc_hash":hash,"domain":"ufc-certificates"});
        let res = app().oneshot(
            Request::builder().method("POST").uri("/api/v1/attestations")
                .header("content-type","application/json")
                .body(Body::from(body.to_string())).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }
}
