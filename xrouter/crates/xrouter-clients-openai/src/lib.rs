use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;
use xrouter_contracts::ReasoningConfig;
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

const GIGACHAT_OAUTH_URL: &str = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth";
const GIGACHAT_DEFAULT_SCOPE: &str = "GIGACHAT_API_PERS";
const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;

pub fn build_http_client(timeout_seconds: u64) -> Option<Client> {
    Client::builder().timeout(Duration::from_secs(timeout_seconds)).build().ok()
}

pub fn build_http_client_insecure_tls(timeout_seconds: u64) -> Option<Client> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_seconds))
        .danger_accept_invalid_certs(true)
        .build()
        .ok()
}

#[derive(Clone)]
struct HttpRuntime {
    provider_id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    http_client: Option<Client>,
    max_inflight: Option<Arc<Semaphore>>,
}

impl HttpRuntime {
    fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        let max_inflight = max_inflight.map(Semaphore::new).map(Arc::new);
        Self { provider_id, base_url, api_key, http_client, max_inflight }
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|value| !value.trim().is_empty())
    }

    fn base_url(&self) -> Result<&str, CoreError> {
        self.base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Provider("provider base_url is not configured".to_string()))
    }

    fn build_url(&self, path: &str) -> Result<String, CoreError> {
        let base_url = self.base_url()?.trim_end_matches('/');
        Ok(format!("{base_url}/{}", path.trim_start_matches('/')))
    }

    fn client(&self) -> Result<&Client, CoreError> {
        self.http_client
            .as_ref()
            .ok_or_else(|| CoreError::Provider("provider client init failed".to_string()))
    }

    fn acquire_inflight_permit(
        &self,
    ) -> Result<Option<tokio::sync::OwnedSemaphorePermit>, CoreError> {
        self.max_inflight
            .as_ref()
            .map(|semaphore| {
                semaphore.clone().try_acquire_owned().map_err(|_| {
                    CoreError::Provider(format!(
                        "provider overloaded: max in-flight limit reached for {}",
                        self.provider_id
                    ))
                })
            })
            .transpose()
    }

    async fn post_json<T: DeserializeOwned>(
        &self,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
    ) -> Result<T, CoreError> {
        let _permit = self.acquire_inflight_permit()?;
        let client = self.client()?;

        let mut request = client.post(url).header("Content-Type", "application/json").json(payload);
        if let Some(token) = bearer_override.or(self.api_key()) {
            request = request.bearer_auth(token);
        }
        for (name, value) in extra_headers {
            request = request.header(name, value);
        }

        let response = request
            .send()
            .await
            .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))?;
        let response = response
            .error_for_status()
            .map_err(|err| CoreError::Provider(format!("provider returned error status: {err}")))?;
        response
            .json::<T>()
            .await
            .map_err(|err| CoreError::Provider(format!("provider response parse failed: {err}")))
    }

    async fn post_form<T: DeserializeOwned>(
        &self,
        url: &str,
        form_fields: &[(String, String)],
        headers: &[(String, String)],
    ) -> Result<T, CoreError> {
        let client = self.client()?;
        let mut request = client.post(url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        request
            .form(form_fields)
            .send()
            .await
            .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))?
            .error_for_status()
            .map_err(|err| CoreError::Provider(format!("provider returned error status: {err}")))?
            .json::<T>()
            .await
            .map_err(|err| CoreError::Provider(format!("provider response parse failed: {err}")))
    }
}

pub struct MockProviderClient {
    provider_id: String,
}

impl MockProviderClient {
    pub fn new(provider_id: String) -> Self {
        Self { provider_id }
    }
}

#[async_trait]
impl ProviderClient for MockProviderClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        _reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
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
}

pub struct OpenAiClient {
    runtime: HttpRuntime,
}

impl OpenAiClient {
    pub fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(provider_id, base_url, api_key, http_client, max_inflight),
        }
    }
}

