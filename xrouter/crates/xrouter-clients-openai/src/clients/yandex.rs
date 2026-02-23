use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Map, Value, json};
use tracing::{debug, info, warn};
use uuid::Uuid;
use xrouter_contracts::{
    ResponseInputContent, ResponseInputItem, ResponsesInput, ToolCall, ToolFunction,
};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome,
};

use crate::HttpRuntime;

const LEGACY_TOOL_CALL_START_MARKER: &str = "[TOOL_CALL_START]";
const LEGACY_TOOL_CALL_END_MARKER: &str = "[TOOL_CALL_END]";

pub struct YandexResponsesClient {
    runtime: HttpRuntime,
    project: Option<String>,
}

impl YandexResponsesClient {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        project: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "yandex".to_string(),
                base_url,
                api_key,
                http_client,
                max_inflight,
            ),
            project,
        }
    }
}

#[async_trait]
impl ProviderClient for YandexResponsesClient {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("responses")?;
        let upstream_model = build_yandex_upstream_model(request.model, self.project.as_deref())?;
        let (payload, normalization) = build_yandex_responses_payload(
            &upstream_model,
            request.input,
            request.tools,
            request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "yandex",
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
                provider = "yandex",
                model = request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        let mut headers = Vec::new();
        if let Some(project) = self.project.as_deref().filter(|value| !value.trim().is_empty()) {
            headers.push(("OpenAI-Project".to_string(), project.to_string()));
        }
        self.runtime.post_responses_stream("request", &url, &payload, None, &headers, None).await
    }

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("responses")?;
        let upstream_model =
            build_yandex_upstream_model(request.request.model, self.project.as_deref())?;
        let (payload, normalization) = build_yandex_responses_payload(
            &upstream_model,
            request.request.input,
            request.request.tools,
            request.request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "yandex",
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
                provider = "yandex",
                model = request.request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        let mut headers = Vec::new();
        if let Some(project) = self.project.as_deref().filter(|value| !value.trim().is_empty()) {
            headers.push(("OpenAI-Project".to_string(), project.to_string()));
        }
        self.runtime
            .post_responses_stream(
                request.request_id,
                &url,
                &payload,
                None,
                &headers,
                request.sender,
            )
            .await
    }
}

