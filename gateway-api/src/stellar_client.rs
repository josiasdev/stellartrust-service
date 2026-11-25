use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use utoipa::ToSchema;

use crate::resilience::{call_with_resilience, CircuitBreaker, ResilienceError};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct AttestationRecordDto {
    pub issuer: String,
    pub doc_hash: String,
    pub domain: String,
    pub timestamp: u64,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct AttestationTxMetadata {
    pub issuer: String,
    pub doc_hash: String,
    pub domain: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StellarConfig {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub contract_id: String,
    pub source_account: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StellarClientError {
    #[error("invalid doc_hash: expected 64 hex chars")]
    InvalidDocHash,
    #[error("stellar cli error: {0}")]
    Cli(String),
    #[error("stellar rpc/transient error: {0}")]
    Transient(String),
    #[error("record not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("already registered: {0}")]
    AlreadyRegistered(String),
    #[error("already revoked: {0}")]
    AlreadyRevoked(String),
    #[error("unexpected output: {0}")]
    UnexpectedOutput(String),
}

impl StellarClientError {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

#[async_trait]
pub trait StellarGateway: Send + Sync {
    async fn register(
        &self,
        issuer: &str,
        doc_hash: &str,
        domain: &str,
    ) -> Result<AttestationTxMetadata, StellarClientError>;
    async fn verify(
        &self,
        doc_hash: &str,
    ) -> Result<Option<AttestationRecordDto>, StellarClientError>;
    async fn revoke(&self, issuer: &str, doc_hash: &str) -> Result<(), StellarClientError>;
}

#[derive(Clone)]
pub struct StellarCliClient {
    config: StellarConfig,
    breaker: CircuitBreaker,
    max_attempts: u32,
    base_delay: Duration,
}

impl StellarCliClient {
    pub fn new(
        config: StellarConfig,
        breaker: CircuitBreaker,
        max_attempts: u32,
        base_delay: Duration,
    ) -> Self {
        Self {
            config,
            breaker,
            max_attempts,
            base_delay,
        }
    }

    fn normalize_doc_hash(doc_hash: &str) -> Result<String, StellarClientError> {
        let h = doc_hash.trim().trim_start_matches("0x").to_lowercase();
        if h.len() != 64 || hex::decode(&h).is_err() {
            return Err(StellarClientError::InvalidDocHash);
        }
        Ok(h)
    }

    fn extract_tx_hash(output: &str) -> Option<String> {
        for token in output.split_whitespace() {
            let cleaned = token.trim_matches(|c: char| !c.is_ascii_hexdigit());
            if cleaned.len() == 64 && hex::decode(cleaned).is_ok() {
                return Some(cleaned.to_lowercase());
            }
        }
        None
    }

    async fn run_cli(&self, args: &[String]) -> Result<String, StellarClientError> {
        let output = Command::new("stellar")
            .args(args)
            .output()
            .await
            .map_err(|e| {
                StellarClientError::Transient(format!("failed to execute stellar CLI: {e}"))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if output.status.success() {
            return Ok(stdout);
        }

        let msg = if stderr.is_empty() { stdout } else { stderr };
        let lower = msg.to_lowercase();
        if lower.contains("alreadyregistered") || lower.contains("already registered") {
            return Err(StellarClientError::AlreadyRegistered(msg));
        }
        if lower.contains("alreadyrevoked") || lower.contains("already revoked") {
            return Err(StellarClientError::AlreadyRevoked(msg));
        }
        if lower.contains("unauthorized") {
            return Err(StellarClientError::Unauthorized(msg));
        }
        if lower.contains("notfound") || lower.contains("not found") {
            return Err(StellarClientError::NotFound(msg));
        }
        if lower.contains("timeout")
            || lower.contains("connection")
            || lower.contains("rpc")
            || lower.contains("unavailable")
            || lower.contains("503")
            || lower.contains("502")
        {
            return Err(StellarClientError::Transient(msg));
        }
        Err(StellarClientError::Cli(msg))
    }

    fn base_args(&self) -> Vec<String> {
        vec![
            "contract".into(),
            "invoke".into(),
            "--id".into(),
            self.config.contract_id.clone(),
            "--source-account".into(),
            self.config.source_account.clone(),
            "--rpc-url".into(),
            self.config.rpc_url.clone(),
            "--network-passphrase".into(),
            self.config.network_passphrase.clone(),
            "--".into(),
        ]
    }

    async fn invoke(&self, args: Vec<String>) -> Result<String, StellarClientError> {
        let breaker = self.breaker.clone();
        let max_attempts = self.max_attempts;
        let base_delay = self.base_delay;
        let res = call_with_resilience(
            &breaker,
            max_attempts,
            base_delay,
            || {
                let this = self.clone();
                let a = args.clone();
                async move { this.run_cli(&a).await }
            },
            |e| e.is_transient(),
        )
        .await;
        match res {
            Ok(v) => Ok(v),
            Err(ResilienceError::CircuitOpen) => {
                Err(StellarClientError::Transient("circuit breaker open".into()))
            }
            Err(ResilienceError::Inner(e)) => Err(e),
        }
    }
}

#[async_trait]
impl StellarGateway for StellarCliClient {
    async fn register(
        &self,
        issuer: &str,
        doc_hash: &str,
        domain: &str,
    ) -> Result<AttestationTxMetadata, StellarClientError> {
        let hash = Self::normalize_doc_hash(doc_hash)?;
        let mut args = self.base_args();
        args.extend([
            "register".into(),
            "--issuer".into(),
            issuer.into(),
            "--doc_hash".into(),
            hash.clone(),
            "--domain".into(),
            domain.into(),
        ]);
        let out = self.invoke(args).await?;
        let tx_hash = Self::extract_tx_hash(&out);
        Ok(AttestationTxMetadata {
            issuer: issuer.to_string(),
            doc_hash: hash,
            domain: domain.to_string(),
            status: "submitted".to_string(),
            tx_hash,
        })
    }

    async fn verify(
        &self,
        doc_hash: &str,
    ) -> Result<Option<AttestationRecordDto>, StellarClientError> {
        let hash = Self::normalize_doc_hash(doc_hash)?;
        let args = vec![
            "contract".into(),
            "invoke".into(),
            "--id".into(),
            self.config.contract_id.clone(),
            "--source-account".into(),
            self.config.source_account.clone(),
            "--rpc-url".into(),
            self.config.rpc_url.clone(),
            "--network-passphrase".into(),
            self.config.network_passphrase.clone(),
            "--send=no".into(),
            "--".into(),
            "verify".into(),
            "--doc_hash".into(),
            hash,
        ];
        let out = self.invoke(args).await?;
        let out = out.trim();
        if out.is_empty() || out == "null" {
            return Ok(None);
        }
        // CLI may return JSON object or a Soroban-JSON representation.
        // Try direct parse as DTO first, then as a generic serde_json::Value to map.
        if let Ok(dto) = serde_json::from_str::<AttestationRecordDto>(out) {
            return Ok(Some(dto));
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(out) {
            if value.is_null() {
                return Ok(None);
            }
            if let Some(obj) = value.as_object() {
                let dto = AttestationRecordDto {
                    issuer: obj
                        .get("issuer")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    doc_hash: obj
                        .get("doc_hash")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    domain: obj
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    timestamp: obj
                        .get("timestamp")
                        .and_then(|v| v.as_u64())
                        .unwrap_or_default(),
                    revoked: obj
                        .get("revoked")
                        .and_then(|v| v.as_bool())
                        .unwrap_or_default(),
                };
                return Ok(Some(dto));
            }
        }
        Err(StellarClientError::UnexpectedOutput(out.to_string()))
    }

    async fn revoke(&self, issuer: &str, doc_hash: &str) -> Result<(), StellarClientError> {
        let hash = Self::normalize_doc_hash(doc_hash)?;
        let mut args = self.base_args();
        args.extend([
            "revoke".into(),
            "--issuer".into(),
            issuer.into(),
            "--doc_hash".into(),
            hash,
        ]);
        self.invoke(args).await?;
        Ok(())
    }
}
