use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageName {
    Ingest,
    Tokenize,
    Hold,
    Generate,
    Finalize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponsesRequest {
    pub model: String,
    pub input: String,
    #[serde(default)]
    pub stream: bool,
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
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ResponseReasoningSummary {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseOutputItem {
    Message { id: String, role: String, content: Vec<ResponseOutputText> },
    Reasoning { id: String, summary: Vec<ResponseReasoningSummary> },
    FunctionCall { id: String, call_id: String, name: String, arguments: String },
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
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
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

        ResponsesRequest { model: self.model, input, stream: self.stream }
    }
}

impl ChatCompletionsResponse {
    pub fn from_responses(response: ResponsesResponse) -> Self {
        let mut content = String::new();
        let mut reasoning = None;
        let mut tool_calls = Vec::new();

        for item in &response.output {
            match item {
                ResponseOutputItem::Message { content: parts, .. } => {
                    if let Some(first) = parts.first() {
                        content = first.text.clone();
                    }
                }
                ResponseOutputItem::Reasoning { summary, .. } => {
                    if let Some(first) = summary.first() {
                        reasoning = Some(first.text.clone());
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
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                },
                finish_reason: response.finish_reason,
            }],
            usage: response.usage,
        }
    }
}
