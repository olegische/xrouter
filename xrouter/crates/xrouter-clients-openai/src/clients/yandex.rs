use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use xrouter_contracts::{ReasoningConfig, ResponseEvent, ResponsesInput};
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

use crate::HttpRuntime;

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
        input: &ResponsesInput,
        _reasoning: Option<&ReasoningConfig>,
        _tools: Option<&[Value]>,
        _tool_choice: Option<&Value>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("responses")?;
        let upstream_model = build_yandex_upstream_model(model, self.project.as_deref())?;
        let payload = build_yandex_responses_payload(&upstream_model, input);
        let mut headers = Vec::new();
        if let Some(project) = self.project.as_deref().filter(|value| !value.trim().is_empty()) {
            headers.push(("OpenAI-Project".to_string(), project.to_string()));
        }
        self.runtime.post_responses_stream("request", &url, &payload, None, &headers, None).await
    }

    async fn generate_stream(
        &self,
        request_id: &str,
        model: &str,
        input: &ResponsesInput,
        _reasoning: Option<&ReasoningConfig>,
        _tools: Option<&[Value]>,
        _tool_choice: Option<&Value>,
        sender: Option<&mpsc::Sender<Result<ResponseEvent, CoreError>>>,
    ) -> Result<ProviderOutcome, CoreError> {
        let url = self.runtime.build_url("responses")?;
        let upstream_model = build_yandex_upstream_model(model, self.project.as_deref())?;
        let payload = build_yandex_responses_payload(&upstream_model, input);
        let mut headers = Vec::new();
        if let Some(project) = self.project.as_deref().filter(|value| !value.trim().is_empty()) {
            headers.push(("OpenAI-Project".to_string(), project.to_string()));
        }
        self.runtime.post_responses_stream(request_id, &url, &payload, None, &headers, sender).await
    }
}

pub(crate) fn build_yandex_responses_payload(model: &str, input: &ResponsesInput) -> Value {
    let input_value =
        serde_json::to_value(input).unwrap_or_else(|_| Value::String(input.to_canonical_text()));
    json!({
        "model": model,
        "input": input_value,
        "stream": true
    })
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
