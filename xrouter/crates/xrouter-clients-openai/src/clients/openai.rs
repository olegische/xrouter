use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::Client;
use serde_json::{Value, json};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use xrouter_contracts::{ReasoningConfig, ResponsesInput, ResponsesRequest};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome,
};

use crate::protocol::base_chat_payload;
use crate::runtime::SharedProviderRuntime;
#[cfg(not(target_arch = "wasm32"))]
use crate::transport::HttpRuntime;

pub struct OpenAiClient {
    runtime: SharedProviderRuntime,
}

impl OpenAiClient {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self::with_runtime(Arc::new(HttpRuntime::new(
            provider_id,
            base_url,
            api_key,
            http_client,
            max_inflight,
        )))
    }

    pub fn with_runtime(runtime: SharedProviderRuntime) -> Self {
        Self { runtime }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl ProviderClient for OpenAiClient {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openai_payload(
            request.model,
            request.instructions,
            request.input,
            request.reasoning,
            request.tools,
            request.tool_choice,
        );
        self.runtime
            .post_chat_completions_stream("request", &url, &payload, request.auth_bearer, &[], None)
            .await
    }

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openai_payload(
            request.request.model,
            request.request.instructions,
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
                request.request.auth_bearer,
                &[],
                request.sender,
            )
            .await
    }
}

pub(crate) fn build_openai_payload(
    model: &str,
    instructions: Option<&str>,
    input: &ResponsesInput,
    reasoning: Option<&ReasoningConfig>,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> Value {
    let mut payload = base_chat_payload(
        &ResponsesRequest {
            model: model.to_string(),
            instructions: instructions.map(str::to_string),
            previous_response_id: None,
            input: input.clone(),
            parallel_tool_calls: None,
            stream: true,
            reasoning: reasoning.cloned(),
            store: None,
            include: None,
            service_tier: None,
            prompt_cache_key: None,
            text: None,
            tools: None,
            tool_choice: None,
        },
        tools,
        tool_choice,
    );
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

#[cfg(test)]
mod tests {
    use super::build_openai_payload;
    use serde_json::json;
    use xrouter_contracts::{ReasoningConfig, ResponsesInput};

    #[test]
    fn maps_xhigh_to_high() {
        let input = ResponsesInput::Text("Reply with ok".to_string());
        let reasoning = ReasoningConfig { effort: Some("xhigh".to_string()), summary: None };
        let payload =
            build_openai_payload("gpt-4.1-mini", None, &input, Some(&reasoning), None, None);
        assert_eq!(payload["reasoning"]["effort"], "high");
    }

    #[test]
    fn forces_stream_true() {
        let input = ResponsesInput::Text("hello".to_string());
        let payload = build_openai_payload("gpt-4.1-mini", None, &input, None, None, None);
        assert_eq!(payload["stream"], json!(true));
    }
}
