use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
struct CircuitBreakerInner {
    state: CircuitState,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    probe_in_flight: bool,
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    inner: Arc<Mutex<CircuitBreakerInner>>,
    failure_threshold: u32,
    open_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, open_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CircuitBreakerInner {
                state: CircuitState::Closed,
                consecutive_failures: 0,
                opened_at: None,
                probe_in_flight: false,
            })),
            failure_threshold: failure_threshold.max(1),
            open_timeout,
        }
    }

    pub fn state(&self) -> CircuitState {
        let mut inner = self.inner.lock().expect("circuit breaker lock poisoned");
        self.maybe_half_open(&mut inner);
        inner.state
    }

    pub fn can_execute(&self) -> bool {
        let mut inner = self.inner.lock().expect("circuit breaker lock poisoned");
        self.maybe_half_open(&mut inner);
        match inner.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                if inner.probe_in_flight {
                    return false;
                }
                inner.probe_in_flight = true;
                true
            }
        }
    }

    pub fn record_success(&self) {
        let mut inner = self.inner.lock().expect("circuit breaker lock poisoned");
        inner.state = CircuitState::Closed;
        inner.consecutive_failures = 0;
        inner.opened_at = None;
        inner.probe_in_flight = false;
    }

    pub fn record_failure(&self) {
        let mut inner = self.inner.lock().expect("circuit breaker lock poisoned");
        inner.consecutive_failures = inner.consecutive_failures.saturating_add(1);
        match inner.state {
            CircuitState::HalfOpen => self.open(&mut inner),
            CircuitState::Closed if inner.consecutive_failures >= self.failure_threshold => {
                self.open(&mut inner)
            }
            _ => {}
        }
    }

    fn maybe_half_open(&self, inner: &mut CircuitBreakerInner) {
        if inner.state == CircuitState::Open {
            if let Some(opened_at) = inner.opened_at {
                if opened_at.elapsed() >= self.open_timeout {
                    inner.state = CircuitState::HalfOpen;
                }
            }
        }
    }

    fn open(&self, inner: &mut CircuitBreakerInner) {
        inner.state = CircuitState::Open;
        inner.opened_at = Some(Instant::now());
        inner.probe_in_flight = false;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResilienceError<E> {
    #[error("circuit breaker is open")]
    CircuitOpen,
    #[error(transparent)]
    Inner(E),
}

pub async fn retry_with_backoff<T, E, F, Fut, P>(
    max_attempts: u32,
    base_delay: Duration,
    mut operation: F,
    mut is_transient: P,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    P: FnMut(&E) -> bool,
{
    let attempts = max_attempts.max(1);
    let mut delay = base_delay;
    let mut last_err: Option<E> = None;
    for attempt in 0..attempts {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                let transient = is_transient(&err);
                last_err = Some(err);
                if !transient || attempt + 1 == attempts {
                    break;
                }
                let jitter_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_millis() as u64
                    % (delay.as_millis() as u64 / 2 + 1);
                tokio::time::sleep(delay + Duration::from_millis(jitter_ms)).await;
                delay = delay.saturating_mul(2);
            }
        }
    }
    Err(last_err.expect("retry must execute at least once"))
}

pub async fn call_with_resilience<T, E, F, Fut>(
    breaker: &CircuitBreaker,
    max_attempts: u32,
    base_delay: Duration,
    operation: F,
    is_transient: impl FnMut(&E) -> bool,
) -> Result<T, ResilienceError<E>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    if !breaker.can_execute() {
        return Err(ResilienceError::CircuitOpen);
    }
    match retry_with_backoff(max_attempts, base_delay, operation, is_transient).await {
        Ok(value) => {
            breaker.record_success();
            Ok(value)
        }
        Err(err) => {
            breaker.record_failure();
            Err(ResilienceError::Inner(err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn circuit_opens_after_threshold() {
        let cb = CircuitBreaker::new(2, Duration::from_secs(60));
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.can_execute());
    }

    #[test]
    fn circuit_half_open_then_closes_on_success() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.can_execute());
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn circuit_half_open_allows_single_probe() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(10));
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(15));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.can_execute());
        assert!(!cb.can_execute());
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn retry_retries_transient_errors() {
        let calls = AtomicU32::new(0);
        let result = retry_with_backoff(
            3,
            Duration::from_millis(1),
            || async {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err::<u32, &str>("transient")
                } else {
                    Ok(42)
                }
            },
            |_| true,
        )
        .await;
        assert_eq!(result, Ok(42));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_stops_on_non_transient() {
        let calls = AtomicU32::new(0);
        let result = retry_with_backoff(
            5,
            Duration::from_millis(1),
            || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), &str>("fatal")
            },
            |_| false,
        )
        .await;
        assert_eq!(result, Err("fatal"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
