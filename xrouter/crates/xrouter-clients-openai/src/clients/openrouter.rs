use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::Client;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use tracing::{debug, info};
use xrouter_contracts::{ReasoningConfig, ResponsesInput, ResponsesRequest};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome,
};

use crate::protocol::base_chat_payload;
use crate::runtime::SharedProviderRuntime;
#[cfg(not(target_arch = "wasm32"))]
use crate::transport::HttpRuntime;

pub struct OpenRouterClient {
    runtime: SharedProviderRuntime,
}

impl OpenRouterClient {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self::with_runtime(Arc::new(HttpRuntime::new(
            "openrouter".to_string(),
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
impl ProviderClient for OpenRouterClient {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let (payload, normalization) = build_openrouter_payload(
            request.model,
            request.instructions,
            request.input,
            request.reasoning,
            request.tools,
            request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "openrouter",
            model = request.model,
            tools_in = normalization.tools_in,
            tools_out = normalization.tools_out,
            tools_dropped = normalization.tools_dropped,
            tool_choice_in = normalization.tool_choice_in,
            tool_choice_out = normalization.tool_choice_out
        );
        if !normalization.dropped_tool_types.is_empty() {
            debug!(
                event = "provider.request.payload.normalized.details",
                provider = "openrouter",
                model = request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        log_forwarded_attribution_headers(request.model, request.forward_headers);
        self.runtime
            .post_chat_completions_stream(
                "request",
                &url,
                &payload,
                request.auth_bearer,
                request.forward_headers,
                None,
            )
            .await
    }

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let (payload, normalization) = build_openrouter_payload(
            request.request.model,
            request.request.instructions,
            request.request.input,
            request.request.reasoning,
            request.request.tools,
            request.request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "openrouter",
            model = request.request.model,
            tools_in = normalization.tools_in,
            tools_out = normalization.tools_out,
            tools_dropped = normalization.tools_dropped,
            tool_choice_in = normalization.tool_choice_in,
            tool_choice_out = normalization.tool_choice_out
        );
        if !normalization.dropped_tool_types.is_empty() {
            debug!(
                event = "provider.request.payload.normalized.details",
                provider = "openrouter",
                model = request.request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        log_forwarded_attribution_headers(request.request.model, request.request.forward_headers);
        self.runtime
            .post_chat_completions_stream(
                request.request_id,
                &url,
                &payload,
                request.request.auth_bearer,
                request.request.forward_headers,
                request.sender,
            )
            .await
    }
}

pub(crate) fn build_openrouter_payload(
    model: &str,
    instructions: Option<&str>,
    input: &ResponsesInput,
    reasoning: Option<&ReasoningConfig>,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> (Value, OpenRouterNormalization) {
    let normalized_tools = normalize_tools_for_chat_completions(tools);
    let normalized_tool_choice =
        normalize_tool_choice_for_chat_completions(tool_choice, !normalized_tools.tools.is_empty());
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
        Some(&normalized_tools.tools),
        normalized_tool_choice.as_ref(),
    );
    if let Some(reasoning_cfg) = reasoning
        && let Ok(value) = serde_json::to_value(reasoning_cfg)
    {
        payload.insert("reasoning".to_string(), value);
    }
    (
        Value::Object(payload),
        OpenRouterNormalization {
            tools_in: tools.map(|t| t.len()).unwrap_or(0),
            tools_out: normalized_tools.tools.len(),
            tools_dropped: normalized_tools.dropped_count,
            dropped_tool_types: normalized_tools.dropped_tool_types,
            tool_choice_in: tool_choice
                .map(tool_choice_debug_label)
                .unwrap_or_else(|| "none".to_string()),
            tool_choice_out: normalized_tool_choice
                .as_ref()
                .map(tool_choice_debug_label)
                .unwrap_or_else(|| "none".to_string()),
        },
    )
}

#[derive(Debug, Clone)]
pub(crate) struct OpenRouterNormalization {
    pub(crate) tools_in: usize,
    pub(crate) tools_out: usize,
    pub(crate) tools_dropped: usize,
    pub(crate) dropped_tool_types: Vec<String>,
    pub(crate) tool_choice_in: String,
    pub(crate) tool_choice_out: String,
}

#[derive(Debug, Clone)]
struct NormalizedTools {
    tools: Vec<Value>,
    dropped_count: usize,
    dropped_tool_types: Vec<String>,
}

fn normalize_tools_for_chat_completions(tools: Option<&[Value]>) -> NormalizedTools {
    let mut normalized = Vec::new();
    let mut dropped_tool_types = Vec::new();
    for tool in tools.unwrap_or(&[]) {
        if let Some(function_tool) = normalize_function_tool(tool) {
            normalized.push(function_tool);
        } else {
            dropped_tool_types
                .push(tool.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string());
        }
    }
    let dropped_count = dropped_tool_types.len();
    NormalizedTools { tools: normalized, dropped_count, dropped_tool_types }
}

fn normalize_tool_choice_for_chat_completions(
    tool_choice: Option<&Value>,
    has_tools: bool,
) -> Option<Value> {
    if !has_tools {
        return None;
    }
    let choice = tool_choice?;
    if let Some(text) = choice.as_str() {
        return match text {
            "auto" | "none" | "required" => Some(Value::String(text.to_string())),
            "any" => Some(Value::String("required".to_string())),
            _ => None,
        };
    }
    let obj = choice.as_object()?;
    let kind = obj.get("type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "auto" => Some(Value::String("auto".to_string())),
        "none" => Some(Value::String("none".to_string())),
        "required" | "any" => Some(Value::String("required".to_string())),
        "function" => {
            if let Some(function) = obj.get("function").and_then(Value::as_object)
                && let Some(name) = function.get("name").and_then(Value::as_str)
                && !name.trim().is_empty()
            {
                return Some(json!({"type":"function","function":{"name":name}}));
            }
            if let Some(name) = obj.get("name").and_then(Value::as_str)
                && !name.trim().is_empty()
            {
                return Some(json!({"type":"function","function":{"name":name}}));
            }
            None
        }
        "tool" => obj
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(|name| json!({"type":"function","function":{"name":name}})),
        _ => None,
    }
}

fn tool_choice_debug_label(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return format!("string:{text}");
    }
    if let Some(kind) = value.get("type").and_then(Value::as_str) {
        return format!("object:{kind}");
    }
    "other".to_string()
}

fn normalize_function_tool(tool: &Value) -> Option<Value> {
    let tool_obj = tool.as_object()?;
    let kind = tool_obj.get("type").and_then(Value::as_str)?;
    if kind != "function" {
        return None;
    }

    if let Some(function) = tool_obj.get("function") {
        let function_obj = function.as_object()?;
        let name = function_obj.get("name").and_then(Value::as_str)?.trim();
        if name.is_empty() {
            return None;
        }
        return Some(tool.clone());
    }

    let name = tool_obj.get("name").and_then(Value::as_str)?.trim();
    if name.is_empty() {
        return None;
    }
    let mut function = Map::new();
    function.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = tool_obj.get("description").cloned() {
        function.insert("description".to_string(), description);
    }
    let parameters = tool_obj
        .get("parameters")
        .cloned()
        .or_else(|| tool_obj.get("input_schema").cloned())
        .unwrap_or_else(|| json!({"type":"object","properties":{}}));
    function.insert("parameters".to_string(), parameters);

    Some(json!({
        "type": "function",
        "function": Value::Object(function),
    }))
}

fn log_forwarded_attribution_headers(model: &str, headers: &[(String, String)]) {
    let referer = find_forwarded_header(headers, "HTTP-Referer");
    let title = find_forwarded_header(headers, "X-OpenRouter-Title")
        .or_else(|| find_forwarded_header(headers, "X-Title"));
    let categories = find_forwarded_header(headers, "X-OpenRouter-Categories");

    if referer.is_none() && title.is_none() && categories.is_none() {
        return;
    }

    debug!(
        event = "provider.request.headers.forwarded",
        provider = "openrouter",
        model = model,
        openrouter.forwarded_referer = referer.unwrap_or(""),
        openrouter.forwarded_title = title.unwrap_or(""),
        openrouter.forwarded_categories = categories.unwrap_or("")
    );
}

fn find_forwarded_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find_map(|(header_name, value)| {
        header_name.eq_ignore_ascii_case(name).then_some(value.as_str())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        OpenRouterClient, build_openrouter_payload, find_forwarded_header,
        normalize_tool_choice_for_chat_completions,
    };
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use xrouter_contracts::{ReasoningConfig, ResponsesInput};
    use xrouter_core::{
        CoreError, ProviderGenerateRequest, ProviderGenerateStreamRequest, ProviderOutcome,
        ResponseEventSink,
    };

    use crate::runtime::ProviderRuntime;

    #[test]
    fn keeps_only_function_tools_and_tracks_drops() {
        let input = ResponsesInput::Text("hello".to_string());
        let tools = vec![
            json!({"type":"function","name":"ping","parameters":{"type":"object","properties":{}}}),
            json!({"type":"web_search"}),
        ];
        let (payload, normalization) =
            build_openrouter_payload("openai/gpt-4.1-mini", None, &input, None, Some(&tools), None);

        assert_eq!(normalization.tools_in, 2);
        assert_eq!(normalization.tools_out, 1);
        assert_eq!(normalization.tools_dropped, 1);
        assert_eq!(normalization.dropped_tool_types, vec!["web_search".to_string()]);

        let payload_tools =
            payload.get("tools").and_then(Value::as_array).expect("tools must be present");
        assert_eq!(payload_tools.len(), 1);
        assert_eq!(payload_tools[0]["type"], "function");
        assert_eq!(payload_tools[0]["function"]["name"], "ping");
    }

    #[test]
    fn normalizes_tool_choice_variants_for_chat_completions() {
        assert_eq!(
            normalize_tool_choice_for_chat_completions(Some(&json!("any")), true),
            Some(json!("required"))
        );
        assert_eq!(
            normalize_tool_choice_for_chat_completions(
                Some(&json!({"type":"function","name":"ping"})),
                true
            ),
            Some(json!({"type":"function","function":{"name":"ping"}}))
        );
        assert_eq!(normalize_tool_choice_for_chat_completions(Some(&json!("auto")), false), None);
    }

    #[test]
    fn keeps_reasoning_effort_as_is() {
        let input = ResponsesInput::Text("Reply with ok".to_string());
        let reasoning = ReasoningConfig { effort: Some("xhigh".to_string()), summary: None };
        let (payload, _) =
            build_openrouter_payload("openai/gpt-5.2", None, &input, Some(&reasoning), None, None);
        assert_eq!(payload["reasoning"]["effort"], "xhigh");
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn forces_stream_true() {
        let input = ResponsesInput::Text("hello".to_string());
        let (payload, _) =
            build_openrouter_payload("openai/gpt-5-mini", None, &input, None, None, None);
        assert_eq!(payload["stream"], json!(true));
    }

    struct HeaderCaptureRuntime {
        seen_headers: Arc<Mutex<Vec<(String, String)>>>,
    }

    #[async_trait]
    impl ProviderRuntime for HeaderCaptureRuntime {
        fn api_key(&self) -> Option<String> {
            None
        }

        fn build_url(&self, path: &str) -> Result<String, CoreError> {
            Ok(format!("https://openrouter.ai/api/v1/{}", path.trim_start_matches('/')))
        }

        async fn post_chat_completions_stream(
            &self,
            _request_id: &str,
            _url: &str,
            _payload: &Value,
            _bearer_override: Option<&str>,
            extra_headers: &[(String, String)],
            _sender: Option<&dyn ResponseEventSink>,
        ) -> Result<ProviderOutcome, CoreError> {
            *self.seen_headers.lock().expect("lock must succeed") = extra_headers.to_vec();
            Ok(ProviderOutcome {
                chunks: vec!["ok".to_string()],
                output_tokens: 1,
                reasoning: None,
                reasoning_details: None,
                tool_calls: None,
                emitted_live: false,
            })
        }

        async fn post_responses_stream(
            &self,
            _request_id: &str,
            _url: &str,
            _payload: &Value,
            _bearer_override: Option<&str>,
            _extra_headers: &[(String, String)],
            _sender: Option<&dyn ResponseEventSink>,
        ) -> Result<ProviderOutcome, CoreError> {
            panic!("OpenRouter client should use chat/completions transport");
        }

        async fn post_form_json(
            &self,
            _url: &str,
            _form_fields: &[(String, String)],
            _headers: &[(String, String)],
        ) -> Result<Value, CoreError> {
            panic!("OpenRouter client should not use form transport");
        }
    }

    #[tokio::test]
    async fn generate_passes_forward_headers_to_runtime() {
        let seen_headers = Arc::new(Mutex::new(Vec::new()));
        let runtime = Arc::new(HeaderCaptureRuntime { seen_headers: seen_headers.clone() });
        let client = OpenRouterClient::with_runtime(runtime);
        let input = ResponsesInput::Text("hello".to_string());
        let forward_headers = vec![
            ("HTTP-Referer".to_string(), "https://example.com".to_string()),
            ("X-OpenRouter-Title".to_string(), "Example App".to_string()),
        ];

        let result = xrouter_core::ProviderClient::generate(
            &client,
            ProviderGenerateRequest {
                model: "openai/gpt-5-mini",
                instructions: None,
                input: &input,
                reasoning: None,
                tools: None,
                tool_choice: None,
                auth_bearer: None,
                forward_headers: &forward_headers,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(*seen_headers.lock().expect("lock must succeed"), forward_headers);
    }

    #[tokio::test]
    async fn generate_stream_passes_forward_headers_to_runtime() {
        let seen_headers = Arc::new(Mutex::new(Vec::new()));
        let runtime = Arc::new(HeaderCaptureRuntime { seen_headers: seen_headers.clone() });
        let client = OpenRouterClient::with_runtime(runtime);
        let input = ResponsesInput::Text("hello".to_string());
        let forward_headers = vec![
            ("X-Title".to_string(), "Example App".to_string()),
            ("X-OpenRouter-Categories".to_string(), "cli-agent".to_string()),
        ];

        let result = xrouter_core::ProviderClient::generate_stream(
            &client,
            ProviderGenerateStreamRequest {
                request_id: "req_1",
                request: ProviderGenerateRequest {
                    model: "openai/gpt-5-mini",
                    instructions: None,
                    input: &input,
                    reasoning: None,
                    tools: None,
                    tool_choice: None,
                    auth_bearer: None,
                    forward_headers: &forward_headers,
                },
                sender: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(*seen_headers.lock().expect("lock must succeed"), forward_headers);
    }

    #[test]
    fn finds_forwarded_headers_case_insensitively() {
        let headers = vec![
            ("http-referer".to_string(), "https://example.com".to_string()),
            ("x-title".to_string(), "Example App".to_string()),
        ];

        assert_eq!(find_forwarded_header(&headers, "HTTP-Referer"), Some("https://example.com"));
        assert_eq!(find_forwarded_header(&headers, "X-Title"), Some("Example App"));
        assert_eq!(find_forwarded_header(&headers, "X-OpenRouter-Categories"), None);
    }
}
