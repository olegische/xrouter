use async_trait::async_trait;
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

pub struct OpenAiCompatibleClient {
    provider_id: String,
}

impl OpenAiCompatibleClient {
    pub fn new(provider_id: String) -> Self {
        Self { provider_id }
    }
}

#[async_trait]
impl ProviderClient for OpenAiCompatibleClient {
    async fn generate(&self, _model: &str, input: &str) -> Result<ProviderOutcome, CoreError> {
        let mut chunks = Vec::new();
        let mut output_tokens = 0u32;

        for token in input.split_whitespace() {
            output_tokens = output_tokens.saturating_add(1);
            chunks.push(format!("{} ", token));
        }

        if chunks.is_empty() {
            return Err(CoreError::Provider("provider returned empty output".to_string()));
        }

        chunks.insert(0, format!("[{}] ", self.provider_id));
        Ok(ProviderOutcome { chunks, output_tokens })
    }
}