#[async_trait]
impl ProviderClient for OpenAiClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openai_payload(model, input, reasoning);
        let response: ChatCompletionsResponse =
            self.runtime.post_json(&url, &payload, None, &[]).await?;
        map_chat_completion_response(response)
    }
}

pub struct OpenRouterClient {
    runtime: HttpRuntime,
}

impl OpenRouterClient {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "openrouter".to_string(),
                base_url,
                api_key,
                http_client,
                max_inflight,
            ),
        }
    }
}

#[async_trait]
impl ProviderClient for OpenRouterClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_openrouter_payload(model, input, reasoning);
        let response: ChatCompletionsResponse =
            self.runtime.post_json(&url, &payload, None, &[]).await?;
        map_chat_completion_response(response)
    }
}

pub struct DeepSeekClient {
    runtime: HttpRuntime,
}

impl DeepSeekClient {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "deepseek".to_string(),
                base_url,
                api_key,
                http_client,
                max_inflight,
            ),
        }
    }
}

#[async_trait]
impl ProviderClient for DeepSeekClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_deepseek_payload(model, input, reasoning);
        let response: ChatCompletionsResponse =
            self.runtime.post_json(&url, &payload, None, &[]).await?;
        map_chat_completion_response(response)
    }
}

pub struct ZaiClient {
    runtime: HttpRuntime,
}

impl ZaiClient {
    pub fn new(
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "zai".to_string(),
                base_url,
                api_key,
                http_client,
                max_inflight,
            ),
        }
    }
}

#[async_trait]
impl ProviderClient for ZaiClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_zai_payload(model, input, reasoning);
        let response: ChatCompletionsResponse =
            self.runtime.post_json(&url, &payload, None, &[]).await?;
        map_chat_completion_response(response)
    }
}

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
        model: &str,
        input: &str,
        _reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("responses")?;
        let upstream_model = build_yandex_upstream_model(model, self.project.as_deref())?;
        let payload = build_yandex_responses_payload(&upstream_model, input);
        let mut headers = Vec::new();
        if let Some(project) = self.project.as_deref().filter(|value| !value.trim().is_empty()) {
            headers.push(("OpenAI-Project".to_string(), project.to_string()));
        }
        let response: ResponsesApiResponse =
            self.runtime.post_json(&url, &payload, None, &headers).await?;
        map_responses_api_response(response)
    }
}

pub struct GigachatClient {
    runtime: HttpRuntime,
    scope: String,
    token_state: Arc<Mutex<Option<GigachatToken>>>,
}

impl GigachatClient {
    pub fn new(
        base_url: Option<String>,
        authorization_key: Option<String>,
        scope: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "gigachat".to_string(),
                base_url,
                authorization_key,
                http_client,
                max_inflight,
            ),
            scope: scope.unwrap_or_else(|| GIGACHAT_DEFAULT_SCOPE.to_string()),
            token_state: Arc::new(Mutex::new(None)),
        }
    }

    async fn access_token(&self) -> Result<String, CoreError> {
        let now_ms = current_time_millis();
        let mut guard = self.token_state.lock().await;
        if let Some(token) = guard.as_ref()
            && token.expires_at_ms > now_ms + TOKEN_REFRESH_BUFFER_MS
        {
            return Ok(token.access_token.clone());
        }

        let authorization_key = self.runtime.api_key().ok_or_else(|| {
            CoreError::Provider("provider api_key is not configured for gigachat".to_string())
        })?;

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {authorization_key}")),
            ("RqUID".to_string(), Uuid::new_v4().to_string()),
            ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()),
        ];
        let form_fields = vec![("scope".to_string(), self.scope.clone())];

        let response: GigachatOauthResponse =
            self.runtime.post_form(GIGACHAT_OAUTH_URL, &form_fields, &headers).await?;

        let token = GigachatToken {
            access_token: response.access_token,
            expires_at_ms: response.expires_at,
        };

        let value = token.access_token.clone();
        *guard = Some(token);
        Ok(value)
    }
}

