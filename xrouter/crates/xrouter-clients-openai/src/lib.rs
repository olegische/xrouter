use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::sync::Semaphore;
use xrouter_contracts::ReasoningConfig;
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

#[derive(Debug, Clone, Copy)]
enum ClientMode {
    Real,
    Mock,
}

pub struct OpenAiCompatibleClient {
    provider_id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    project: Option<String>,
    http_client: Option<Client>,
    max_inflight: Option<Arc<Semaphore>>,
    mode: ClientMode,
}

impl OpenAiCompatibleClient {
    pub fn build_http_client(timeout_seconds: u64) -> Option<Client> {
        Client::builder().timeout(Duration::from_secs(timeout_seconds)).build().ok()
    }

    pub fn new_with_http_client(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        project: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        let max_inflight = max_inflight.map(Semaphore::new).map(Arc::new);
        Self {
            provider_id,
            base_url,
            api_key,
            project,
            http_client,
            max_inflight,
            mode: ClientMode::Real,
        }
    }

    pub fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        project: Option<String>,
        timeout_seconds: u64,
        max_inflight: Option<usize>,
    ) -> Self {
        let http_client = Self::build_http_client(timeout_seconds);
        Self::new_with_http_client(
            provider_id,
            base_url,
            api_key,
            project,
            http_client,
            max_inflight,
        )
    }

    pub fn mock(provider_id: String) -> Self {
        Self {
            provider_id,
            base_url: None,
            api_key: None,
            project: None,
            http_client: None,
            max_inflight: None,
            mode: ClientMode::Mock,
        }
    }

    fn mock_generate(&self, model: &str, input: &str) -> Result<ProviderOutcome, CoreError> {
        let mut chunks = Vec::new();
        let mut output_tokens = 0u32;

        for token in input.split_whitespace() {
            output_tokens = output_tokens.saturating_add(1);
            chunks.push(format!("{token} "));
        }

        if chunks.is_empty() {
            return Err(CoreError::Provider("provider returned empty output".to_string()));
        }

        chunks.insert(0, format!("[{}] ", self.provider_id));
        let reasoning = if model.contains("deepseek-reasoner") {
            Some("Reasoned with DeepSeek reasoning mode before composing final answer.".to_string())
        } else {
            None
        };

        Ok(ProviderOutcome { chunks, output_tokens, reasoning, reasoning_details: None })
    }

    async fn remote_generate(
        &self,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let Some(base_url) = self.base_url.as_deref().filter(|v| !v.trim().is_empty()) else {
            return Err(CoreError::Provider("provider base_url is not configured".to_string()));
        };
        let _permit = self
            .max_inflight
            .as_ref()
            .map(|semaphore| {
                semaphore.clone().try_acquire_owned().map_err(|_| {
                    CoreError::Provider(format!(
                        "provider overloaded: max in-flight limit reached for {}",
                        self.provider_id
                    ))
                })
            })
            .transpose()?;
        let client = self
            .http_client
            .as_ref()
            .ok_or_else(|| CoreError::Provider("provider client init failed".to_string()))?;

        if self.provider_id == "yandex" {
            return self
                .remote_generate_yandex_responses(base_url, client, model, input, reasoning)
                .await;
        }

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let payload = build_request_payload(&self.provider_id, model, input, reasoning);
        let request = self.build_http_request(client, &url, &payload);

        let response = request
            .send()
            .await
            .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| CoreError::Provider(format!("provider returned error status: {err}")))?;

        let payload = response
            .json::<ChatCompletionsResponse>()
            .await
            .map_err(|err| CoreError::Provider(format!("provider response parse failed: {err}")))?;
        let first = payload
            .choices
            .first()
            .ok_or_else(|| CoreError::Provider("provider returned empty choices".to_string()))?;
        let content = extract_message_content(&first.message.content).ok_or_else(|| {
            CoreError::Provider("provider returned empty message content".to_string())
        })?;
        let chunks = vec![content.clone()];
        let output_tokens = payload
            .usage
            .and_then(|usage| usage.completion_tokens)
            .unwrap_or_else(|| content.split_whitespace().count() as u32);
        let reasoning_details = first.message.reasoning_details.clone();
        let reasoning = first
            .message
            .reasoning_content
            .clone()
            .or_else(|| first.message.reasoning.clone())
            .or_else(|| {
                reasoning_details
                    .as_ref()
                    .and_then(|details| extract_reasoning_from_details(details))
            });

        Ok(ProviderOutcome { chunks, output_tokens, reasoning, reasoning_details })
    }

    async fn remote_generate_yandex_responses(
        &self,
        base_url: &str,
        client: &Client,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = format!("{}/responses", base_url.trim_end_matches('/'));
        let payload = build_responses_request_payload(model, input, reasoning);
        let request = self.build_http_request(client, &url, &payload);
        let response = request
            .send()
            .await
            .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| CoreError::Provider(format!("provider returned error status: {err}")))?;
        let payload = response
            .json::<ResponsesApiResponse>()
            .await
            .map_err(|err| CoreError::Provider(format!("provider response parse failed: {err}")))?;
        let content =
            extract_message_text_from_responses_output(&payload.output).ok_or_else(|| {
                CoreError::Provider("provider returned empty message content".to_string())
            })?;
        let reasoning_details =
            extract_reasoning_content_items_from_responses_output(&payload.output);
        let reasoning =
            extract_reasoning_text_from_responses_output(&payload.output).or_else(|| {
                reasoning_details
                    .as_ref()
                    .and_then(|details| extract_reasoning_from_details(details))
            });
        let output_tokens = payload
            .usage
            .as_ref()
            .map(|u| u.output_tokens)
            .unwrap_or_else(|| content.split_whitespace().count() as u32);

        Ok(ProviderOutcome { chunks: vec![content], output_tokens, reasoning, reasoning_details })
    }

    fn build_http_request(
        &self,
        client: &Client,
        url: &str,
        payload: &Value,
    ) -> reqwest::RequestBuilder {
        let mut request = client.post(url).header("Content-Type", "application/json").json(payload);
        if let Some(api_key) = self.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
            request = request.bearer_auth(api_key);
        }
        if self.provider_id == "yandex"
            && let Some(project) = self.project.as_deref().filter(|v| !v.trim().is_empty())
        {
            request = request.header("OpenAI-Project", project);
        }
        request
    }
}

