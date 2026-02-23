use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use xrouter_contracts::{ReasoningConfig, ResponsesInput};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome,
};

use crate::{HttpRuntime, base_chat_payload};

pub struct OpenAiClient {
    runtime: HttpRuntime,
}

impl OpenAiClient {
    pub fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(provider_id, base_url, api_key, http_client, max_inflight),
        }
    }
}

#[async_trait]
impl ProviderClient for OpenAiClient {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openai_payload(
            request.model,
            request.input,
            request.reasoning,
            request.tools,
            request.tool_choice,
        );
        self.runtime.post_chat_completions_stream("request", &url, &payload, None, &[], None).await
    }

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openai_payload(
            request.request.model,
            request.request.input,
            request.request.reasoning,
            request.request.tools,
            request.request.tool_choice,
        );
        self.runtime
            .post_chat_completions_stream(
                request.request_id,
                &url,
                &payload,
                None,
                &[],
                request.sender,
            )
            .await
    }
}

pub(crate) fn build_openai_payload(
    model: &str,
    input: &ResponsesInput,
    reasoning: Option<&ReasoningConfig>,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> Value {
    let mut payload = base_chat_payload(model, input, tools, tool_choice);
    if let Some(reasoning_cfg) = normalize_openai_reasoning(reasoning) {
        payload.insert("reasoning".to_string(), reasoning_cfg);
    }
    Value::Object(payload)
}

fn normalize_openai_reasoning(reasoning: Option<&ReasoningConfig>) -> Option<Value> {
    let effort = reasoning?.effort.as_deref()?.trim();
    if effort.is_empty() {
        return None;
    }
    let mapped = if effort.eq_ignore_ascii_case("xhigh") { "high" } else { effort };
    Some(json!({ "effort": mapped }))
}
