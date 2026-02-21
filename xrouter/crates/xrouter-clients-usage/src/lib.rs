use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

#[cfg(feature = "billing")]
use async_trait::async_trait;
#[cfg(feature = "billing")]
use xrouter_core::{CoreError, FinalizeResult, UsageClient};

#[derive(Default)]
struct BillingState {
    holds: HashSet<String>,
    finalized: HashSet<String>,
    force_recovery: HashSet<String>,
    external_recovery: HashSet<String>,
}

#[derive(Clone, Default)]
pub struct InMemoryUsageClient {
    inner: Arc<Mutex<BillingState>>,
}

impl InMemoryUsageClient {
    pub async fn mark_force_recovery(&self, request_id: &str) {
        let mut guard = self.inner.lock().await;
        guard.force_recovery.insert(request_id.to_string());
    }

    pub async fn mark_external_recovery(&self, request_id: &str) {
        let mut guard = self.inner.lock().await;
        guard.external_recovery.insert(request_id.to_string());
    }
}

#[cfg(feature = "billing")]
#[async_trait]
impl UsageClient for InMemoryUsageClient {
    async fn acquire_hold(&self, request_id: &str, _expected_tokens: u32) -> Result<(), CoreError> {
        let mut guard = self.inner.lock().await;
        guard.holds.insert(request_id.to_string());
        Ok(())
    }

    async fn finalize_charge(
        &self,
        request_id: &str,
        billable_tokens: u32,
    ) -> Result<FinalizeResult, CoreError> {
        let mut guard = self.inner.lock().await;

        if !guard.holds.contains(request_id) {
            return Err(CoreError::Billing("finalize attempted without hold".to_string()));
        }

        if guard.finalized.contains(request_id) {
            return Ok(FinalizeResult::AlreadyCommitted);
        }

        if billable_tokens == 0 {
            guard.finalized.insert(request_id.to_string());
            return Ok(FinalizeResult::AlreadyCommitted);
        }

        if guard.external_recovery.contains(request_id) {
            return Ok(FinalizeResult::RecoveredExternally);
        }

        if guard.force_recovery.contains(request_id) {
            return Ok(FinalizeResult::RecoveryRequired);
        }

        guard.finalized.insert(request_id.to_string());
        Ok(FinalizeResult::Committed)
    }
}
