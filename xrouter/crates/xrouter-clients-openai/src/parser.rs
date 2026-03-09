use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use tracing::warn;
use uuid::Uuid;
use xrouter_contracts::{ToolCall, ToolFunction};
use xrouter_core::{CoreError, ProviderOutcome};

pub fn map_chat_completion_response(
    payload: ChatCompletionsResponse,
) -> Result<ProviderOutcome, CoreError> {
    let first = payload
        .choices
        .first()
        .ok_or_else(|| CoreError::Provider("provider returned empty choices".to_string()))?;

    let content = extract_message_content(&first.message.content).unwrap_or_default();
    let tool_calls = first
        .message
        .tool_calls
        .as_ref()
        .map(|calls| map_provider_tool_calls(calls))
        .filter(|calls| !calls.is_empty());
    if content.is_empty() && tool_calls.is_none() {
        return Err(CoreError::Provider("provider returned empty message content".to_string()));
    }

    let output_tokens =
        payload.usage.and_then(|usage| usage.completion_tokens).unwrap_or_else(|| {
            if content.is_empty() { 0 } else { content.split_whitespace().count() as u32 }
        });

    let reasoning_details = first.message.reasoning_details.clone();
    let reasoning = first
        .message
        .reasoning_content
        .clone()
        .or_else(|| first.message.reasoning.clone())
        .or_else(|| {
            reasoning_details.as_ref().and_then(|details| extract_reasoning_from_details(details))
        });

    let chunks = if content.is_empty() { Vec::new() } else { vec![content] };
    Ok(ProviderOutcome {
        chunks,
        output_tokens,
        reasoning,
        reasoning_details,
        tool_calls,
        emitted_live: false,
    })
}

pub fn map_responses_api_response(
    payload: ResponsesApiResponse,
) -> Result<ProviderOutcome, CoreError> {
    let content = extract_message_text_from_responses_output(&payload.output).unwrap_or_default();
    let tool_calls = extract_tool_calls_from_responses_output(&payload.output);
    if content.is_empty() && tool_calls.is_none() {
        warn!(
            event = "provider.responses.empty_message_content",
            "provider returned empty message content for responses payload; treating as empty completed response"
        );
    }

    let reasoning_details = extract_reasoning_content_items_from_responses_output(&payload.output);
    let reasoning = extract_reasoning_text_from_responses_output(&payload.output).or_else(|| {
        reasoning_details.as_ref().and_then(|details| extract_reasoning_from_details(details))
    });

    let output_tokens = payload.usage.as_ref().map(|u| u.output_tokens).unwrap_or_else(|| {
        if content.is_empty() { 0 } else { content.split_whitespace().count() as u32 }
    });

    let chunks = if content.is_empty() { Vec::new() } else { vec![content] };
    Ok(ProviderOutcome {
        chunks,
        output_tokens,
        reasoning,
        reasoning_details,
        tool_calls,
        emitted_live: false,
    })
}

