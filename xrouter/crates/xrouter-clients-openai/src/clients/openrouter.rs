use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;
use xrouter_contracts::{ReasoningConfig, ResponseEvent, ResponsesInput};
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

use crate::{HttpRuntime, base_chat_payload};

pub struct OpenRouterClient {
    runtime: HttpRuntime,
}

impl OpenRouterClient {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "openrouter".to_string(),
                base_url,
                api_key,
                http_client,
                max_inflight,
            ),
        }
    }
}

#[async_trait]
impl ProviderClient for OpenRouterClient {
    async fn generate(
        &self,
        model: &str,
        input: &ResponsesInput,
        reasoning: Option<&ReasoningConfig>,
        tools: Option<&[Value]>,
        tool_choice: Option<&Value>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openrouter_payload(model, input, reasoning, tools, tool_choice);
        self.runtime.post_chat_completions_stream("request", &url, &payload, None, &[], None).await
    }

    async fn generate_stream(
        &self,
        request_id: &str,
        model: &str,
        input: &ResponsesInput,
        reasoning: Option<&ReasoningConfig>,
        tools: Option<&[Value]>,
        tool_choice: Option<&Value>,
        sender: Option<&mpsc::Sender<Result<ResponseEvent, CoreError>>>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openrouter_payload(model, input, reasoning, tools, tool_choice);
        self.runtime
            .post_chat_completions_stream(request_id, &url, &payload, None, &[], sender)
            .await
    }
}

pub(crate) fn build_openrouter_payload(
    model: &str,
    input: &ResponsesInput,
    reasoning: Option<&ReasoningConfig>,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> Value {
    let mut payload = base_chat_payload(model, input, tools, tool_choice);
    if let Some(reasoning_cfg) = reasoning
        && let Ok(value) = serde_json::to_value(reasoning_cfg)
    {
        payload.insert("reasoning".to_string(), value);
    }
    Value::Object(payload)
}
