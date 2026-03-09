use std::sync::Arc;

use xrouter_clients_openai::DeepSeekClient;
use xrouter_clients_openai::runtime::SharedProviderRuntime;
use xrouter_contracts::ResponsesInput;
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome, ResponseEventSink,
};

use crate::error::BrowserError;
use crate::runtime::BrowserProviderRuntime;

pub const DEFAULT_DEMO_PROMPT: &str = "Hello, what can you do?";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserProvider {
    DeepSeek,
}

impl BrowserProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
        }
    }
}

impl TryFrom<&str> for BrowserProvider {
    type Error = BrowserError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "deepseek" => Ok(Self::DeepSeek),
            other => Err(BrowserError::UnsupportedProvider(other.to_string())),
        }
    }
}

pub struct BrowserInferenceClient {
    provider: BrowserProvider,
    runtime: Arc<BrowserProviderRuntime>,
    shared_runtime: SharedProviderRuntime,
}

impl BrowserInferenceClient {
    pub fn new(
        provider: BrowserProvider,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        let runtime = Arc::new(BrowserProviderRuntime::new(provider.as_str(), base_url, api_key));
        let shared_runtime: SharedProviderRuntime = runtime.clone();
        Self { provider, runtime, shared_runtime }
    }

    pub fn cancel(&self, request_id: &str) -> Result<(), BrowserError> {
        self.runtime.cancel(request_id)
    }

    pub async fn generate_text(
        &self,
        request_id: &str,
        model: &str,
        input: &str,
    ) -> Result<ProviderOutcome, CoreError> {
        self.generate_text_stream(request_id, model, input, None).await
    }

    pub async fn generate_text_stream(
        &self,
        request_id: &str,
        model: &str,
        input: &str,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError> {
        let input = ResponsesInput::Text(input.to_string());
        let request = ProviderGenerateRequest {
            model,
            input: &input,
            reasoning: None,
            tools: None,
            tool_choice: None,
            auth_bearer: None,
        };
        match self.provider {
            BrowserProvider::DeepSeek => {
                let client = DeepSeekClient::with_runtime(self.shared_runtime.clone());
                client
                    .generate_stream(ProviderGenerateStreamRequest { request_id, request, sender })
                    .await
            }
        }
    }

    pub async fn generate_demo_prompt_stream(
        &self,
        request_id: &str,
        model: &str,
        sender: Option<&dyn ResponseEventSink>,
    ) -> Result<ProviderOutcome, CoreError> {
        self.generate_text_stream(request_id, model, DEFAULT_DEMO_PROMPT, sender).await
    }
}

#[cfg(test)]
mod tests {
    use xrouter_core::CoreError;

    use super::{BrowserInferenceClient, BrowserProvider, DEFAULT_DEMO_PROMPT};

    #[test]
    fn provider_parser_rejects_unknown_provider() {
        let result = super::BrowserProvider::try_from("openai");
        assert!(result.is_err());
    }

    #[test]
    fn native_inference_reports_unsupported_platform() {
        let client = BrowserInferenceClient::new(
            BrowserProvider::DeepSeek,
            Some("https://api.deepseek.com".to_string()),
            Some("test".to_string()),
        );
        let result = futures::executor::block_on(client.generate_demo_prompt_stream(
            "request-1",
            "deepseek-chat",
            None,
        ));
        assert!(matches!(result, Err(CoreError::Provider(message)) if message.contains("wasm32")));
    }

    #[test]
    fn demo_prompt_stays_stable() {
        assert_eq!(DEFAULT_DEMO_PROMPT, "Hello, what can you do?");
    }

    #[test]
    fn cancel_is_idempotent_on_native() {
        let client = BrowserInferenceClient::new(
            BrowserProvider::DeepSeek,
            Some("https://api.deepseek.com".to_string()),
            Some("test".to_string()),
        );
        client.cancel("request-1").expect("cancel should be idempotent");
    }
}
