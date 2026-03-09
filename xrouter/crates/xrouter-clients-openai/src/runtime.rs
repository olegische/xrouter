use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use xrouter_core::{CoreError, ProviderOutcome, ResponseEventSink};

pub type SharedProviderRuntime = Arc<dyn ProviderRuntime>;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ProviderRuntime: Send + Sync {
    fn api_key(&self) -> Option<String>;

    fn build_url(&self, path: &str) -> Result<String, CoreError>;

    async fn post_chat_completions_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError>;

    async fn post_responses_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError>;

    async fn post_form_json(
        &self,
        url: &str,
        form_fields: &[(String, String)],
        headers: &[(String, String)],
    ) -> Result<Value, CoreError>;
}
