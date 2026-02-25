use std::{sync::Arc, time::Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Instrument, error, info, info_span, warn};
use uuid::Uuid;
use xrouter_contracts::{
    ReasoningConfig, ResponseEvent, ResponseOutputItem, ResponseOutputText,
    ResponseReasoningSummary, ResponsesInput, ResponsesRequest, ResponsesResponse, StageName,
    ToolCall, ToolFunction, Usage,
};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("client disconnected during {0:?}")]
    ClientDisconnected(StageName),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelState {
    Idle,
    Ingest,
    Tokenize,
    Generate,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub request_id: String,
    pub state: KernelState,
    pub client_connected: bool,
    pub response_completed: bool,
    pub model: String,
    pub request_input: ResponsesInput,
    pub input: String,
    pub request_reasoning: Option<ReasoningConfig>,
    pub request_tools: Option<Vec<serde_json::Value>>,
    pub request_tool_choice: Option<serde_json::Value>,
    pub output_text: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub reasoning: Option<String>,
    pub reasoning_details: Option<Vec<serde_json::Value>>,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl ExecutionContext {
    fn new(request: ResponsesRequest) -> Self {
        let request_input = request.input.clone();
        let input = request_input.to_canonical_text();
        Self {
            request_id: Uuid::new_v4().to_string(),
            state: KernelState::Ingest,
            client_connected: true,
            response_completed: false,
            model: request.model,
            request_input,
            input,
            request_reasoning: request.reasoning,
            request_tools: request.tools,
            request_tool_choice: request.tool_choice,
            output_text: String::new(),
            tool_calls: None,
            reasoning: None,
            reasoning_details: None,
            input_tokens: 0,
            output_tokens: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutcome {
    pub chunks: Vec<String>,
    pub output_tokens: u32,
    pub reasoning: Option<String>,
    pub reasoning_details: Option<Vec<serde_json::Value>>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub emitted_live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: String,
    pub provider: String,
    pub description: String,
    pub context_length: u32,
    pub tokenizer: String,
    pub instruct_type: String,
    pub modality: String,
    pub top_provider_context_length: u32,
    pub is_moderated: bool,
    pub max_completion_tokens: u32,
}

pub fn synthesize_model_id(provider: &str, provider_model: &str) -> String {
    format!("{provider}/{provider_model}")
}

pub fn default_model_catalog() -> Vec<ModelDescriptor> {
    vec![
        ModelDescriptor {
            id: "gpt-4.1-mini".to_string(),
            provider: "openrouter".to_string(),
            description: "OpenRouter default chat model".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 16384,
        },
        ModelDescriptor {
            id: "anthropic/claude-3.5-sonnet".to_string(),
            provider: "openrouter".to_string(),
            description: "Anthropic Claude 3.5 Sonnet via OpenRouter".to_string(),
            context_length: 200000,
            tokenizer: "anthropic".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 200000,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "deepseek-chat".to_string(),
            provider: "deepseek".to_string(),
            description: "DeepSeek Chat is a general-purpose model tuned for fast conversational responses, coding assistance, and routine multi-turn tasks.".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "deepseek-reasoner".to_string(),
            provider: "deepseek".to_string(),
            description: "DeepSeek Reasoner is optimized for step-by-step reasoning on complex math, logic, and long multi-stage problem solving.".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 64000,
        },
        ModelDescriptor {
            id: "GigaChat-2".to_string(),
            provider: "gigachat".to_string(),
            description: "GigaChat 2 is Sber's base model for everyday conversational tasks and straightforward text generation.".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "GigaChat-2-Pro".to_string(),
            provider: "gigachat".to_string(),
            description: "GigaChat 2 Pro is positioned by Sber for tasks requiring stronger reasoning and coding than the base GigaChat 2 model.".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "GigaChat-2-Max".to_string(),
            provider: "gigachat".to_string(),
            description: "GigaChat 2 Max is Sber's flagship general-purpose model with the highest quality among the GigaChat 2 line.".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "yandexgpt/latest".to_string(),
            provider: "yandex".to_string(),
            description: "YandexGPT Pro 5 (latest branch): general-purpose Yandex model for complex generation tasks such as RAG, document analysis, reporting, and structured information extraction.".to_string(),
            context_length: 32768,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 32768,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "yandexgpt/rc".to_string(),
            provider: "yandex".to_string(),
            description: "YandexGPT Pro 5.1 (RC branch): release-candidate branch with improved function calling and structured output support before rollout to latest.".to_string(),
            context_length: 32768,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 32768,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "yandexgpt-lite/latest".to_string(),
            provider: "yandex".to_string(),
            description: "YandexGPT Lite 5 (latest branch): smallest and fastest Yandex text model, optimized for low-latency tasks like classification, formatting, and summarization.".to_string(),
            context_length: 32768,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 32768,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "aliceai-llm/latest".to_string(),
            provider: "yandex".to_string(),
            description: "Alice AI LLM (latest branch): Yandex flagship conversational model, strong on complex tasks and noticeably better for multi-turn chat and assistant scenarios.".to_string(),
            context_length: 32768,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 32768,
            is_moderated: true,
            max_completion_tokens: 8192,
        },
        ModelDescriptor {
            id: "llama3.1:8b".to_string(),
            provider: "ollama".to_string(),
            description: "Llama 3.1 8B via Ollama".to_string(),
            context_length: 8192,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 8192,
            is_moderated: true,
            max_completion_tokens: 4096,
        },
        ModelDescriptor {
            id: "glm-4.5".to_string(),
            provider: "zai".to_string(),
            description: "GLM-4.5 is Z.AI's flagship general model focused on strong coding, reasoning, and long-context agent workflows.".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 98304,
        },
        ModelDescriptor {
            id: "gpt-4.1-mini".to_string(),
            provider: "xrouter".to_string(),
            description: "XRouter GPT-4.1 mini".to_string(),
            context_length: 128000,
            tokenizer: "unknown".to_string(),
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128000,
            is_moderated: true,
            max_completion_tokens: 16384,
        },
    ]
}

#[async_trait]
pub trait ProviderClient: Send + Sync {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError>;

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let _ = request.request_id;
        let _ = request.sender;
        self.generate(request.request).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderGenerateRequest<'a> {
    pub model: &'a str,
    pub input: &'a ResponsesInput,
    pub reasoning: Option<&'a ReasoningConfig>,
    pub tools: Option<&'a [serde_json::Value]>,
    pub tool_choice: Option<&'a serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderGenerateStreamRequest<'a> {
    pub request_id: &'a str,
    pub request: ProviderGenerateRequest<'a>,
    pub sender: Option<&'a mpsc::Sender<Result<ResponseEvent, CoreError>>>,
}

#[async_trait]
pub trait StageHandler: Send + Sync {
    fn stage(&self) -> StageName;
    async fn handle(&self, context: &mut ExecutionContext) -> Result<(), CoreError>;
}

struct IngestHandler;

#[async_trait]
impl StageHandler for IngestHandler {
    fn stage(&self) -> StageName {
        StageName::Ingest
    }

    async fn handle(&self, context: &mut ExecutionContext) -> Result<(), CoreError> {
        if context.input.trim().is_empty() {
            return Err(CoreError::Validation("input must not be empty".to_string()));
        }
        context.state = KernelState::Tokenize;
        Ok(())
    }
}

struct TokenizeHandler;

#[async_trait]
impl StageHandler for TokenizeHandler {
    fn stage(&self) -> StageName {
        StageName::Tokenize
    }

    async fn handle(&self, context: &mut ExecutionContext) -> Result<(), CoreError> {
        context.input_tokens = context.input.split_whitespace().count() as u32;
        context.state = KernelState::Generate;
        Ok(())
    }
}

struct GenerateHandler {
    provider: Arc<dyn ProviderClient>,
    sender: Option<mpsc::Sender<Result<ResponseEvent, CoreError>>>,
}

#[async_trait]
impl StageHandler for GenerateHandler {
    fn stage(&self) -> StageName {
        StageName::Generate
    }

    async fn handle(&self, context: &mut ExecutionContext) -> Result<(), CoreError> {
        info!(
            event = "provider.request.started",
            provider_model = %context.model,
            input_chars = context.input.len()
        );
        let provider_started_at = Instant::now();
        let result = match self
            .provider
            .generate_stream(ProviderGenerateStreamRequest {
                request_id: &context.request_id,
                request: ProviderGenerateRequest {
                    model: &context.model,
                    input: &context.request_input,
                    reasoning: context.request_reasoning.as_ref(),
                    tools: context.request_tools.as_deref(),
                    tool_choice: context.request_tool_choice.as_ref(),
                },
                sender: self.sender.as_ref(),
            })
            .await
        {
            Ok(result) => result,
            Err(error) => {
                warn!(
                    event = "provider.request.failed",
                    provider_model = %context.model,
                    duration_ms = provider_started_at.elapsed().as_millis() as u64,
                    error = %error
                );
                return Err(error);
            }
        };
        info!(
            event = "provider.request.completed",
            provider_model = %context.model,
            output_tokens = result.output_tokens,
            chunk_count = result.chunks.len(),
            duration_ms = provider_started_at.elapsed().as_millis() as u64
        );

        context.output_tokens = result.output_tokens;
        context.tool_calls = result.tool_calls;
        context.reasoning = result.reasoning;
        context.reasoning_details = result.reasoning_details;
        if !result.emitted_live
            && context.client_connected
            && let (Some(reasoning), Some(sender)) = (&context.reasoning, &self.sender)
        {
            let _ = sender
                .send(Ok(ResponseEvent::ReasoningDelta {
                    id: context.request_id.clone(),
                    delta: reasoning.clone(),
                }))
                .await;
        }
        for chunk in result.chunks {
            context.output_text.push_str(&chunk);
            if !result.emitted_live
                && context.client_connected
                && let Some(sender) = &self.sender
            {
                let _ = sender
                    .send(Ok(ResponseEvent::OutputTextDelta {
                        id: context.request_id.clone(),
                        delta: chunk,
                    }))
                    .await;
            }
        }

        context.response_completed = true;
        context.state = KernelState::Done;
        Ok(())
    }
}

pub struct ExecutionEngine {
    provider: Arc<dyn ProviderClient>,
}

fn tool_call_id_from_response_id(response_id: &str) -> String {
    let suffix = response_id.strip_prefix("resp_").unwrap_or(response_id);
    format!("call_{suffix}")
}

fn parse_tool_call(output_text: &str, response_id: &str) -> Option<ToolCall> {
    let marker = "TOOL_CALL:";
    let marker_idx = output_text.find(marker)?;
    let payload = output_text.get(marker_idx + marker.len()..)?.trim();
    let (name_raw, args_raw) = payload.split_once(':')?;
    let name = name_raw.trim();
    let arguments = args_raw.trim();

    if name.is_empty() || arguments.is_empty() {
        return None;
    }
    if serde_json::from_str::<serde_json::Value>(arguments).is_err() {
        return None;
    }

    Some(ToolCall {
        id: tool_call_id_from_response_id(response_id),
        kind: "function".to_string(),
        function: ToolFunction { name: name.to_string(), arguments: arguments.to_string() },
    })
}

fn build_output_items(
    _response_id: &str,
    output_text: &str,
    reasoning: Option<String>,
    reasoning_details: Option<Vec<serde_json::Value>>,
    tool_calls: Option<Vec<ToolCall>>,
) -> Vec<ResponseOutputItem> {
    let mut output = Vec::new();

    output.push(ResponseOutputItem::Message {
        id: "msg_0".to_string(),
        role: "assistant".to_string(),
        content: vec![ResponseOutputText {
            kind: "output_text".to_string(),
            text: output_text.to_string(),
        }],
    });

    let has_reasoning_text = reasoning.as_ref().is_some_and(|value| !value.trim().is_empty());
    let reasoning_content = reasoning_details.unwrap_or_default();
    let has_reasoning_details = !reasoning_content.is_empty();
    if has_reasoning_text || has_reasoning_details {
        let summary = reasoning
            .filter(|value| !value.trim().is_empty())
            .map(|text| vec![ResponseReasoningSummary { text }])
            .unwrap_or_default();
        output.push(ResponseOutputItem::Reasoning {
            id: "rs_0".to_string(),
            summary,
            content: reasoning_content,
        });
    }

    if let Some(calls) = tool_calls {
        for call in calls {
            output.push(ResponseOutputItem::FunctionCall {
                id: format!("fc_{}", call.id),
                call_id: call.id,
                name: call.function.name,
                arguments: call.function.arguments,
            });
        }
    }

    output
}

impl ExecutionEngine {
    pub fn new(provider: Arc<dyn ProviderClient>) -> Self {
        Self { provider }
    }

    pub async fn execute(&self, request: ResponsesRequest) -> Result<ResponsesResponse, CoreError> {
        self.execute_with_disconnect(request, None).await
    }

    pub async fn execute_with_disconnect(
        &self,
        request: ResponsesRequest,
        disconnect_at: Option<StageName>,
    ) -> Result<ResponsesResponse, CoreError> {
        self.execute_internal(request, disconnect_at, None).await
    }

    pub fn execute_stream(
        self: Arc<Self>,
        request: ResponsesRequest,
        disconnect_at: Option<StageName>,
    ) -> ReceiverStream<Result<ResponseEvent, CoreError>> {
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(async move {
            let result = self
                .execute_internal(request, disconnect_at, Some(tx.clone()))
                .instrument(info_span!("execute_stream"))
                .await;
            if let Err(e) = result {
                let _ = tx
                    .send(Ok(ResponseEvent::ResponseError {
                        id: "unknown".to_string(),
                        message: e.to_string(),
                    }))
                    .await;
            }
        });
        ReceiverStream::new(rx)
    }

    async fn execute_internal(
        &self,
        request: ResponsesRequest,
        disconnect_at: Option<StageName>,
        sender: Option<mpsc::Sender<Result<ResponseEvent, CoreError>>>,
    ) -> Result<ResponsesResponse, CoreError> {
        let request_started_at = Instant::now();
        let mut context = ExecutionContext::new(request);
        info!(
            event = "core.request.started",
            request_id = %context.request_id,
            model = %context.model,
            stream = sender.is_some(),
            input_chars = context.input.len()
        );

        let ingest = IngestHandler;
        if let Err(error) = self.run_stage(&ingest, &mut context, disconnect_at.as_ref()).await {
            warn!(
                event = "core.request.failed",
                request_id = %context.request_id,
                model = %context.model,
                stage = "ingest",
                duration_ms = request_started_at.elapsed().as_millis() as u64,
                error = %error
            );
            return Err(error);
        }

        let tokenize = TokenizeHandler;
        if let Err(error) = self.run_stage(&tokenize, &mut context, disconnect_at.as_ref()).await {
            warn!(
                event = "core.request.failed",
                request_id = %context.request_id,
                model = %context.model,
                stage = "tokenize",
                duration_ms = request_started_at.elapsed().as_millis() as u64,
                error = %error
            );
            return Err(error);
        }

        let generate =
            GenerateHandler { provider: Arc::clone(&self.provider), sender: sender.clone() };
        if let Err(error) = self.run_stage(&generate, &mut context, disconnect_at.as_ref()).await {
            warn!(
                event = "core.request.failed",
                request_id = %context.request_id,
                model = %context.model,
                stage = "generate",
                duration_ms = request_started_at.elapsed().as_millis() as u64,
                error = %error
            );
            return Err(error);
        }

        if context.state != KernelState::Done {
            context.state = KernelState::Failed;
            error!(
                event = "core.request.failed",
                request_id = %context.request_id,
                model = %context.model,
                stage = "terminal",
                duration_ms = request_started_at.elapsed().as_millis() as u64,
                error = "terminal state reached without completion"
            );
            return Err(CoreError::Validation(
                "terminal state reached without completion".to_string(),
            ));
        }

        let tool_calls = context.tool_calls.clone().or_else(|| {
            parse_tool_call(&context.output_text, &context.request_id).map(|call| vec![call])
        });
        let finish_reason =
            if tool_calls.is_some() { "tool_calls".to_string() } else { "stop".to_string() };
        let output = build_output_items(
            &context.request_id,
            &context.output_text,
            context.reasoning.clone(),
            context.reasoning_details.clone(),
            tool_calls.clone(),
        );

        if let Some(tx) = sender {
            let _ = tx
                .send(Ok(ResponseEvent::ResponseCompleted {
                    id: context.request_id.clone(),
                    output: output.clone(),
                    finish_reason: finish_reason.clone(),
                    usage: Usage {
                        input_tokens: context.input_tokens,
                        output_tokens: context.output_tokens,
                        total_tokens: context.input_tokens + context.output_tokens,
                    },
                }))
                .await;
        }

        let response = ResponsesResponse {
            id: context.request_id,
            object: "response".to_string(),
            status: "completed".to_string(),
            output,
            finish_reason: finish_reason.clone(),
            usage: Usage {
                input_tokens: context.input_tokens,
                output_tokens: context.output_tokens,
                total_tokens: context.input_tokens + context.output_tokens,
            },
        };
        info!(
            event = "core.request.completed",
            request_id = %response.id,
            status = %response.status,
            finish_reason = %finish_reason,
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            total_tokens = response.usage.total_tokens,
            output_items = response.output.len(),
            duration_ms = request_started_at.elapsed().as_millis() as u64
        );
        Ok(response)
    }

    async fn run_stage<H: StageHandler>(
        &self,
        handler: &H,
        context: &mut ExecutionContext,
        disconnect_at: Option<&StageName>,
    ) -> Result<(), CoreError> {
        let stage = handler.stage();
        let stage_label = format!("{stage:?}");
        let span = info_span!(
            "pipeline_stage",
            request_id = %context.request_id,
            stage = ?stage,
            model = %context.model
        );

        async move {
            let stage_started_at = Instant::now();
            info!(event = "pipeline.stage.started");
            if disconnect_at == Some(&stage) {
                context.client_connected = false;
                match stage {
                    StageName::Ingest | StageName::Tokenize => {
                        context.state = KernelState::Failed;
                        warn!(
                            event = "pipeline.stage.disconnected",
                            duration_ms = stage_started_at.elapsed().as_millis() as u64
                        );
                        return Err(CoreError::ClientDisconnected(stage));
                    }
                    StageName::Generate => {}
                }
            }

            let result = handler.handle(context).await;
            match &result {
                Ok(()) => info!(
                    event = "pipeline.stage.completed",
                    state = ?context.state,
                    duration_ms = stage_started_at.elapsed().as_millis() as u64
                ),
                Err(error) => warn!(
                    event = "pipeline.stage.failed",
                    stage_name = %stage_label,
                    duration_ms = stage_started_at.elapsed().as_millis() as u64,
                    error = %error
                ),
            }
            result
        }
        .instrument(span)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ProviderBehavior {
        Success,
        Fail,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CoreFixture<'a> {
        name: &'a str,
        model: &'a str,
        input: &'a str,
        provider: ProviderBehavior,
        disconnect: Option<StageName>,
    }

    impl<'a> CoreFixture<'a> {
        fn parse(raw: &'a str) -> Self {
            let mut fixture = Self {
                name: "unnamed",
                model: "fake",
                input: "world",
                provider: ProviderBehavior::Success,
                disconnect: None,
            };

            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                let value = value.trim();

                match key {
                    "name" => fixture.name = value,
                    "model" => fixture.model = value,
                    "input" => fixture.input = value,
                    "provider" => {
                        fixture.provider = match value {
                            "success" => ProviderBehavior::Success,
                            "fail" => ProviderBehavior::Fail,
                            other => panic!("unsupported provider fixture value: {other}"),
                        }
                    }
                    "disconnect" => {
                        fixture.disconnect = match value {
                            "none" => None,
                            "ingest" => Some(StageName::Ingest),
                            "tokenize" => Some(StageName::Tokenize),
                            "generate" => Some(StageName::Generate),
                            other => panic!("unsupported disconnect fixture value: {other}"),
                        }
                    }
                    other => panic!("unsupported fixture key: {other}"),
                }
            }

            fixture
        }
    }

    struct FakeProvider {
        behavior: ProviderBehavior,
    }

    #[async_trait]
    impl ProviderClient for FakeProvider {
        async fn generate(
            &self,
            request: ProviderGenerateRequest<'_>,
        ) -> Result<ProviderOutcome, CoreError> {
            let input_text = request.input.to_canonical_text();
            match self.behavior {
                ProviderBehavior::Success => {
                    let chunks = vec!["hello ".to_string(), input_text];
                    Ok(ProviderOutcome {
                        output_tokens: 2,
                        chunks,
                        reasoning: None,
                        reasoning_details: None,
                        tool_calls: None,
                        emitted_live: false,
                    })
                }
                ProviderBehavior::Fail => Err(CoreError::Provider("provider failed".to_string())),
            }
        }
    }

    fn assert_snapshot(name: &str, actual: &str, expected: &str) {
        let actual = actual.trim();
        let expected = expected.trim();
        assert_eq!(
            actual, expected,
            "snapshot mismatch for fixture `{name}`\n\nactual:\n{actual}\n\nexpected:\n{expected}"
        );
    }

    fn render_result(result: Result<ResponsesResponse, CoreError>) -> String {
        match result {
            Ok(response) => {
                let output_text = response
                    .output
                    .iter()
                    .find_map(|item| {
                        if let ResponseOutputItem::Message { content, .. } = item {
                            content.first().map(|part| part.text.as_str())
                        } else {
                            None
                        }
                    })
                    .unwrap_or("");
                format!(
                    "kind=ok\nstatus={}\noutput={}\nusage_total={}",
                    response.status, output_text, response.usage.total_tokens
                )
            }
            Err(error) => format!("kind=err\nerror_kind={}\nerror={}", error_kind(&error), error),
        }
    }

    fn error_kind(error: &CoreError) -> &'static str {
        match error {
            CoreError::Validation(_) => "Validation",
            CoreError::Provider(_) => "Provider",
            CoreError::ClientDisconnected(_) => "ClientDisconnected",
        }
    }

    fn build_provider(behavior: ProviderBehavior) -> Arc<dyn ProviderClient> {
        Arc::new(FakeProvider { behavior })
    }

    fn build_engine(fixture: &CoreFixture<'_>) -> ExecutionEngine {
        ExecutionEngine::new(build_provider(fixture.provider))
    }

    async fn check_fixture(raw_fixture: &str, expected_snapshot: &str) {
        let fixture = CoreFixture::parse(raw_fixture);
        let fixture_name = fixture.name;
        let disconnect = fixture.disconnect.clone();
        let engine = build_engine(&fixture);
        let request = ResponsesRequest {
            model: fixture.model.to_string(),
            input: xrouter_contracts::ResponsesInput::Text(fixture.input.to_string()),
            stream: false,
            reasoning: None,
            tools: None,
            tool_choice: None,
        };
        let result = engine.execute_with_disconnect(request, disconnect).await;
        let actual_snapshot = render_result(result);
        assert_snapshot(fixture_name, &actual_snapshot, expected_snapshot);
    }

    #[tokio::test]
    async fn core_pipeline_fixtures() {
        let fixtures = [
            (
                r#"
name=success
model=fake
input=world
provider=success
disconnect=none
"#,
                r#"
kind=ok
status=completed
output=hello world
usage_total=3
"#,
            ),
            (
                r#"
name=provider_error
model=fake
input=world
provider=fail
disconnect=none
"#,
                r#"
kind=err
error_kind=Provider
error=provider error: provider failed
"#,
            ),
            (
                r#"
name=disconnect_ingest_fails_fast
model=fake
input=world
provider=success
disconnect=ingest
"#,
                r#"
kind=err
error_kind=ClientDisconnected
error=client disconnected during Ingest
"#,
            ),
            (
                r#"
name=disconnect_generate_does_not_cancel
model=fake
input=world
provider=success
disconnect=generate
"#,
                r#"
kind=ok
status=completed
output=hello world
usage_total=3
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected).await;
        }
    }
}