#[async_trait]
impl ProviderClient for OpenAiCompatibleClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        match self.mode {
            ClientMode::Real => self.remote_generate(model, input, reasoning).await,
            ClientMode::Mock => self.mock_generate(model, input),
        }
    }
}

fn build_request_payload(
    provider_id: &str,
    model: &str,
    input: &str,
    reasoning: Option<&ReasoningConfig>,
) -> Value {
    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(model.to_string()));
    payload.insert("messages".to_string(), json!([{ "role": "user", "content": input }]));
    payload.insert("stream".to_string(), Value::Bool(false));

    match provider_id {
        "openrouter" => {
            if let Some(reasoning_cfg) = reasoning
                && let Ok(value) = serde_json::to_value(reasoning_cfg)
            {
                payload.insert("reasoning".to_string(), value);
            }
        }
        "deepseek" => {
            let has_effort = reasoning
                .and_then(|cfg| cfg.effort.as_deref())
                .is_some_and(|value| !value.trim().is_empty());
            if model == "deepseek-chat" && has_effort {
                payload.insert("thinking".to_string(), json!({ "type": "enabled" }));
            }
        }
        "zai" => {
            if let Some(effort) = reasoning.and_then(|cfg| cfg.effort.as_deref()).map(str::trim)
                && !effort.is_empty()
            {
                let thinking_type =
                    if effort.eq_ignore_ascii_case("none") { "disabled" } else { "enabled" };
                payload.insert("thinking".to_string(), json!({ "type": thinking_type }));
            }
        }
        _ => {
            if let Some(reasoning_cfg) = normalize_openai_reasoning(reasoning) {
                payload.insert("reasoning".to_string(), reasoning_cfg);
            }
        }
    }

    Value::Object(payload)
}

fn build_responses_request_payload(
    model: &str,
    input: &str,
    reasoning: Option<&ReasoningConfig>,
) -> Value {
    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(model.to_string()));
    payload.insert("input".to_string(), Value::String(input.to_string()));
    payload.insert("stream".to_string(), Value::Bool(false));
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

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    content: serde_json::Value,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiResponse {
    #[serde(default)]
    output: Vec<ResponsesApiOutputItem>,
    #[serde(default)]
    usage: Option<ResponsesApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiUsage {
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiOutputItem {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Option<Vec<Value>>,
    #[serde(default)]
    summary: Option<Vec<ResponsesApiSummary>>,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiSummary {
    #[serde(default)]
    text: String,
}

fn extract_message_content(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(text) if !text.is_empty() => Some(text.clone()),
        serde_json::Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

fn extract_reasoning_from_details(details: &[serde_json::Value]) -> Option<String> {
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
    output.iter().find_map(|item| {
        if item.kind != "message" {
            return None;
        }
        let text = item
            .content
            .as_ref()?
            .iter()
            .find_map(|part| part.get("text").and_then(Value::as_str))?
            .trim()
            .to_string();
        if text.is_empty() { None } else { Some(text) }
    })
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
            .map(|v| v.text.trim().to_string())
            .filter(|v| !v.is_empty());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reasoning(effort: &str) -> ReasoningConfig {
        ReasoningConfig { effort: Some(effort.to_string()) }
    }

    #[test]
    fn openrouter_keeps_reasoning_effort_as_is() {
        let payload = build_request_payload(
            "openrouter",
            "openai/gpt-5.2",
            "Reply with ok",
            Some(&reasoning("xhigh")),
        );
        assert_eq!(payload["reasoning"]["effort"], "xhigh");
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn deepseek_chat_enables_thinking_when_effort_present() {
        let payload = build_request_payload(
            "deepseek",
            "deepseek-chat",
            "Reply with ok",
            Some(&reasoning("medium")),
        );
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn deepseek_reasoner_does_not_set_thinking() {
        let payload = build_request_payload(
            "deepseek",
            "deepseek-reasoner",
            "Reply with ok",
            Some(&reasoning("high")),
        );
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn non_openrouter_maps_xhigh_to_high() {
        let payload = build_request_payload(
            "xrouter",
            "gpt-4.1-mini",
            "Reply with ok",
            Some(&reasoning("xhigh")),
        );
        assert_eq!(payload["reasoning"]["effort"], "high");
    }

    #[test]
    fn zai_enables_thinking_when_effort_present() {
        let payload =
            build_request_payload("zai", "glm-5", "Reply with ok", Some(&reasoning("high")));
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn zai_disables_thinking_when_effort_none() {
        let payload =
            build_request_payload("zai", "glm-5", "Reply with ok", Some(&reasoning("none")));
        assert_eq!(payload["thinking"]["type"], "disabled");
        assert!(payload.get("reasoning").is_none());
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
}
