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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(untagged)]
pub enum ResponseInputContent {
    Text(String),
    Parts(Vec<ResponseInputPart>),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponseInputPart {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
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
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
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
    pub input: ResponsesInput,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
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
            input: ResponsesInput::Text(input),
            stream: self.stream,
            reasoning: self.reasoning,
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
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
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
    match content? {
        ResponseInputContent::Text(text) => {
            let text = text.trim();
            if text.is_empty() { None } else { Some(text.to_string()) }
        }
        ResponseInputContent::Parts(parts) => {
            let merged = parts
                .iter()
                .filter_map(|part| {
                    part.input_text
                        .as_deref()
                        .or(part.output_text.as_deref())
                        .or(part.text.as_deref())
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                })
                .collect::<String>();
            if merged.is_empty() { None } else { Some(merged) }
        }
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
}
