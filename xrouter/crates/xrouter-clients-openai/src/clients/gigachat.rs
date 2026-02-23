use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use uuid::Uuid;
use xrouter_contracts::{ReasoningConfig, ResponseEvent, ResponsesInput};
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

use crate::{HttpRuntime, base_chat_payload};

const GIGACHAT_OAUTH_URL: &str = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth";
const GIGACHAT_DEFAULT_SCOPE: &str = "GIGACHAT_API_PERS";
const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;

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
        input: &ResponsesInput,
        _reasoning: Option<&ReasoningConfig>,
        _tools: Option<&[Value]>,
        _tool_choice: Option<&Value>,
    ) -> Result<ProviderOutcome, CoreError> {
        let access_token = self.access_token().await?;
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_gigachat_payload(model, input);
        self.runtime
            .post_chat_completions_stream(
                "request",
                &url,
                &payload,
                Some(access_token.as_str()),
                &[],
                None,
            )
            .await
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
        let access_token = self.access_token().await?;
        let url = self.runtime.build_url("chat/completions")?;
        let payload = build_gigachat_payload(model, input);
        self.runtime
            .post_chat_completions_stream(
                request_id,
                &url,
                &payload,
                Some(access_token.as_str()),
                &[],
                sender,
            )
            .await
    }
}

pub(crate) fn build_gigachat_payload(model: &str, input: &ResponsesInput) -> Value {
    Value::Object(base_chat_payload(model, input, None, None))
}

#[derive(Debug, Clone)]
struct GigachatToken {
    access_token: String,
    expires_at_ms: i64,
}

#[derive(Debug, Deserialize)]
struct GigachatOauthResponse {
    access_token: String,
    expires_at: i64,
}

fn current_time_millis() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    duration.as_millis() as i64
}