pub fn map_chat_completion_stream_text(payload: &str) -> Result<ProviderOutcome, CoreError> {
    let mut chunks = Vec::<String>::new();
    let mut all_content = String::new();
    let mut reasoning = String::new();
    let mut reasoning_details = Vec::<Value>::new();
    let mut output_tokens = None::<u32>;
    let mut tool_calls_by_index = HashMap::<usize, StreamToolCall>::new();
    let mut direct_tool_calls = Vec::<ToolCall>::new();

    for event in extract_sse_data_events(payload) {
        if event == "[DONE]" {
            continue;
        }
        let parsed: ChatCompletionsStreamChunk = serde_json::from_str(&event)
            .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;

        if let Some(usage) = parsed.usage.and_then(|usage| usage.completion_tokens) {
            output_tokens = Some(usage);
        }

        for choice in parsed.choices {
            if let Some(content_delta) = extract_message_content(&choice.delta.content)
                && !content_delta.is_empty()
            {
                all_content.push_str(&content_delta);
                chunks.push(content_delta);
            }
            if let Some(message) = choice.message.as_ref() {
                if let Some(content) = extract_message_content(&message.content)
                    && !content.is_empty()
                {
                    all_content.push_str(&content);
                    chunks.push(content);
                }
                if let Some(tool_calls) = message.tool_calls.as_ref() {
                    direct_tool_calls.extend(map_provider_tool_calls(tool_calls));
                }
            }

            if let Some(text) = choice.delta.reasoning_content.or(choice.delta.reasoning)
                && !text.trim().is_empty()
            {
                reasoning.push_str(&text);
            }

            if let Some(details) = choice.delta.reasoning_details {
                reasoning_details.extend(details);
            }

            for tool_delta in choice
                .delta
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .chain(choice.tool_calls.unwrap_or_default())
            {
                let index = tool_delta.index.unwrap_or(tool_calls_by_index.len());
                let entry = tool_calls_by_index.entry(index).or_default();
                if let Some(id) = tool_delta.id.filter(|v| !v.trim().is_empty()) {
                    entry.id = Some(id);
                }
                if let Some(kind) = tool_delta.kind.filter(|v| !v.trim().is_empty()) {
                    entry.kind = Some(kind);
                }
                if let Some(function) = tool_delta.function {
                    if let Some(name) = function.name.filter(|v| !v.trim().is_empty()) {
                        entry.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        entry.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    let mut tool_calls = finalize_stream_tool_calls(tool_calls_by_index);
    if !direct_tool_calls.is_empty() {
        if let Some(existing) = tool_calls.as_mut() {
            existing.extend(direct_tool_calls);
        } else {
            tool_calls = Some(direct_tool_calls);
        }
    }
    let reasoning = if reasoning.trim().is_empty() { None } else { Some(reasoning) };
    let reasoning_details =
        if reasoning_details.is_empty() { None } else { Some(reasoning_details) };
    let output_tokens = output_tokens.unwrap_or_else(|| {
        if all_content.is_empty() { 0 } else { all_content.split_whitespace().count() as u32 }
    });

    if all_content.is_empty() && tool_calls.is_none() {
        warn!(
            event = "provider.responses.stream.empty_message_content",
            "provider returned empty message content in responses stream; treating as empty completed response"
        );
    }

    let final_chunks = if all_content.is_empty() { Vec::new() } else { chunks };
    Ok(ProviderOutcome {
        chunks: final_chunks,
        output_tokens,
        reasoning,
        reasoning_details,
        tool_calls,
        emitted_live: false,
    })
}

pub fn map_responses_stream_text(payload: &str) -> Result<ProviderOutcome, CoreError> {
    let mut chunks = Vec::<String>::new();
    let mut all_content = String::new();
    let mut tool_calls = Vec::<ToolCall>::new();

    for event in extract_sse_data_events(payload) {
        if event == "[DONE]" {
            continue;
        }
        let parsed: ResponsesStreamEvent = serde_json::from_str(&event)
            .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;

        if parsed.kind == "response.output_text.delta"
            && let Some(delta) = parsed.delta.or(parsed.text)
            && !delta.is_empty()
        {
            all_content.push_str(&delta);
            chunks.push(delta);
            continue;
        }

        if parsed.kind == "response.output_item.added"
            && let Some(item) = parsed.item
            && item.kind == "function_call"
            && let Some(call_id) = item.call_id.as_deref()
            && let Some(name) = item.name.as_deref()
            && !call_id.trim().is_empty()
            && !name.trim().is_empty()
        {
            tool_calls.push(ToolCall {
                id: call_id.to_string(),
                kind: "function".to_string(),
                function: ToolFunction {
                    name: name.to_string(),
                    arguments: item.arguments.unwrap_or_else(|| "{}".to_string()),
                },
            });
            continue;
        }

        if ((parsed.kind == "response.completed") || parsed.kind.is_empty())
            && let Some(response) = parsed.response
        {
            let mut mapped = map_responses_api_response(response)?;
            if !all_content.is_empty() && mapped.chunks.is_empty() {
                mapped.chunks = chunks.clone();
            }
            if mapped.tool_calls.is_none() && !tool_calls.is_empty() {
                mapped.tool_calls = Some(tool_calls.clone());
            }
            return Ok(mapped);
        }
    }

    let tool_calls = if tool_calls.is_empty() { None } else { Some(tool_calls) };
    let output_tokens =
        if all_content.is_empty() { 0 } else { all_content.split_whitespace().count() as u32 };

    if all_content.is_empty() && tool_calls.is_none() {
        warn!(
            event = "provider.responses.stream.empty_message_content.tail",
            "provider returned empty message content in responses stream tail; treating as empty completed response"
        );
    }

    Ok(ProviderOutcome {
        chunks: if all_content.is_empty() { Vec::new() } else { chunks },
        output_tokens,
        reasoning: None,
        reasoning_details: None,
        tool_calls,
        emitted_live: false,
    })
}

fn extract_sse_data_events(payload: &str) -> Vec<String> {
    let mut owned = payload.replace('\r', "");
    drain_sse_frames(&mut owned, true)
        .into_iter()
        .filter_map(|frame| sse_frame_to_data(&frame))
        .collect()
}

pub fn drain_sse_frames(buffer: &mut String, flush_tail: bool) -> Vec<String> {
    let mut frames = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let frame = buffer[..idx].to_string();
        buffer.replace_range(..idx + 2, "");
        frames.push(frame);
    }
    if flush_tail {
        let tail = buffer.trim();
        if !tail.is_empty() {
            frames.push(tail.to_string());
            buffer.clear();
        }
    }
    frames
}

fn sse_frame_to_data(frame: &str) -> Option<String> {
    let mut data_lines = Vec::<String>::new();
    for line in frame.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }
    if data_lines.is_empty() { None } else { Some(data_lines.join("\n")) }
}

pub fn extract_chat_delta_chunks(frame: &str, _request_id: &str) -> Result<Vec<String>, CoreError> {
    let Some(data) = sse_frame_to_data(frame) else {
        return Ok(Vec::new());
    };
    if data == "[DONE]" {
        return Ok(Vec::new());
    }
    let parsed: ChatCompletionsStreamChunk = serde_json::from_str(&data)
        .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
    let mut chunks = Vec::new();
    for choice in parsed.choices {
        if let Some(content_delta) = extract_message_content(&choice.delta.content)
            && !content_delta.is_empty()
        {
            chunks.push(content_delta);
        }
    }
    Ok(chunks)
}

pub fn extract_chat_reasoning_delta(
    frame: &str,
    _request_id: &str,
) -> Result<Option<String>, CoreError> {
    let Some(data) = sse_frame_to_data(frame) else {
        return Ok(None);
    };
    if data == "[DONE]" {
        return Ok(None);
    }
    let parsed: ChatCompletionsStreamChunk = serde_json::from_str(&data)
        .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
    let text = parsed
        .choices
        .into_iter()
        .filter_map(|choice| choice.delta.reasoning_content.or(choice.delta.reasoning))
        .collect::<String>();
    if text.trim().is_empty() { Ok(None) } else { Ok(Some(text)) }
}

pub fn extract_responses_text_delta(frame: &str) -> Result<Option<String>, CoreError> {
    let Some(data) = sse_frame_to_data(frame) else {
        return Ok(None);
    };
    if data == "[DONE]" {
        return Ok(None);
    }
    let parsed: ResponsesStreamEvent = serde_json::from_str(&data)
        .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
    if parsed.kind == "response.output_text.delta" {
        return Ok(parsed.delta.or(parsed.text));
    }
    Ok(None)
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionsResponse {
    pub(crate) choices: Vec<Choice>,
    #[serde(default)]
    pub(crate) usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Choice {
    pub(crate) message: Message,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Message {
    #[serde(default)]
    pub(crate) content: Value,
    #[serde(default)]
    pub(crate) reasoning: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_content: Option<String>,
    #[serde(default)]
    pub(crate) reasoning_details: Option<Vec<Value>>,
    #[serde(default)]
    pub(crate) tool_calls: Option<Vec<ProviderToolCall>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Usage {
    #[serde(default)]
    pub(crate) completion_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ResponsesApiResponse {
    #[serde(default)]
    pub(crate) output: Vec<ResponsesApiOutputItem>,
    #[serde(default)]
    pub(crate) usage: Option<ResponsesApiUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponsesApiUsage {
    #[serde(default)]
    pub(crate) output_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponsesApiOutputItem {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) content: Option<Vec<Value>>,
    #[serde(default)]
    pub(crate) summary: Option<Vec<ResponsesApiSummary>>,
    #[serde(default)]
    pub(crate) call_id: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsStreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamMessageDelta,
    #[serde(default)]
    tool_calls: Option<Vec<ProviderToolCallDelta>>,
    #[serde(default)]
    message: Option<Message>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamMessageDelta {
    #[serde(default)]
    content: Value,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Option<Vec<Value>>,
    #[serde(default)]
    tool_calls: Option<Vec<ProviderToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ProviderToolCallDelta {
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    function: Option<ProviderToolFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ProviderToolFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct StreamToolCall {
    id: Option<String>,
    kind: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[cfg(test)]
mod tests {
    use super::{
        ChatCompletionsResponse, Choice, Message, ProviderToolCall, ProviderToolFunction,
        ResponsesApiOutputItem, ResponsesApiResponse, ResponsesApiUsage, Usage,
        extract_reasoning_from_details, map_chat_completion_response,
        map_chat_completion_stream_text, map_responses_api_response, map_responses_stream_text,
    };
    use serde_json::{Value, json};
    use xrouter_contracts::{ToolCall, ToolFunction};

    #[test]
    fn map_chat_completion_response_accepts_tool_only_message() {
        let payload = ChatCompletionsResponse {
            choices: vec![Choice {
                message: Message {
                    content: Value::String(String::new()),
                    reasoning: None,
                    reasoning_content: None,
                    reasoning_details: None,
                    tool_calls: Some(vec![ProviderToolCall {
                        id: Some("call_1".to_string()),
                        kind: Some("function".to_string()),
                        function: Some(ProviderToolFunction {
                            name: Some("read_file".to_string()),
                            arguments: Some("{\"path\":\"README.md\"}".to_string()),
                        }),
                    }]),
                },
            }],
            usage: Some(Usage { completion_tokens: Some(7) }),
        };

        let outcome = map_chat_completion_response(payload).expect("tool-only completion is valid");
        assert!(outcome.chunks.is_empty());
        assert_eq!(outcome.output_tokens, 7);
        assert_eq!(
            outcome.tool_calls,
            Some(vec![ToolCall {
                id: "call_1".to_string(),
                kind: "function".to_string(),
                function: ToolFunction {
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"README.md\"}".to_string(),
                },
            }])
        );
    }

    #[test]
    fn reasoning_details_summary_is_extracted() {
        let details = vec![json!({
            "type": "reasoning.summary",
            "summary": "A concise summary"
        })];
        assert_eq!(extract_reasoning_from_details(&details), Some("A concise summary".to_string()));
    }

    #[test]
    fn reasoning_details_text_and_summary_are_joined() {
        let details = vec![
            json!({
                "type": "reasoning.summary",
                "summary": "Summary"
            }),
            json!({
                "type": "reasoning.text",
                "text": "Detailed chain"
            }),
        ];
        assert_eq!(
            extract_reasoning_from_details(&details),
            Some("Summary\nDetailed chain".to_string())
        );
    }

    #[test]
    fn chat_sse_with_delta_only_is_not_empty() {
        let sse = concat!(
            "event: message\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome = map_chat_completion_stream_text(sse).expect("delta-only SSE must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
        assert!(outcome.tool_calls.is_none());
    }

    #[test]
    fn responses_sse_with_delta_only_is_not_empty() {
        let sse = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"output_tokens\":1}}}\n\n"
        );
        let outcome = map_responses_stream_text(sse).expect("responses SSE must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn responses_sse_without_type_but_with_response_object_is_not_empty() {
        let sse = concat!(
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"output_tokens\":1}}}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome =
            map_responses_stream_text(sse).expect("responses fallback payload must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
        assert_eq!(outcome.output_tokens, 1);
    }

    #[test]
    fn chat_sse_without_trailing_separator_is_not_empty() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"index\":0,\"finish_reason\":null}]}";
        let outcome = map_chat_completion_stream_text(sse).expect("SSE tail frame must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn responses_sse_without_trailing_separator_is_not_empty() {
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}";
        let outcome = map_responses_stream_text(sse).expect("SSE tail frame must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn responses_sse_with_empty_completed_payload_is_fail_soft() {
        let sse = concat!(
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[],\"usage\":{\"output_tokens\":0}}}\n\n"
        );
        let outcome = map_responses_stream_text(sse)
            .expect("empty completed responses payload must not fail");
        assert!(outcome.chunks.is_empty());
        assert_eq!(outcome.output_tokens, 0);
        assert!(outcome.tool_calls.is_none());
    }

    #[test]
    fn responses_message_text_skips_empty_parts_and_joins_non_empty() {
        let payload = ResponsesApiResponse {
            output: vec![ResponsesApiOutputItem {
                kind: "message".to_string(),
                content: Some(vec![
                    json!({"type":"output_text","text":""}),
                    json!({"type":"output_text","text":"hello"}),
                    json!({"type":"output_text","text":" world"}),
                ]),
                summary: None,
                call_id: None,
                name: None,
                arguments: None,
            }],
            usage: Some(ResponsesApiUsage { output_tokens: 2 }),
        };
        let outcome = map_responses_api_response(payload).expect("message text must be extracted");
        assert_eq!(outcome.chunks.join(""), "helloworld");
    }

    #[test]
    fn chat_sse_with_choice_level_tool_calls_is_not_empty() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{},\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Kyiv\\\"}\"}}],\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome =
            map_chat_completion_stream_text(sse).expect("choice-level tool_calls must parse");
        let tool_calls = outcome.tool_calls.expect("tool calls must be present");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"Kyiv\"}");
    }

    #[test]
    fn chat_sse_with_message_level_tool_calls_is_not_empty() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{},\"message\":{\"role\":\"assistant\",\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Kyiv\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome =
            map_chat_completion_stream_text(sse).expect("message-level tool_calls must parse");
        let tool_calls = outcome.tool_calls.expect("tool calls must be present");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"Kyiv\"}");
    }
}

#[derive(Debug, Deserialize)]
struct ResponsesStreamEvent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    item: Option<ResponsesApiOutputItem>,
    #[serde(default)]
    response: Option<ResponsesApiResponse>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponsesApiSummary {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProviderToolCall {
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(rename = "type", default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) function: Option<ProviderToolFunction>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProviderToolFunction {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) arguments: Option<String>,
}

fn map_provider_tool_calls(tool_calls: &[ProviderToolCall]) -> Vec<ToolCall> {
    tool_calls
        .iter()
        .filter_map(|call| {
            let function = call.function.as_ref()?;
            let name = function.name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let arguments = function.arguments.clone().unwrap_or_else(|| "{}".to_string());
            let call_id =
                call.id.clone().unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
            Some(ToolCall {
                id: call_id,
                kind: call.kind.clone().unwrap_or_else(|| "function".to_string()),
                function: ToolFunction { name: name.to_string(), arguments },
            })
        })
        .collect()
}

fn finalize_stream_tool_calls(by_index: HashMap<usize, StreamToolCall>) -> Option<Vec<ToolCall>> {
    let mut sorted = by_index.into_iter().collect::<Vec<_>>();
    sorted.sort_by_key(|(idx, _)| *idx);
    let calls = sorted
        .into_iter()
        .filter_map(|(_, call)| {
            let name = call.name?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let arguments =
                if call.arguments.trim().is_empty() { "{}".to_string() } else { call.arguments };
            Some(ToolCall {
                id: call.id.unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple())),
                kind: call.kind.unwrap_or_else(|| "function".to_string()),
                function: ToolFunction { name, arguments },
            })
        })
        .collect::<Vec<_>>();
    if calls.is_empty() { None } else { Some(calls) }
}

fn extract_tool_calls_from_responses_output(
    output: &[ResponsesApiOutputItem],
) -> Option<Vec<ToolCall>> {
    let calls = output
        .iter()
        .filter(|item| item.kind == "function_call")
        .filter_map(|item| {
            let call_id = item.call_id.as_deref()?.trim();
            let name = item.name.as_deref()?.trim();
            if call_id.is_empty() || name.is_empty() {
                return None;
            }
            let arguments = item.arguments.clone().unwrap_or_else(|| "{}".to_string());
            Some(ToolCall {
                id: call_id.to_string(),
                kind: "function".to_string(),
                function: ToolFunction { name: name.to_string(), arguments },
            })
        })
        .collect::<Vec<_>>();
    if calls.is_empty() { None } else { Some(calls) }
}

fn extract_message_content(content: &Value) -> Option<String> {
    match content {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

pub(crate) fn extract_reasoning_from_details(details: &[Value]) -> Option<String> {
    let text = details
        .iter()
        .filter_map(|detail| {
            let kind = detail.get("type").and_then(Value::as_str)?;
            match kind {
                "reasoning.summary" => detail.get("summary").and_then(Value::as_str),
                "reasoning.text" => detail.get("text").and_then(Value::as_str),
                _ => None,
            }
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if text.is_empty() { None } else { Some(text) }
}

fn extract_message_text_from_responses_output(output: &[ResponsesApiOutputItem]) -> Option<String> {
    let text = output
        .iter()
        .filter(|item| item.kind == "message")
        .filter_map(|item| item.content.as_ref())
        .flat_map(|parts| parts.iter())
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.get("output_text").and_then(Value::as_str))
                .or_else(|| part.get("input_text").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("");
    if text.is_empty() { None } else { Some(text) }
}

fn extract_reasoning_text_from_responses_output(
    output: &[ResponsesApiOutputItem],
) -> Option<String> {
    output.iter().find_map(|item| {
        if item.kind != "reasoning" {
            return None;
        }
        let summary = item
            .summary
            .as_ref()
            .and_then(|values| values.first())
            .map(|value| value.text.trim().to_string())
            .filter(|value| !value.is_empty());
        if summary.is_some() {
            return summary;
        }
        item.content.as_ref().and_then(|details| extract_reasoning_from_details(details))
    })
}

fn extract_reasoning_content_items_from_responses_output(
    output: &[ResponsesApiOutputItem],
) -> Option<Vec<Value>> {
    output.iter().find_map(|item| {
        if item.kind != "reasoning" {
            return None;
        }
        item.content.clone().filter(|items| !items.is_empty())
    })
}
