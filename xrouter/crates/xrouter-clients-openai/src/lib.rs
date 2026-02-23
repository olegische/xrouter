use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Semaphore;
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

#[derive(Debug, Clone, Copy)]
enum ClientMode {
    Real,
    Mock,
}

pub struct OpenAiCompatibleClient {
    provider_id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    http_client: Option<Client>,
    max_inflight: Option<Arc<Semaphore>>,
    mode: ClientMode,
}

impl OpenAiCompatibleClient {
    pub fn build_http_client(timeout_seconds: u64) -> Option<Client> {
        Client::builder().timeout(Duration::from_secs(timeout_seconds)).build().ok()
    }

    pub fn new_with_http_client(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        let max_inflight = max_inflight.map(Semaphore::new).map(Arc::new);
        Self { provider_id, base_url, api_key, http_client, max_inflight, mode: ClientMode::Real }
    }

    pub fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        timeout_seconds: u64,
        max_inflight: Option<usize>,
    ) -> Self {
        let http_client = Self::build_http_client(timeout_seconds);
        Self::new_with_http_client(provider_id, base_url, api_key, http_client, max_inflight)
    }

    pub fn mock(provider_id: String) -> Self {
        Self {
            provider_id,
            base_url: None,
            api_key: None,
            http_client: None,
            max_inflight: None,
            mode: ClientMode::Mock,
        }
    }

    fn mock_generate(&self, model: &str, input: &str) -> Result<ProviderOutcome, CoreError> {
        let mut chunks = Vec::new();
        let mut output_tokens = 0u32;

        for token in input.split_whitespace() {
            output_tokens = output_tokens.saturating_add(1);
            chunks.push(format!("{token} "));
        }

        if chunks.is_empty() {
            return Err(CoreError::Provider("provider returned empty output".to_string()));
        }

        chunks.insert(0, format!("[{}] ", self.provider_id));
        let reasoning = if model.contains("deepseek-reasoner") {
            Some("Reasoned with DeepSeek reasoning mode before composing final answer.".to_string())
        } else {
            None
        };

        Ok(ProviderOutcome { chunks, output_tokens, reasoning })
    }

    async fn remote_generate(
        &self,
        model: &str,
        input: &str,
    ) -> Result<ProviderOutcome, CoreError> {
        let Some(base_url) = self.base_url.as_deref().filter(|v| !v.trim().is_empty()) else {
            return Err(CoreError::Provider("provider base_url is not configured".to_string()));
        };
        let _permit = self
            .max_inflight
            .as_ref()
            .map(|semaphore| {
                semaphore.clone().try_acquire_owned().map_err(|_| {
                    CoreError::Provider(format!(
                        "provider overloaded: max in-flight limit reached for {}",
                        self.provider_id
                    ))
                })
            })
            .transpose()?;
        let client = self
            .http_client
            .as_ref()
            .ok_or_else(|| CoreError::Provider("provider client init failed".to_string()))?;

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let mut request =
            client.post(url).header("Content-Type", "application/json").json(&json!({
                "model": model,
                "messages": [{"role": "user", "content": input}],
                "stream": false
            }));

        if let Some(api_key) = self.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
            request = request.bearer_auth(api_key);
        }

        let response = request
            .send()
            .await
            .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| CoreError::Provider(format!("provider returned error status: {err}")))?;

        let payload = response
            .json::<ChatCompletionsResponse>()
            .await
            .map_err(|err| CoreError::Provider(format!("provider response parse failed: {err}")))?;
        let first = payload
            .choices
            .first()
            .ok_or_else(|| CoreError::Provider("provider returned empty choices".to_string()))?;
        let content = extract_message_content(&first.message.content).ok_or_else(|| {
            CoreError::Provider("provider returned empty message content".to_string())
        })?;
        let chunks = vec![content.clone()];
        let output_tokens = payload
            .usage
            .and_then(|usage| usage.completion_tokens)
            .unwrap_or_else(|| content.split_whitespace().count() as u32);
        let reasoning =
            first.message.reasoning_content.clone().or_else(|| first.message.reasoning.clone());

        Ok(ProviderOutcome { chunks, output_tokens, reasoning })
    }
}

#[async_trait]
impl ProviderClient for OpenAiCompatibleClient {
    async fn generate(&self, model: &str, input: &str) -> Result<ProviderOutcome, CoreError> {
        match self.mode {
            ClientMode::Real => self.remote_generate(model, input).await,
            ClientMode::Mock => self.mock_generate(model, input),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    content: serde_json::Value,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    completion_tokens: Option<u32>,
}

fn extract_message_content(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(text) if !text.is_empty() => Some(text.clone()),
        serde_json::Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}