pub(crate) fn build_yandex_responses_payload(
    model: &str,
    input: &ResponsesInput,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> (Value, YandexNormalization) {
    let normalized_tools = normalize_tools_for_responses(tools);
    let normalized_tool_choice =
        normalize_tool_choice_for_responses(tool_choice, !normalized_tools.tools.is_empty());
    let sanitized_input = sanitize_yandex_input(input);
    let input_value = serde_json::to_value(&sanitized_input)
        .unwrap_or_else(|_| Value::String(sanitized_input.to_canonical_text()));
    let mut payload = json!({
        "model": model,
        "input": input_value,
        "stream": true
    });
    if let Some(obj) = payload.as_object_mut() {
        if !normalized_tools.tools.is_empty() {
            obj.insert("tools".to_string(), Value::Array(normalized_tools.tools.clone()));
        }
        if let Some(choice) = normalized_tool_choice.clone() {
            obj.insert("tool_choice".to_string(), choice);
        }
    }

    (
        payload,
        YandexNormalization {
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

fn sanitize_yandex_input(input: &ResponsesInput) -> ResponsesInput {
    let ResponsesInput::Items(items) = input else {
        return input.clone();
    };

    let mut filtered = Vec::<ResponseInputItem>::new();
    let mut pending_tool_call_id: Option<String> = None;

    for (idx, item) in items.iter().enumerate() {
        if is_function_call_item(item) {
            if let Some(call_id) = item.call_id.as_deref().map(str::trim).filter(|v| !v.is_empty())
            {
                pending_tool_call_id = Some(call_id.to_string());
            }
            filtered.push(item.clone());
            continue;
        }

        if is_function_call_output_item(item) {
            if let Some(call_id) = item_call_id(item)
                && pending_tool_call_id.as_deref() == Some(call_id.as_str())
            {
                pending_tool_call_id = None;
            }
            filtered.push(item.clone());
            continue;
        }

        if is_assistant_message(item) {
            // Python mapper behavior: skip empty assistant messages without tool calls.
            if !has_tool_calls(item) && extract_item_text(item).is_none() {
                continue;
            }

            // Python mapper behavior: skip preamble assistant message between tool call and result.
            if !has_tool_calls(item)
                && let Some(pending_call_id) = pending_tool_call_id.as_deref()
                && has_matching_tool_output_ahead(items, idx, pending_call_id)
            {
                continue;
            }
        }

        filtered.push(item.clone());
    }

    ResponsesInput::Items(filtered)
}

fn has_matching_tool_output_ahead(
    items: &[ResponseInputItem],
    current_index: usize,
    pending_call_id: &str,
) -> bool {
    for future in &items[current_index + 1..] {
        if is_function_call_output_item(future)
            && item_call_id(future).as_deref() == Some(pending_call_id)
        {
            return true;
        }
        if is_assistant_message(future) || is_user_message(future) {
            break;
        }
    }
    false
}

fn is_assistant_message(item: &ResponseInputItem) -> bool {
    item.role.as_deref() == Some("assistant")
}

fn is_user_message(item: &ResponseInputItem) -> bool {
    item.role.as_deref() == Some("user")
}

fn is_function_call_item(item: &ResponseInputItem) -> bool {
    item.kind.as_deref() == Some("function_call")
}

fn is_function_call_output_item(item: &ResponseInputItem) -> bool {
    item.kind.as_deref() == Some("function_call_output") || item.role.as_deref() == Some("tool")
}

fn item_call_id(item: &ResponseInputItem) -> Option<String> {
    item.call_id.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).or_else(
        || {
            item.extra
                .get("tool_call_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        },
    )
}

fn has_tool_calls(item: &ResponseInputItem) -> bool {
    item.extra.get("tool_calls").and_then(Value::as_array).is_some_and(|calls| !calls.is_empty())
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

pub(crate) fn build_yandex_upstream_model(
    model: &str,
    project: Option<&str>,
) -> Result<String, CoreError> {
    if model.starts_with("gpt://") {
        return Ok(model.to_string());
    }

    let project = project.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
        CoreError::Provider("provider project is not configured for yandex".to_string())
    })?;

    Ok(format!("gpt://{project}/{model}"))
}

#[derive(Debug, Clone)]
pub(crate) struct YandexNormalization {
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

fn normalize_tools_for_responses(tools: Option<&[Value]>) -> NormalizedTools {
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

fn normalize_function_tool(tool: &Value) -> Option<Value> {
    let tool_obj = tool.as_object()?;
    let kind = tool_obj.get("type").and_then(Value::as_str)?;
    if kind != "function" {
        return None;
    }

    let function_obj = tool_obj.get("function").and_then(Value::as_object);
    let name = function_obj
        .and_then(|obj| obj.get("name").and_then(Value::as_str))
        .or_else(|| tool_obj.get("name").and_then(Value::as_str))?
        .trim();
    if name.is_empty() {
        return None;
    }

    let description = function_obj
        .and_then(|obj| obj.get("description").and_then(Value::as_str))
        .or_else(|| tool_obj.get("description").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let parameters = function_obj
        .and_then(|obj| obj.get("parameters"))
        .cloned()
        .or_else(|| function_obj.and_then(|obj| obj.get("input_schema")).cloned())
        .or_else(|| tool_obj.get("parameters").cloned())
        .or_else(|| tool_obj.get("input_schema").cloned())
        .or_else(|| function_obj.and_then(|obj| obj.get("schema")).cloned())
        .or_else(|| tool_obj.get("schema").cloned())
        .unwrap_or_else(|| json!({"type":"object","properties":{}}));
    let strict = function_obj
        .and_then(|obj| obj.get("strict"))
        .cloned()
        .or_else(|| tool_obj.get("strict").cloned());

    let mut normalized = Map::new();
    normalized.insert("type".to_string(), Value::String("function".to_string()));
    normalized.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = description {
        normalized.insert("description".to_string(), Value::String(description));
    }
    normalized.insert("parameters".to_string(), parameters);
    if let Some(strict) = strict {
        normalized.insert("strict".to_string(), strict);
    }
    Some(Value::Object(normalized))
}

fn normalize_tool_choice_for_responses(
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
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| {
                    obj.get("function")
                        .and_then(Value::as_object)
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                })?
                .trim();
            if name.is_empty() {
                return None;
            }
            Some(json!({"type":"function","name":name}))
        }
        "tool" => {
            let name = obj.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty() {
                return None;
            }
            Some(json!({"type":"function","name":name}))
        }
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

pub(crate) fn map_yandex_responses_stream_text(
    payload: &str,
) -> Result<ProviderOutcome, CoreError> {
    let mut chunks = Vec::<String>::new();
    let mut all_content = String::new();
    let mut tool_calls = Vec::<ToolCall>::new();

    for event in extract_sse_data_events(payload) {
        if event == "[DONE]" {
            continue;
        }
        let parsed: Value = serde_json::from_str(&event)
            .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
        let kind = parsed.get("type").and_then(Value::as_str).unwrap_or_default();

        if kind == "response.output_text.delta"
            && let Some(delta) = parsed
                .get("delta")
                .and_then(Value::as_str)
                .or_else(|| parsed.get("text").and_then(Value::as_str))
            && !delta.is_empty()
        {
            all_content.push_str(delta);
            chunks.push(delta.to_string());
            continue;
        }

        if kind == "response.output_item.added"
            && let Some(item) = parsed.get("item").and_then(Value::as_object)
            && item.get("type").and_then(Value::as_str) == Some("function_call")
            && let Some(call_id) = item.get("call_id").and_then(Value::as_str).map(str::trim)
            && let Some(name) = item.get("name").and_then(Value::as_str).map(str::trim)
            && !call_id.is_empty()
            && !name.is_empty()
        {
            tool_calls.push(ToolCall {
                id: call_id.to_string(),
                kind: "function".to_string(),
                function: ToolFunction {
                    name: name.to_string(),
                    arguments: item
                        .get("arguments")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| "{}".to_string()),
                },
            });
            continue;
        }

        if kind == "response.completed"
            && let Some(response) = parsed.get("response")
        {
            let mut mapped = map_yandex_response_object(response);
            apply_legacy_tool_fallback_from_accumulated_stream(&mut mapped, &all_content);
            if !all_content.is_empty() && mapped.chunks.is_empty() && mapped.tool_calls.is_none() {
                mapped.chunks = chunks.clone();
            }
            if mapped.tool_calls.is_none() && !tool_calls.is_empty() {
                mapped.tool_calls = Some(tool_calls.clone());
            }
            return Ok(mapped);
        }

        // Yandex can stream cumulative response snapshots without `type`.
        if kind.is_empty()
            && let Some(response) = parsed.get("response")
        {
            let snapshot_text = extract_text_from_response_output(response);
            if !snapshot_text.is_empty() {
                if snapshot_text.starts_with(&all_content) {
                    let delta = &snapshot_text[all_content.len()..];
                    if !delta.is_empty() {
                        all_content.push_str(delta);
                        chunks.push(delta.to_string());
                    }
                } else if all_content.is_empty() {
                    all_content = snapshot_text.clone();
                    chunks.push(snapshot_text);
                } else {
                    all_content = snapshot_text.clone();
                    chunks = vec![snapshot_text];
                }
            }

            merge_tool_calls(&mut tool_calls, extract_tool_calls_from_response_output(response));

            if response.get("status").and_then(Value::as_str) == Some("completed") {
                let mut mapped = map_yandex_response_object(response);
                apply_legacy_tool_fallback_from_accumulated_stream(&mut mapped, &all_content);
                if !all_content.is_empty()
                    && mapped.chunks.is_empty()
                    && mapped.tool_calls.is_none()
                {
                    mapped.chunks = chunks.clone();
                }
                if mapped.tool_calls.is_none() && !tool_calls.is_empty() {
                    mapped.tool_calls = Some(tool_calls.clone());
                }
                return Ok(mapped);
            }
            continue;
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

fn apply_legacy_tool_fallback_from_accumulated_stream(
    outcome: &mut ProviderOutcome,
    all_content: &str,
) {
    if outcome.tool_calls.is_some() || all_content.trim().is_empty() {
        return;
    }
    if let Some((assistant_text, calls)) = parse_legacy_tool_calls_from_text(all_content) {
        outcome.chunks = if assistant_text.is_empty() { Vec::new() } else { vec![assistant_text] };
        outcome.tool_calls = Some(calls);
    }
}

fn map_yandex_response_object(response: &Value) -> ProviderOutcome {
    let mut content = extract_text_from_response_output(response);
    let mut tool_calls = extract_tool_calls_from_response_output(response);
    if tool_calls.is_empty()
        && let Some((assistant_text, legacy_calls)) = parse_legacy_tool_calls_from_text(&content)
    {
        content = assistant_text;
        tool_calls = legacy_calls;
    }
    let output_tokens = response
        .get("usage")
        .and_then(Value::as_object)
        .and_then(|usage| usage.get("output_tokens"))
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or_else(|| {
            if content.is_empty() { 0 } else { content.split_whitespace().count() as u32 }
        });

    ProviderOutcome {
        chunks: if content.is_empty() { Vec::new() } else { vec![content] },
        output_tokens,
        reasoning: None,
        reasoning_details: None,
        tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
        emitted_live: false,
    }
}

fn extract_text_from_response_output(response: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(output) = response.get("output") {
        collect_text_candidates(output, &mut parts);
    }
    if parts.is_empty() {
        collect_text_candidates(response, &mut parts);
    }
    parts.join("")
}

fn collect_text_candidates(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_text_candidates(item, out);
            }
        }
        Value::Object(map) => {
            for key in ["text", "output_text", "input_text", "value"] {
                if let Some(text) = map.get(key).and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_string());
                    }
                }
            }
            for nested in map.values() {
                collect_text_candidates(nested, out);
            }
        }
        _ => {}
    }
}

fn extract_tool_calls_from_response_output(response: &Value) -> Vec<ToolCall> {
    response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .filter_map(|item| {
            let call_id = item.get("call_id").and_then(Value::as_str).map(str::trim)?;
            let name = item.get("name").and_then(Value::as_str).map(str::trim)?;
            if call_id.is_empty() || name.is_empty() {
                return None;
            }
            let arguments = item
                .get("arguments")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| item.get("arguments").map(Value::to_string))
                .unwrap_or_else(|| "{}".to_string());
            Some(ToolCall {
                id: call_id.to_string(),
                kind: "function".to_string(),
                function: ToolFunction { name: name.to_string(), arguments },
            })
        })
        .collect()
}

fn parse_legacy_tool_calls_from_text(text: &str) -> Option<(String, Vec<ToolCall>)> {
    if let Some((assistant_text, parsed_calls)) = parse_fenced_tool_calls_message(text) {
        let calls = parsed_calls
            .into_iter()
            .enumerate()
            .map(|(index, (name, arguments))| ToolCall {
                id: format!("yandex-legacy-fenced-{index}-{}", Uuid::new_v4()),
                kind: "function".to_string(),
                function: ToolFunction { name, arguments },
            })
            .collect::<Vec<_>>();
        if !calls.is_empty() {
            return Some((assistant_text, calls));
        }
    }

    if let Some(parsed_calls) = parse_legacy_tool_calls_message(text) {
        let calls = parsed_calls
            .into_iter()
            .enumerate()
            .map(|(index, (name, arguments))| ToolCall {
                id: format!("yandex-legacy-{index}-{}", Uuid::new_v4()),
                kind: "function".to_string(),
                function: ToolFunction { name, arguments },
            })
            .collect::<Vec<_>>();
        if !calls.is_empty() {
            return Some((String::new(), calls));
        }
    }

    let (name, arguments) = parse_legacy_tool_call_message(text)?;
    Some((
        String::new(),
        vec![ToolCall {
            id: format!("yandex-legacy-{}", Uuid::new_v4()),
            kind: "function".to_string(),
            function: ToolFunction { name, arguments },
        }],
    ))
}

fn parse_legacy_tool_calls_message(text: &str) -> Option<Vec<(String, String)>> {
    let raw = text.trim();
    if !raw.contains(LEGACY_TOOL_CALL_START_MARKER) {
        return None;
    }
    let mut out = Vec::new();
    for chunk in raw.split(LEGACY_TOOL_CALL_START_MARKER).skip(1) {
        let part = chunk.trim_start();
        let (name, raw_arguments) = part.split_once('\n')?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let raw_arguments = raw_arguments
            .split_once(LEGACY_TOOL_CALL_END_MARKER)
            .map(|(before, _)| before)
            .unwrap_or(raw_arguments)
            .trim();
        if raw_arguments.is_empty() {
            return None;
        }
        let arguments = parse_legacy_tool_arguments(raw_arguments)?;
        out.push((name.to_string(), arguments));
    }
    if out.is_empty() { None } else { Some(out) }
}

fn parse_legacy_tool_call_message(text: &str) -> Option<(String, String)> {
    let raw = text.trim();
    if raw.is_empty() {
        return None;
    }
    let has_start_marker = raw.contains(LEGACY_TOOL_CALL_START_MARKER);
    let after_marker =
        raw.strip_prefix(LEGACY_TOOL_CALL_START_MARKER).map(str::trim_start).unwrap_or(raw);
    let (name, raw_arguments) = after_marker.split_once('\n')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if !has_start_marker && name != "exec_command" {
        return None;
    }
    let raw_arguments = raw_arguments
        .split_once(LEGACY_TOOL_CALL_END_MARKER)
        .map(|(before, _)| before)
        .unwrap_or(raw_arguments)
        .trim();
    if raw_arguments.is_empty() {
        return None;
    }
    let arguments = parse_legacy_tool_arguments(raw_arguments)?;
    Some((name.to_string(), arguments))
}

fn parse_fenced_tool_calls_message(text: &str) -> Option<(String, Vec<(String, String)>)> {
    let mut tool_calls = Vec::new();
    let mut assistant_text = String::new();
    let mut cursor = 0;

    while let Some(start_rel_idx) = text[cursor..].find("```") {
        let start_idx = cursor + start_rel_idx;
        assistant_text.push_str(&text[cursor..start_idx]);
        let after_start_idx = start_idx + 3;
        let Some(end_rel_idx) = text[after_start_idx..].find("```") else {
            assistant_text.push_str(&text[start_idx..]);
            cursor = text.len();
            break;
        };
        let end_idx = after_start_idx + end_rel_idx;
        let block = text[after_start_idx..end_idx].trim();
        if let Some((name, arguments)) = parse_fenced_tool_call_block(block) {
            tool_calls.push((name, arguments));
        } else {
            assistant_text.push_str("```");
            assistant_text.push_str(&text[after_start_idx..end_idx]);
            assistant_text.push_str("```");
        }
        cursor = end_idx + 3;
    }

    if cursor < text.len() {
        assistant_text.push_str(&text[cursor..]);
    }

    if tool_calls.is_empty() { None } else { Some((assistant_text.trim().to_string(), tool_calls)) }
}

fn parse_fenced_tool_call_block(block: &str) -> Option<(String, String)> {
    if block.is_empty() {
        return None;
    }
    let (name, raw_arguments) = block.split_once('\n')?;
    let name = name.trim();
    if !is_tool_name_like(name) {
        return None;
    }
    let arguments = parse_legacy_tool_arguments(raw_arguments.trim())?;
    Some((name.to_string(), arguments))
}

fn is_tool_name_like(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn parse_legacy_tool_arguments(raw_arguments: &str) -> Option<String> {
    parse_json_object_to_canonical_string(raw_arguments)
        .or_else(|| keep_raw_json_object_string(raw_arguments))
}

fn parse_json_object_to_canonical_string(raw: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    serde_json::to_string(&parsed).ok()
}

fn keep_raw_json_object_string(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed.to_string());
    }
    None
}

