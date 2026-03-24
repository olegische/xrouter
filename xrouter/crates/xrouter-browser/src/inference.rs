use std::{collections::HashMap, sync::Arc};

use xrouter_clients_openai::runtime::SharedProviderRuntime;
use xrouter_clients_openai::{DeepSeekClient, OpenAiClient, OpenRouterClient, ZaiClient};
use xrouter_contracts::{ResponseEvent, ResponsesInput, ResponsesRequest, ResponsesResponse};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome, ResponseEventSink, response_completed_event_from_outcome,
    responses_response_from_outcome,
};

use crate::error::BrowserError;
use crate::runtime::BrowserProviderRuntime;

pub const DEFAULT_DEMO_PROMPT: &str = "Hello, what can you do?";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserProvider {
    DeepSeek,
    OpenAi,
    OpenRouter,
    Zai,
}

impl BrowserProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
            Self::Zai => "zai",
        }
    }
}

impl TryFrom<&str> for BrowserProvider {
    type Error = BrowserError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "deepseek" => Ok(Self::DeepSeek),
            "openai" => Ok(Self::OpenAi),
            "openrouter" => Ok(Self::OpenRouter),
            "zai" => Ok(Self::Zai),
            other => Err(BrowserError::UnsupportedProvider(other.to_string())),
        }
    }
}

pub struct BrowserInferenceClient {
    provider: BrowserProvider,
    runtime: Arc<BrowserProviderRuntime>,
    shared_runtime: SharedProviderRuntime,
}

impl BrowserInferenceClient {
    pub fn new(
        provider: BrowserProvider,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        let runtime = Arc::new(BrowserProviderRuntime::new(provider.as_str(), base_url, api_key));
        let shared_runtime: SharedProviderRuntime = runtime.clone();
        Self { provider, runtime, shared_runtime }
    }

    pub fn cancel(&self, request_id: &str) -> Result<(), BrowserError> {
        self.runtime.cancel(request_id)
    }

    pub async fn generate_text(
        &self,
        request_id: &str,
        model: &str,
        input: &str,
    ) -> Result<ProviderOutcome, CoreError> {
        self.generate_text_stream(request_id, model, input, None).await
    }

