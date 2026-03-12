use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageName {
    Ingest,
    Tokenize,
    Generate,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextVerbosity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextFormatType {
    JsonSchema,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct TextFormatConfig {
    #[serde(rename = "type")]
    pub kind: TextFormatType,
    pub strict: bool,
    pub schema: Value,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct TextControls {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<TextVerbosity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormatConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(untagged)]
pub enum ResponseInputContent {
    Text(String),
    Parts(Vec<ResponseInputPart>),
}

impl ResponseInputContent {
    pub fn to_text(&self) -> Option<String> {
        match self {
            Self::Text(text) => {
                let text = text.trim();
                if text.is_empty() { None } else { Some(text.to_string()) }
            }
            Self::Parts(parts) => flatten_response_input_parts(parts),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponseInputPart {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(untagged)]
pub enum ResponseToolOutput {
    Text(String),
    Parts(Vec<ResponseInputPart>),
    Json(Value),
}

impl ResponseToolOutput {
    pub fn to_text_lossy(&self) -> Option<String> {
        match self {
            Self::Text(text) => {
                let text = text.trim();
                if text.is_empty() { None } else { Some(text.to_string()) }
            }
            Self::Parts(parts) => flatten_response_input_parts(parts)
                .or_else(|| serde_json::to_string(parts).ok().filter(|value| !value.is_empty())),
            Self::Json(value) => serialize_json_value(value),
        }
    }

    pub fn to_serialized_string(&self) -> Option<String> {
        match self {
            Self::Text(text) => {
                let text = text.trim();
                if text.is_empty() { None } else { Some(text.to_string()) }
            }
            Self::Parts(parts) => {
                serde_json::to_string(parts).ok().filter(|value| !value.is_empty())
            }
            Self::Json(value) => serialize_json_value(value),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponseInputItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ResponseInputContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<ResponseToolOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(untagged)]
pub enum ResponsesInput {
    Text(String),
    Items(Vec<ResponseInputItem>),
}

impl ResponsesInput {
    pub fn to_canonical_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Items(items) => flatten_response_items(items),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponsesRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    pub input: ResponsesInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextControls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub status: String,
    pub output: Vec<ResponseOutputItem>,
    pub finish_reason: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseEvent {
    OutputTextDelta {
        id: String,
        delta: String,
    },
    ReasoningDelta {
        id: String,
        delta: String,
    },
    ResponseCompleted {
        id: String,
        output: Vec<ResponseOutputItem>,
        finish_reason: String,
        usage: Usage,
    },
    ResponseError {
        id: String,
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponseOutputText {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponseReasoningSummary {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseOutputItem {
    Message {
        id: String,
        role: String,
        content: Vec<ResponseOutputText>,
    },
    Reasoning {
        id: String,
        summary: Vec<ResponseReasoningSummary>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        content: Vec<Value>,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_details: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ChatCompletionsResponse {
    pub id: String,
    pub object: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Usage,
}

impl ChatCompletionsRequest {
    pub fn into_responses_request(self) -> ResponsesRequest {
        let input = self
            .messages
            .into_iter()
            .map(|m| format!("{}:{}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        ResponsesRequest {
            model: self.model,
            instructions: None,
            previous_response_id: None,
            input: ResponsesInput::Text(input),
            parallel_tool_calls: None,
            stream: self.stream,
            reasoning: self.reasoning,
            store: None,
            include: None,
            service_tier: None,
            prompt_cache_key: None,
            text: None,
            tools: None,
            tool_choice: None,
        }
    }
}

impl ChatCompletionsResponse {
    pub fn from_responses(response: ResponsesResponse) -> Self {
        let mut content = String::new();
        let mut reasoning = None;
        let mut reasoning_details = None;
        let mut tool_calls = Vec::new();

        for item in &response.output {
            match item {
                ResponseOutputItem::Message { content: parts, .. } => {
                    if let Some(first) = parts.first() {
                        content = first.text.clone();
                    }
                }
                ResponseOutputItem::Reasoning { summary, content: details, .. } => {
                    if let Some(first) = summary.first() {
                        reasoning = Some(first.text.clone());
                    }
                    if !details.is_empty() {
                        reasoning_details = Some(details.clone());
                    }
                }
                ResponseOutputItem::FunctionCall { call_id, name, arguments, .. } => {
                    tool_calls.push(ToolCall {
                        id: call_id.clone(),
                        kind: "function".to_string(),
                        function: ToolFunction { name: name.clone(), arguments: arguments.clone() },
                    });
                }
            }
        }

        Self {
            id: response.id,
            object: "chat.completion".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content,
                    reasoning: reasoning.clone(),
                    reasoning_content: reasoning,
                    reasoning_details,
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                },
                finish_reason: response.finish_reason,
            }],
            usage: response.usage,
        }
    }
}

fn flatten_response_items(items: &[ResponseInputItem]) -> String {
    items.iter().filter_map(flatten_response_item).collect::<Vec<_>>().join("\n")
}

fn flatten_response_item(item: &ResponseInputItem) -> Option<String> {
    let kind = item.kind.as_deref().unwrap_or_default();
    if kind == "message" || item.role.is_some() {
        let role = item.role.as_deref().unwrap_or("user");
        let content = extract_item_text(item)?;
        return Some(format!("{role}:{content}"));
    }
    if kind == "function_call_output" {
        let content = item
            .output
            .as_ref()
            .and_then(ResponseToolOutput::to_text_lossy)
            .or_else(|| extract_content_text(item.content.as_ref()))
            .or_else(|| item.text.as_deref().map(str::trim).map(str::to_string))?;
        if let Some(call_id) = item.call_id.as_deref()
            && !call_id.trim().is_empty()
        {
            return Some(format!("tool:{call_id}:{content}"));
        }
        return Some(format!("tool:{content}"));
    }
    if kind == "function_call" {
        let name = item.name.as_deref().unwrap_or("function");
        let arguments = item.arguments.as_deref().unwrap_or("");
        if arguments.trim().is_empty() {
            return Some(format!("assistant_function_call:{name}"));
        }
        return Some(format!("assistant_function_call:{name}:{arguments}"));
    }
    if kind == "custom_tool_call" {
        let name = item.name.as_deref().unwrap_or("custom_tool");
        let input = item.input.as_deref().unwrap_or("");
        if input.trim().is_empty() {
            return Some(format!("assistant_custom_tool_call:{name}"));
        }
        return Some(format!("assistant_custom_tool_call:{name}:{input}"));
    }
    if kind == "custom_tool_call_output" || kind == "mcp_tool_call_output" {
        let content = item
            .output
            .as_ref()
            .and_then(ResponseToolOutput::to_text_lossy)
            .or_else(|| extract_content_text(item.content.as_ref()))
            .or_else(|| item.text.as_deref().map(str::trim).map(str::to_string))?;
        if let Some(call_id) = item.call_id.as_deref()
            && !call_id.trim().is_empty()
        {
            return Some(format!("tool:{call_id}:{content}"));
        }
        return Some(format!("tool:{content}"));
    }
    if kind == "reasoning" {
        return item
            .summary
            .as_ref()
            .and_then(|summary| extract_summary_text(summary))
            .or_else(|| item.content.as_ref().and_then(ResponseInputContent::to_text))
            .map(|content| format!("assistant_reasoning:{content}"));
    }
    if kind == "tool_search_call" {
        let execution = item.execution.as_deref().unwrap_or("").trim();
        if !execution.is_empty() {
            return Some(format!("assistant_tool_search_call:{execution}"));
        }
    }
    if kind == "tool_search_output" {
        let tools = item
            .tools
            .as_ref()
            .and_then(|tools| serde_json::to_string(tools).ok())
            .filter(|value| !value.is_empty())?;
        return Some(format!("tool_search_output:{tools}"));
    }
    extract_item_text(item)
}

fn extract_item_text(item: &ResponseInputItem) -> Option<String> {
    item.text
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .or_else(|| extract_content_text(item.content.as_ref()))
}

fn extract_content_text(content: Option<&ResponseInputContent>) -> Option<String> {
    content.and_then(ResponseInputContent::to_text)
}

fn flatten_response_input_parts(parts: &[ResponseInputPart]) -> Option<String> {
    let merged = parts
        .iter()
        .filter_map(|part| {
            part.input_text
                .as_deref()
                .or(part.output_text.as_deref())
                .or(part.text.as_deref())
                .or(part.value.as_deref())
                .map(str::trim)
                .filter(|text| !text.is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n");
    if merged.is_empty() { None } else { Some(merged) }
}

fn extract_summary_text(summary: &[Value]) -> Option<String> {
    let merged = summary
        .iter()
        .filter_map(|item| {
            item.get("text").and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n");
    if merged.is_empty() { None } else { Some(merged) }
}

fn serialize_json_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() { None } else { Some(text.to_string()) }
        }
        _ => serde_json::to_string(value).ok().filter(|text| !text.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_input_deserializes_text_variant() {
        let request: ResponsesRequest = serde_json::from_str(
            r#"{"model":"deepseek/deepseek-chat","input":"hello","stream":false}"#,
        )
        .expect("request must deserialize");
        assert_eq!(request.input, ResponsesInput::Text("hello".to_string()));
        assert_eq!(request.input.to_canonical_text(), "hello");
    }

    #[test]
    fn responses_input_deserializes_message_items_variant() {
        let request: ResponsesRequest = serde_json::from_str(
            r#"{"model":"deepseek/deepseek-chat","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"привет"}]}],"stream":false}"#,
        )
        .expect("request must deserialize");
        assert_eq!(request.input.to_canonical_text(), "user:привет");
    }

    #[test]
    fn responses_input_flattens_function_call_output_items() {
        let request: ResponsesRequest = serde_json::from_str(
            r#"{"model":"deepseek/deepseek-chat","input":[{"type":"function_call_output","call_id":"call_123","output":"{\"ok\":true}"}],"stream":false}"#,
        )
        .expect("request must deserialize");
        assert_eq!(request.input.to_canonical_text(), "tool:call_123:{\"ok\":true}");
    }

    #[test]
    fn responses_input_preserves_structured_function_call_output_parts() {
        let request: ResponsesRequest = serde_json::from_str(
            r#"{"model":"deepseek/deepseek-chat","input":[{"type":"function_call_output","call_id":"call_123","output":[{"type":"input_text","text":"line 1"},{"type":"input_text","text":"line 2"}]}],"stream":false}"#,
        )
        .expect("request must deserialize");

        let ResponsesInput::Items(items) = &request.input else {
            panic!("expected items input");
        };
        let output = items[0].output.as_ref().expect("output");
        assert_eq!(
            output,
            &ResponseToolOutput::Parts(vec![
                ResponseInputPart {
                    kind: Some("input_text".to_string()),
                    text: Some("line 1".to_string()),
                    input_text: None,
                    output_text: None,
                    image_url: None,
                    detail: None,
                    value: None,
                    extra: Default::default(),
                },
                ResponseInputPart {
                    kind: Some("input_text".to_string()),
                    text: Some("line 2".to_string()),
                    input_text: None,
                    output_text: None,
                    image_url: None,
                    detail: None,
                    value: None,
                    extra: Default::default(),
                },
            ])
        );
        assert_eq!(request.input.to_canonical_text(), "tool:call_123:line 1\nline 2");
    }

    #[test]
    fn responses_input_preserves_json_function_call_output() {
        let request: ResponsesRequest = serde_json::from_str(
            r#"{"model":"deepseek/deepseek-chat","input":[{"type":"function_call_output","call_id":"call_123","output":{"ok":true,"count":2}}],"stream":false}"#,
        )
        .expect("request must deserialize");

        let ResponsesInput::Items(items) = &request.input else {
            panic!("expected items input");
        };
        assert_eq!(
            items[0].output,
            Some(ResponseToolOutput::Json(serde_json::json!({"ok": true, "count": 2})))
        );
        assert_eq!(request.input.to_canonical_text(), "tool:call_123:{\"count\":2,\"ok\":true}");
    }

    #[test]
    fn responses_request_deserializes_full_codex_style_contract() {
        let request: ResponsesRequest = serde_json::from_str(
            r#"{
                "model":"deepseek-chat",
                "instructions":"base instructions",
                "previous_response_id":"resp_prev_1",
                "input":[
                    {
                        "type":"message",
                        "role":"user",
                        "content":[
                            {"type":"input_text","text":"hello"},
                            {"type":"input_image","image_url":"https://example.com/cat.png","detail":"high"}
                        ]
                    },
                    {
                        "type":"message",
                        "role":"assistant",
                        "content":[{"type":"output_text","text":"working on it"}],
                        "phase":"commentary"
                    },
                    {
                        "type":"reasoning",
                        "summary":[{"type":"summary_text","text":"checked workspace"}],
                        "encrypted_content":"secret"
                    },
                    {
                        "type":"function_call",
                        "call_id":"call_1",
                        "name":"list_dir",
                        "arguments":"{\"dir_path\":\"/workspace\"}"
                    },
                    {
                        "type":"function_call_output",
                        "call_id":"call_1",
                        "output":[{"type":"input_text","text":"Absolute path: /workspace"}]
                    },
                    {
                        "type":"custom_tool_call_output",
                        "call_id":"call_2",
                        "output":"patch applied"
                    }
                ],
                "parallel_tool_calls":true,
                "reasoning":{"effort":"medium","summary":"auto"},
                "store":false,
                "stream":true,
                "include":["reasoning.encrypted_content"],
                "service_tier":"priority",
                "prompt_cache_key":"conv_123",
                "text":{"verbosity":"high","format":{"type":"json_schema","strict":true,"schema":{"type":"object"},"name":"codex_output_schema"}},
                "tools":[{"type":"function","function":{"name":"list_dir","parameters":{"type":"object"}}}],
                "tool_choice":"auto"
            }"#,
        )
        .expect("full codex-style request must deserialize");

        assert_eq!(request.instructions.as_deref(), Some("base instructions"));
        assert_eq!(request.previous_response_id.as_deref(), Some("resp_prev_1"));
        assert_eq!(request.parallel_tool_calls, Some(true));
        assert_eq!(request.store, Some(false));
        assert_eq!(
            request.include.as_ref().expect("include"),
            &vec!["reasoning.encrypted_content".to_string()]
        );
        assert_eq!(request.service_tier.as_deref(), Some("priority"));
        assert_eq!(request.prompt_cache_key.as_deref(), Some("conv_123"));
        assert_eq!(
            request.text.as_ref().and_then(|text| text.verbosity.as_ref()),
            Some(&TextVerbosity::High)
        );
        let ResponsesInput::Items(items) = &request.input else {
            panic!("expected item input");
        };
        assert_eq!(items.len(), 6);
        assert_eq!(
            items[0].content.as_ref().and_then(ResponseInputContent::to_text).as_deref(),
            Some("hello")
        );
        assert_eq!(
            items[2].summary.as_ref().and_then(|summary| extract_summary_text(summary)).as_deref(),
            Some("checked workspace")
        );
        assert_eq!(
            items[4].output.as_ref().and_then(ResponseToolOutput::to_text_lossy).as_deref(),
            Some("Absolute path: /workspace")
        );
        assert_eq!(
            request.input.to_canonical_text(),
            "user:hello\nassistant:working on it\nassistant_reasoning:checked workspace\nassistant_function_call:list_dir:{\"dir_path\":\"/workspace\"}\ntool:call_1:Absolute path: /workspace\ntool:call_2:patch applied"
        );
    }
}
