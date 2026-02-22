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
    pub status: String,
    pub output_text: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseEvent {
    OutputTextDelta { id: String, delta: String },
    ResponseCompleted { id: String, usage: Usage },
    ResponseError { id: String, message: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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
        Self {
            id: response.id,
            object: "chat.completion".to_string(),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".to_string(),
                    content: response.output_text,
                },
                finish_reason: "stop".to_string(),
            }],
            usage: response.usage,
        }
    }
}