#[async_trait]
impl ProviderClient for GigachatClient {
    async fn generate(
        &self,
        model: &str,
        input: &str,
        _reasoning: Option<&ReasoningConfig>,
    ) -> Result<ProviderOutcome, CoreError> {
        let access_token = self.access_token().await?;
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_gigachat_payload(model, input);
        let response: ChatCompletionsResponse =
            self.runtime.post_json(&url, &payload, Some(access_token.as_str()), &[]).await?;
        map_chat_completion_response(response)
    }
}

#[derive(Debug, Clone)]
struct GigachatToken {
    access_token: String,
    expires_at_ms: i64,
}

fn current_time_millis() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    duration.as_millis() as i64
}

fn build_openai_payload(model: &str, input: &str, reasoning: Option<&ReasoningConfig>) -> Value {
    let mut payload = base_chat_payload(model, input);
    if let Some(reasoning_cfg) = normalize_openai_reasoning(reasoning) {
        payload.insert("reasoning".to_string(), reasoning_cfg);
    }
    Value::Object(payload)
}

fn build_openrouter_payload(
    model: &str,
    input: &str,
    reasoning: Option<&ReasoningConfig>,
) -> Value {
    let mut payload = base_chat_payload(model, input);
    if let Some(reasoning_cfg) = reasoning
        && let Ok(value) = serde_json::to_value(reasoning_cfg)
    {
        payload.insert("reasoning".to_string(), value);
    }
    Value::Object(payload)
}

fn build_deepseek_payload(model: &str, input: &str, reasoning: Option<&ReasoningConfig>) -> Value {
    let mut payload = base_chat_payload(model, input);
    let has_effort = reasoning
        .and_then(|cfg| cfg.effort.as_deref())
        .is_some_and(|value| !value.trim().is_empty());
    if model == "deepseek-chat" && has_effort {
        payload.insert("thinking".to_string(), json!({ "type": "enabled" }));
    }
    Value::Object(payload)
}

fn build_zai_payload(model: &str, input: &str, reasoning: Option<&ReasoningConfig>) -> Value {
    let mut payload = base_chat_payload(model, input);
    if let Some(effort) = reasoning.and_then(|cfg| cfg.effort.as_deref()).map(str::trim)
        && !effort.is_empty()
    {
        let thinking_type =
            if effort.eq_ignore_ascii_case("none") { "disabled" } else { "enabled" };
        payload.insert("thinking".to_string(), json!({ "type": thinking_type }));
    }
    Value::Object(payload)
}

fn build_gigachat_payload(model: &str, input: &str) -> Value {
    Value::Object(base_chat_payload(model, input))
}

fn base_chat_payload(model: &str, input: &str) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(model.to_string()));
    payload.insert("messages".to_string(), json!([{ "role": "user", "content": input }]));
    payload.insert("stream".to_string(), Value::Bool(false));
    payload
}

fn build_yandex_responses_payload(model: &str, input: &str) -> Value {
    json!({
        "model": model,
        "input": input,
        "stream": false
    })
}

fn build_yandex_upstream_model(model: &str, project: Option<&str>) -> Result<String, CoreError> {
    if model.starts_with("gpt://") {
        return Ok(model.to_string());
    }

    let project = project.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
        CoreError::Provider("provider project is not configured for yandex".to_string())
    })?;

    Ok(format!("gpt://{project}/{model}"))
}

fn normalize_openai_reasoning(reasoning: Option<&ReasoningConfig>) -> Option<Value> {
    let effort = reasoning?.effort.as_deref()?.trim();
    if effort.is_empty() {
        return None;
    }
    let mapped = if effort.eq_ignore_ascii_case("xhigh") { "high" } else { effort };
    Some(json!({ "effort": mapped }))
}

