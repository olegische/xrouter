use async_trait::async_trait;
use reqwest::Client;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use tracing::{debug, info};
use xrouter_contracts::{ReasoningConfig, ResponsesInput};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome,
};

use crate::{HttpRuntime, base_chat_payload};

pub struct XrouterClient {
    runtime: HttpRuntime,
}

impl XrouterClient {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "xrouter".to_string(),
                base_url,
                api_key,
                http_client,
                max_inflight,
            ),
        }
    }
}

#[async_trait]
impl ProviderClient for XrouterClient {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let (payload, normalization) = build_xrouter_payload(
            request.model,
            request.input,
            request.reasoning,
            request.tools,
            request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "xrouter",
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
                provider = "xrouter",
                model = request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        self.runtime.post_chat_completions_stream("request", &url, &payload, None, &[], None).await
    }

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let (payload, normalization) = build_xrouter_payload(
            request.request.model,
            request.request.input,
            request.request.reasoning,
            request.request.tools,
            request.request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "xrouter",
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
                provider = "xrouter",
                model = request.request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
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

pub(crate) fn build_xrouter_payload(
    model: &str,
    input: &ResponsesInput,
    reasoning: Option<&ReasoningConfig>,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> (Value, XrouterNormalization) {
    let normalized_tools = normalize_tools_for_chat_completions(tools);
    let normalized_tool_choice =
        normalize_tool_choice_for_chat_completions(tool_choice, !normalized_tools.tools.is_empty());
    let mut payload = base_chat_payload(
        model,
        input,
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
        XrouterNormalization {
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
pub(crate) struct XrouterNormalization {
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

    if let Some(function) = tool_obj.get("function").and_then(Value::as_object) {
        let name = function.get("name").and_then(Value::as_str)?.trim();
        if name.is_empty() {
            return None;
        }
        return Some(tool.clone());
    }

    let kind = tool_obj.get("type").and_then(Value::as_str).unwrap_or("function");
    if kind != "function" {
        return None;
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
        .or_else(|| tool_obj.get("schema").cloned())
        .unwrap_or_else(|| json!({"type":"object","properties":{}}));
    function.insert("parameters".to_string(), parameters);
    if let Some(strict) = tool_obj.get("strict").cloned() {
        function.insert("strict".to_string(), strict);
    }

    Some(json!({
        "type": "function",
        "function": Value::Object(function),
    }))
}

#[cfg(test)]
mod tests {
    use super::build_xrouter_payload;
    use serde_json::{Value, json};
    use xrouter_contracts::ResponsesInput;

    #[test]
    fn normalizes_bare_function_shape_to_openai_tools_shape() {
        let input = ResponsesInput::Text("hello".to_string());
        let tools = vec![json!({
            "name":"exec_command",
            "description":"run command",
            "strict": false,
            "parameters":{"type":"object","properties":{"cmd":{"type":"string"}}}
        })];
        let (payload, normalization) =
            build_xrouter_payload("zai-org/GLM-4.7-Flash", &input, None, Some(&tools), None);

        assert_eq!(normalization.tools_in, 1);
        assert_eq!(normalization.tools_out, 1);
        let payload_tools =
            payload.get("tools").and_then(Value::as_array).expect("tools must be present");
        assert_eq!(payload_tools[0]["type"], "function");
        assert_eq!(payload_tools[0]["function"]["name"], "exec_command");
        assert_eq!(payload_tools[0]["function"]["strict"], json!(false));
    }
}
