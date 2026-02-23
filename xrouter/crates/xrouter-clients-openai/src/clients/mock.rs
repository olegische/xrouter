use async_trait::async_trait;
use serde_json::Value;
use xrouter_contracts::{ReasoningConfig, ResponsesInput};
use xrouter_core::{CoreError, ProviderClient, ProviderOutcome};

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
        input: &ResponsesInput,
        _reasoning: Option<&ReasoningConfig>,
        _tools: Option<&[Value]>,
        _tool_choice: Option<&Value>,
    ) -> Result<ProviderOutcome, CoreError> {
        let mut chunks = Vec::new();
        let mut output_tokens = 0u32;

        let input_text = input.to_canonical_text();
        for token in input_text.split_whitespace() {
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

        Ok(ProviderOutcome {
            chunks,
            output_tokens,
            reasoning,
            reasoning_details: None,
            tool_calls: None,
            emitted_live: false,
        })
    }
}