fn map_chat_completion_response(
    payload: ChatCompletionsResponse,
) -> Result<ProviderOutcome, CoreError> {
    let first = payload
        .choices
        .first()
        .ok_or_else(|| CoreError::Provider("provider returned empty choices".to_string()))?;

    let content = extract_message_content(&first.message.content).ok_or_else(|| {
        CoreError::Provider("provider returned empty message content".to_string())
    })?;

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
            reasoning_details.as_ref().and_then(|details| extract_reasoning_from_details(details))
        });

    Ok(ProviderOutcome { chunks: vec![content], output_tokens, reasoning, reasoning_details })
}

fn map_responses_api_response(payload: ResponsesApiResponse) -> Result<ProviderOutcome, CoreError> {
    let content = extract_message_text_from_responses_output(&payload.output).ok_or_else(|| {
        CoreError::Provider("provider returned empty message content".to_string())
    })?;

    let reasoning_details = extract_reasoning_content_items_from_responses_output(&payload.output);
    let reasoning = extract_reasoning_text_from_responses_output(&payload.output).or_else(|| {
        reasoning_details.as_ref().and_then(|details| extract_reasoning_from_details(details))
    });

    let output_tokens = payload
        .usage
        .as_ref()
        .map(|u| u.output_tokens)
        .unwrap_or_else(|| content.split_whitespace().count() as u32);

    Ok(ProviderOutcome { chunks: vec![content], output_tokens, reasoning, reasoning_details })
}

#[derive(Debug, Deserialize)]
struct GigachatOauthResponse {
    access_token: String,
    expires_at: i64,
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
    content: Value,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Option<Vec<Value>>,
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

fn extract_reasoning_from_details(details: &[Value]) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reasoning(effort: &str) -> ReasoningConfig {
        ReasoningConfig { effort: Some(effort.to_string()) }
    }

    #[test]
    fn openrouter_keeps_reasoning_effort_as_is() {
        let payload =
            build_openrouter_payload("openai/gpt-5.2", "Reply with ok", Some(&reasoning("xhigh")));
        assert_eq!(payload["reasoning"]["effort"], "xhigh");
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn deepseek_chat_enables_thinking_when_effort_present() {
        let payload =
            build_deepseek_payload("deepseek-chat", "Reply with ok", Some(&reasoning("medium")));
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn deepseek_reasoner_does_not_set_thinking() {
        let payload =
            build_deepseek_payload("deepseek-reasoner", "Reply with ok", Some(&reasoning("high")));
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn non_openrouter_maps_xhigh_to_high() {
        let payload =
            build_openai_payload("gpt-4.1-mini", "Reply with ok", Some(&reasoning("xhigh")));
        assert_eq!(payload["reasoning"]["effort"], "high");
    }

    #[test]
    fn zai_enables_thinking_when_effort_present() {
        let payload = build_zai_payload("glm-5", "Reply with ok", Some(&reasoning("high")));
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn zai_disables_thinking_when_effort_none() {
        let payload = build_zai_payload("glm-5", "Reply with ok", Some(&reasoning("none")));
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

    #[test]
    fn yandex_upstream_model_adds_gpt_prefix() {
        let model = build_yandex_upstream_model("aliceai-llm/latest", Some("folder-123"))
            .expect("model should build");
        assert_eq!(model, "gpt://folder-123/aliceai-llm/latest");
    }

    #[test]
    fn yandex_upstream_model_keeps_prefixed_model() {
        let model = build_yandex_upstream_model(
            "gpt://folder-123/yandexgpt-lite/latest",
            Some("folder-123"),
        )
        .expect("model should pass through");
        assert_eq!(model, "gpt://folder-123/yandexgpt-lite/latest");
    }

    #[test]
    fn yandex_upstream_model_requires_project() {
        let error = build_yandex_upstream_model("aliceai-llm/latest", None)
            .expect_err("missing project should fail");
        assert_eq!(
            error.to_string(),
            "provider error: provider project is not configured for yandex"
        );
    }
}