fn merge_tool_calls(into: &mut Vec<ToolCall>, incoming: Vec<ToolCall>) {
    for call in incoming {
        if into.iter().any(|existing| existing.id == call.id) {
            continue;
        }
        into.push(call);
    }
}

fn extract_sse_data_events(payload: &str) -> Vec<String> {
    payload
        .replace('\r', "")
        .split("\n\n")
        .filter_map(|frame| {
            let data_lines = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>();
            if data_lines.is_empty() { None } else { Some(data_lines.join("\n")) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_yandex_responses_payload, map_yandex_responses_stream_text,
        normalize_tool_choice_for_responses, sanitize_yandex_input,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use xrouter_contracts::{ResponseInputContent, ResponseInputItem, ResponsesInput};

    #[test]
    fn includes_normalized_tools_in_responses_payload() {
        let input = ResponsesInput::Text("hello".to_string());
        let tools = vec![
            json!({
                "type":"function",
                "function":{
                    "name":"read_file",
                    "description":"Read file",
                    "parameters":{"type":"object","properties":{"path":{"type":"string"}}}
                }
            }),
            json!({"type":"web_search"}),
        ];
        let (payload, normalization) = build_yandex_responses_payload(
            "gpt://folder/yandexgpt/rc",
            &input,
            Some(&tools),
            Some(&json!("auto")),
        );

        assert_eq!(normalization.tools_in, 2);
        assert_eq!(normalization.tools_out, 1);
        assert_eq!(normalization.tools_dropped, 1);
        assert_eq!(payload["tools"][0]["type"], "function");
        assert_eq!(payload["tools"][0]["name"], "read_file");
        assert_eq!(payload["tools"][0]["description"], "Read file");
        assert_eq!(payload["tool_choice"], json!("auto"));
    }

    #[test]
    fn normalizes_function_tool_choice_shape() {
        assert_eq!(
            normalize_tool_choice_for_responses(
                Some(&json!({"type":"function","function":{"name":"read_file"}})),
                true
            ),
            Some(json!({"type":"function","name":"read_file"}))
        );
        assert_eq!(
            normalize_tool_choice_for_responses(Some(&json!("any")), true),
            Some(json!("required"))
        );
    }

    #[test]
    fn yandex_responses_sse_without_type_uses_cumulative_response_snapshots() {
        let sse = concat!(
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[],\"usage\":{\"output_tokens\":0},\"status\":\"in_progress\"}}\n\n",
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"hel\"}]}],\"usage\":{\"output_tokens\":1},\"status\":\"in_progress\"}}\n\n",
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello\"}]}],\"usage\":{\"output_tokens\":1},\"status\":\"completed\"}}\n\n"
        );
        let outcome =
            map_yandex_responses_stream_text(sse).expect("snapshot responses SSE must parse");
        assert_eq!(outcome.chunks.join(""), "hello");
    }

    #[test]
    fn yandex_extracts_text_when_output_uses_value_field() {
        let sse = concat!(
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"value\":\"ok\"}]}],\"status\":\"completed\"}}\n\n"
        );
        let outcome = map_yandex_responses_stream_text(sse)
            .expect("responses SSE with value field must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn yandex_legacy_tool_call_content_maps_to_function_call_item() {
        let sse = concat!(
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"exec_command\\n{\\\"cmd\\\":\\\"ls -la\\\"}\"}]}],\"status\":\"completed\"}}\n\n"
        );
        let outcome =
            map_yandex_responses_stream_text(sse).expect("legacy tool-call content must parse");
        let calls = outcome.tool_calls.expect("legacy tool call must map");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "exec_command");
        assert_eq!(calls[0].function.arguments, r#"{"cmd":"ls -la"}"#);
        assert!(outcome.chunks.is_empty());
    }

    #[test]
    fn yandex_fenced_tool_call_content_maps_to_function_call_item() {
        let sse = concat!(
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Ниже вызов\\n```exec_command\\n{\\\"cmd\\\":\\\"pwd\\\"}\\n```\"}]}],\"status\":\"completed\"}}\n\n"
        );
        let outcome =
            map_yandex_responses_stream_text(sse).expect("fenced tool-call content must parse");
        let calls = outcome.tool_calls.expect("fenced legacy tool call must map");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "exec_command");
        assert_eq!(calls[0].function.arguments, r#"{"cmd":"pwd"}"#);
    }

    #[test]
    fn yandex_maps_legacy_tool_call_when_completed_payload_is_empty_but_delta_has_marker() {
        let sse = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"[TOOL_CALL_START]exec_command\\n{\\\"cmd\\\":\\\"ls -la\\\"}[TOOL_CALL_END]\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"\"}]}],\"status\":\"completed\",\"usage\":{\"output_tokens\":0}}}\n\n"
        );
        let outcome =
            map_yandex_responses_stream_text(sse).expect("legacy stream marker must parse");
        let calls = outcome.tool_calls.expect("tool call must be reconstructed from delta");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "exec_command");
        assert_eq!(calls[0].function.arguments, r#"{"cmd":"ls -la"}"#);
    }

    #[test]
    fn yandex_legacy_marker_keeps_raw_arguments_when_json_parse_fails() {
        let sse = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"[TOOL_CALL_START]exec_command\\n{\\\"cmd\\\":foo}[TOOL_CALL_END]\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"\"}]}],\"status\":\"completed\",\"usage\":{\"output_tokens\":0}}}\n\n"
        );
        let outcome =
            map_yandex_responses_stream_text(sse).expect("legacy marker with raw args must parse");
        let calls = outcome.tool_calls.expect("tool call must be reconstructed from delta");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "exec_command");
        assert_eq!(calls[0].function.arguments, r#"{"cmd":foo}"#);
    }

    #[test]
    fn yandex_sanitize_drops_empty_assistant_messages() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("user".to_string()),
                content: Some(ResponseInputContent::Text("hi".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("assistant".to_string()),
                content: Some(ResponseInputContent::Text("".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
        ]);
        let sanitized = sanitize_yandex_input(&input);
        let ResponsesInput::Items(items) = sanitized else {
            panic!("expected items");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].role.as_deref(), Some("user"));
    }

    #[test]
    fn yandex_sanitize_drops_preamble_between_call_and_output() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("function_call".to_string()),
                role: Some("assistant".to_string()),
                content: None,
                text: None,
                output: None,
                call_id: Some("call_1".to_string()),
                name: Some("exec_command".to_string()),
                arguments: Some("{\"cmd\":\"ls\"}".to_string()),
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("assistant".to_string()),
                content: Some(ResponseInputContent::Text("thinking".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("function_call_output".to_string()),
                role: None,
                content: None,
                text: None,
                output: Some("{\"ok\":true}".to_string()),
                call_id: Some("call_1".to_string()),
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
        ]);
        let sanitized = sanitize_yandex_input(&input);
        let ResponsesInput::Items(items) = sanitized else {
            panic!("expected items");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind.as_deref(), Some("function_call"));
        assert_eq!(items[1].kind.as_deref(), Some("function_call_output"));
    }
}