    pub async fn generate_text_stream(
        &self,
        request_id: &str,
        model: &str,
        input: &str,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError> {
        let request = ResponsesRequest {
            model: model.to_string(),
            instructions: None,
            previous_response_id: None,
            input: ResponsesInput::Text(input.to_string()),
            parallel_tool_calls: None,
            stream: true,
            reasoning: None,
            store: None,
            include: None,
            service_tier: None,
            prompt_cache_key: None,
            text: None,
            tools: None,
            tool_choice: None,
        };
        let (outcome, _) =
            self.generate_responses_with_outcome(request_id, &request, None, sender).await?;
        Ok(outcome)
    }

    pub async fn generate_responses(
        &self,
        request_id: &str,
        request: &ResponsesRequest,
    ) -> Result<ResponsesResponse, CoreError> {
        self.generate_responses_stream(request_id, request, None).await
    }

    pub async fn generate_responses_stream(
        &self,
        request_id: &str,
        request: &ResponsesRequest,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ResponsesResponse, CoreError> {
        self.generate_responses_stream_with_headers(request_id, request, None, sender).await
    }

    pub async fn generate_responses_stream_with_headers(
        &self,
        request_id: &str,
        request: &ResponsesRequest,
        request_headers: Option<&HashMap<String, String>>,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ResponsesResponse, CoreError> {
        let (_, response) = self
            .generate_responses_with_outcome(request_id, request, request_headers, sender)
            .await?;
        Ok(response)
    }

    async fn generate_responses_with_outcome(
        &self,
        request_id: &str,
        request: &ResponsesRequest,
        request_headers: Option<&HashMap<String, String>>,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<(ProviderOutcome, ResponsesResponse), CoreError> {
        let forward_headers = extract_forward_headers(self.provider, request_headers);
        let provider_request = build_provider_request(request, &forward_headers);
        match self.provider {
            BrowserProvider::DeepSeek => {
                let client = DeepSeekClient::with_runtime(self.shared_runtime.clone());
                finalize_stream_request(
                    request_id,
                    request,
                    sender,
                    client.generate_stream(ProviderGenerateStreamRequest {
                        request_id,
                        request: provider_request,
                        sender,
                    }),
                )
                .await
            }
            BrowserProvider::OpenAi => {
                let client = OpenAiClient::with_runtime(self.shared_runtime.clone());
                finalize_stream_request(
                    request_id,
                    request,
                    sender,
                    client.generate_stream(ProviderGenerateStreamRequest {
                        request_id,
                        request: provider_request,
                        sender,
                    }),
                )
                .await
            }
            BrowserProvider::OpenRouter => {
                let client = OpenRouterClient::with_runtime(self.shared_runtime.clone());
                finalize_stream_request(
                    request_id,
                    request,
                    sender,
                    client.generate_stream(ProviderGenerateStreamRequest {
                        request_id,
                        request: provider_request,
                        sender,
                    }),
                )
                .await
            }
            BrowserProvider::Zai => {
                let client = ZaiClient::with_runtime(self.shared_runtime.clone());
                finalize_stream_request(
                    request_id,
                    request,
                    sender,
                    client.generate_stream(ProviderGenerateStreamRequest {
                        request_id,
                        request: provider_request,
                        sender,
                    }),
                )
                .await
            }
        }
    }

    pub async fn generate_demo_prompt_stream(
        &self,
        request_id: &str,
        model: &str,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError> {
        self.generate_text_stream(request_id, model, DEFAULT_DEMO_PROMPT, sender).await
    }
}

fn build_provider_request<'a>(
    request: &'a ResponsesRequest,
    forward_headers: &'a [(String, String)],
) -> ProviderGenerateRequest<'a> {
    ProviderGenerateRequest {
        model: &request.model,
        instructions: request.instructions.as_deref(),
        input: &request.input,
        reasoning: request.reasoning.as_ref(),
        tools: request.tools.as_deref(),
        tool_choice: request.tool_choice.as_ref(),
        auth_bearer: None,
        forward_headers,
    }
}

fn extract_forward_headers(
    provider: BrowserProvider,
    request_headers: Option<&HashMap<String, String>>,
) -> Vec<(String, String)> {
    if provider != BrowserProvider::OpenRouter {
        return Vec::new();
    }
    let Some(request_headers) = request_headers else {
        return Vec::new();
    };

    const OPENROUTER_FORWARD_HEADERS: [&str; 4] =
        ["HTTP-Referer", "X-OpenRouter-Title", "X-Title", "X-OpenRouter-Categories"];

    OPENROUTER_FORWARD_HEADERS
        .iter()
        .filter_map(|name| {
            request_headers.iter().find_map(|(header_name, value)| {
                header_name
                    .eq_ignore_ascii_case(name)
                    .then_some(((*name).to_string(), value.clone()))
            })
        })
        .collect()
}

async fn finalize_stream_request(
    request_id: &str,
    request: &ResponsesRequest,
    sender: Option<&dyn ResponseEventSink>,
    future: impl std::future::Future<Output = Result<ProviderOutcome, CoreError>>,
) -> Result<(ProviderOutcome, ResponsesResponse), CoreError> {
    let outcome = match future.await {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(tx) = sender {
                tx.send(Err(error.clone())).await;
            }
            return Err(error);
        }
    };

    emit_non_live_events(request_id, &outcome, sender).await;

    let input_tokens = request.input.to_canonical_text().split_whitespace().count() as u32;
    let response = responses_response_from_outcome(request_id, input_tokens, &outcome);
    if let Some(tx) = sender {
        tx.send(Ok(response_completed_event_from_outcome(request_id, input_tokens, &outcome)))
            .await;
    }

    Ok((outcome, response))
}

async fn emit_non_live_events(
    request_id: &str,
    outcome: &ProviderOutcome,
    sender: Option<&dyn ResponseEventSink>,
) {
    if outcome.emitted_live {
        return;
    }
    let Some(tx) = sender else {
        return;
    };

    if let Some(reasoning) = outcome.reasoning.as_ref().filter(|value| !value.trim().is_empty()) {
        tx.send(Ok(ResponseEvent::ReasoningDelta {
            id: request_id.to_string(),
            delta: reasoning.clone(),
        }))
        .await;
    }

    for chunk in &outcome.chunks {
        tx.send(Ok(ResponseEvent::OutputTextDelta {
            id: request_id.to_string(),
            delta: chunk.clone(),
        }))
        .await;
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use async_trait::async_trait;
    use serde_json::json;
    use xrouter_contracts::{ReasoningConfig, ResponseEvent};
    use xrouter_core::{CoreError, responses_response_from_outcome};

    use super::{
        BrowserInferenceClient, BrowserProvider, DEFAULT_DEMO_PROMPT, build_provider_request,
        emit_non_live_events, extract_forward_headers,
    };

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<Result<ResponseEvent, CoreError>>>,
    }

    #[async_trait]
    impl xrouter_core::ResponseEventSink for RecordingSink {
        async fn send(&self, event: Result<ResponseEvent, CoreError>) {
            self.events.lock().expect("lock").push(event);
        }
    }

    #[test]
    fn provider_parser_rejects_unknown_provider() {
        let result = super::BrowserProvider::try_from("yandex");
        assert!(result.is_err());
    }

    #[test]
    fn provider_parser_accepts_supported_browser_providers() {
        assert_eq!(
            super::BrowserProvider::try_from("deepseek").ok(),
            Some(BrowserProvider::DeepSeek)
        );
        assert_eq!(super::BrowserProvider::try_from("openai").ok(), Some(BrowserProvider::OpenAi));
        assert_eq!(
            super::BrowserProvider::try_from("openrouter").ok(),
            Some(BrowserProvider::OpenRouter)
        );
        assert_eq!(super::BrowserProvider::try_from("zai").ok(), Some(BrowserProvider::Zai));
    }

    #[test]
    fn native_inference_reports_unsupported_platform() {
        let client = BrowserInferenceClient::new(
            BrowserProvider::DeepSeek,
            Some("https://api.deepseek.com".to_string()),
            Some("test".to_string()),
        );
        let result = futures::executor::block_on(client.generate_demo_prompt_stream(
            "request-1",
            "deepseek-chat",
            None,
        ));
        assert!(matches!(result, Err(CoreError::Provider(message)) if message.contains("wasm32")));
    }

    #[test]
    fn demo_prompt_stays_stable() {
        assert_eq!(DEFAULT_DEMO_PROMPT, "Hello, what can you do?");
    }

    #[test]
    fn cancel_is_idempotent_on_native() {
        let client = BrowserInferenceClient::new(
            BrowserProvider::DeepSeek,
            Some("https://api.deepseek.com".to_string()),
            Some("test".to_string()),
        );
        client.cancel("request-1").expect("cancel should be idempotent");
    }

    #[test]
    fn provider_request_preserves_tooling_fields() {
        let request = xrouter_contracts::ResponsesRequest {
            model: "gpt-4.1-mini".to_string(),
            instructions: Some("be precise".to_string()),
            previous_response_id: None,
            input: xrouter_contracts::ResponsesInput::Items(vec![
                xrouter_contracts::ResponseInputItem {
                    kind: Some("message".to_string()),
                    role: Some("user".to_string()),
                    text: Some("hello".to_string()),
                    ..Default::default()
                },
            ]),
            parallel_tool_calls: None,
            stream: true,
            reasoning: Some(ReasoningConfig { effort: Some("high".to_string()), summary: None }),
            store: None,
            include: None,
            service_tier: None,
            prompt_cache_key: None,
            text: None,
            tools: Some(vec![json!({
                "type": "function",
                "function": {
                    "name": "lookup_weather",
                    "parameters": {"type": "object"}
                }
            })]),
            tool_choice: Some(json!("auto")),
        };

        let provider_request = build_provider_request(&request, &[]);
        assert_eq!(provider_request.instructions, Some("be precise"));
        assert_eq!(provider_request.tools.expect("tools")[0]["function"]["name"], "lookup_weather");
        assert_eq!(provider_request.tool_choice.expect("tool choice"), &json!("auto"));
        assert_eq!(provider_request.reasoning.expect("reasoning").effort.as_deref(), Some("high"));
        assert!(provider_request.forward_headers.is_empty());
    }

    #[test]
    fn extracts_openrouter_forward_headers_from_browser_request_headers() {
        let mut request_headers = HashMap::new();
        request_headers.insert("http-referer".to_string(), "https://xcodex.chat".to_string());
        request_headers.insert("x-title".to_string(), "XCodex".to_string());
        request_headers.insert("x-openrouter-categories".to_string(), "cloud-agent".to_string());
        request_headers.insert("authorization".to_string(), "Bearer ignored".to_string());

        let headers = extract_forward_headers(BrowserProvider::OpenRouter, Some(&request_headers));

        assert_eq!(
            headers,
            vec![
                ("HTTP-Referer".to_string(), "https://xcodex.chat".to_string()),
                ("X-Title".to_string(), "XCodex".to_string()),
                ("X-OpenRouter-Categories".to_string(), "cloud-agent".to_string()),
            ]
        );
    }

    #[test]
    fn ignores_browser_request_headers_for_non_openrouter_provider() {
        let mut request_headers = HashMap::new();
        request_headers.insert("HTTP-Referer".to_string(), "https://xcodex.chat".to_string());

        let headers = extract_forward_headers(BrowserProvider::OpenAi, Some(&request_headers));

        assert!(headers.is_empty());
    }

    #[test]
    fn completed_response_includes_function_calls() {
        let request = xrouter_contracts::ResponsesRequest {
            model: "gpt-4.1-mini".to_string(),
            instructions: None,
            previous_response_id: None,
            input: xrouter_contracts::ResponsesInput::Text("call the tool".to_string()),
            parallel_tool_calls: None,
            stream: true,
            reasoning: None,
            store: None,
            include: None,
            service_tier: None,
            prompt_cache_key: None,
            text: None,
            tools: None,
            tool_choice: None,
        };
        let outcome = xrouter_core::ProviderOutcome {
            chunks: Vec::new(),
            output_tokens: 7,
            reasoning: None,
            reasoning_details: None,
            tool_calls: Some(vec![xrouter_contracts::ToolCall {
                id: "call_123".to_string(),
                kind: "function".to_string(),
                function: xrouter_contracts::ToolFunction {
                    name: "lookup_weather".to_string(),
                    arguments: "{\"city\":\"Moscow\"}".to_string(),
                },
            }]),
            emitted_live: true,
        };

        let response = responses_response_from_outcome(
            "req-1",
            request.input.to_canonical_text().split_whitespace().count() as u32,
            &outcome,
        );
        assert_eq!(response.finish_reason, "tool_calls");
        assert!(response.output.iter().any(|item| matches!(
            item,
            xrouter_contracts::ResponseOutputItem::FunctionCall { call_id, name, .. }
            if call_id == "call_123" && name == "lookup_weather"
        )));
    }

    #[test]
    fn non_live_outcome_replays_deltas_to_sink() {
        let sink = RecordingSink::default();
        let outcome = xrouter_core::ProviderOutcome {
            chunks: vec!["hello ".to_string(), "world".to_string()],
            output_tokens: 2,
            reasoning: Some("thinking".to_string()),
            reasoning_details: None,
            tool_calls: None,
            emitted_live: false,
        };

        futures::executor::block_on(emit_non_live_events("req-1", &outcome, Some(&sink)));

        let events = sink.events.lock().expect("lock");
        assert!(events.iter().any(|event| matches!(
            event,
            Ok(ResponseEvent::ReasoningDelta { delta, .. }) if delta == "thinking"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            Ok(ResponseEvent::OutputTextDelta { delta, .. }) if delta == "hello "
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            Ok(ResponseEvent::OutputTextDelta { delta, .. }) if delta == "world"
        )));
    }
}
