use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info_span, Instrument};
use uuid::Uuid;
use xrouter_contracts::{ResponseEvent, ResponsesRequest, ResponsesResponse, StageName, Usage};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("billing error: {0}")]
    Billing(String),
    #[error("client disconnected during {0:?}")]
    ClientDisconnected(StageName),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelState {
    Idle,
    Ingest,
    Tokenize,
    Hold,
    Generate,
    Finalize,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub request_id: String,
    pub state: KernelState,
    pub billing_enabled: bool,
    pub hold_acquired: bool,
    pub hold_released: bool,
    pub charge_committed: bool,
    pub charge_recovery_required: bool,
    pub recovered_externally: bool,
    pub client_connected: bool,
    pub billable_tokens: u32,
    pub external_ledger: u32,
    pub response_completed: bool,
    pub model: String,
    pub input: String,
    pub output_text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl ExecutionContext {
    fn new(request: ResponsesRequest, billing_enabled: bool) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            state: KernelState::Ingest,
            billing_enabled,
            hold_acquired: false,
            hold_released: false,
            charge_committed: false,
            charge_recovery_required: false,
            recovered_externally: false,
            client_connected: true,
            billable_tokens: 0,
            external_ledger: 0,
            response_completed: false,
            model: request.model,
            input: request.input,
            output_text: String::new(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOutcome {
    pub chunks: Vec<String>,
    pub output_tokens: u32,
}

#[async_trait]
pub trait ProviderClient: Send + Sync {
    async fn generate(&self, model: &str, input: &str) -> Result<ProviderOutcome, CoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeResult {
    Committed,
    AlreadyCommitted,
    RecoveryRequired,
    RecoveredExternally,
}

#[cfg(feature = "billing")]
#[async_trait]
pub trait UsageClient: Send + Sync {
    async fn acquire_hold(&self, request_id: &str, expected_tokens: u32) -> Result<(), CoreError>;
    async fn finalize_charge(
        &self,
        request_id: &str,
        billable_tokens: u32,
    ) -> Result<FinalizeResult, CoreError>;
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
        context.state =
            if context.billing_enabled { KernelState::Hold } else { KernelState::Generate };
        Ok(())
    }
}

#[cfg(feature = "billing")]
struct HoldHandler {
    usage_client: Arc<dyn UsageClient>,
}

#[cfg(feature = "billing")]
#[async_trait]
impl StageHandler for HoldHandler {
    fn stage(&self) -> StageName {
        StageName::Hold
    }

    async fn handle(&self, context: &mut ExecutionContext) -> Result<(), CoreError> {
        self.usage_client.acquire_hold(&context.request_id, context.input_tokens).await?;
        context.hold_acquired = true;
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
        if context.billing_enabled && !context.hold_acquired {
            return Err(CoreError::Billing(
                "billing enabled generate path requires acquired hold".to_string(),
            ));
        }

        let result = self.provider.generate(&context.model, &context.input).await?;

        context.output_tokens = result.output_tokens;
        context.billable_tokens = result.output_tokens;
        for chunk in result.chunks {
            context.output_text.push_str(&chunk);
            if context.client_connected {
                if let Some(sender) = &self.sender {
                    let _ = sender
                        .send(Ok(ResponseEvent::OutputTextDelta {
                            id: context.request_id.clone(),
                            delta: chunk,
                        }))
                        .await;
                }
            }
        }

        context.state = if context.billing_enabled {
            KernelState::Finalize
        } else {
            context.response_completed = true;
            KernelState::Done
        };
        Ok(())
    }
}

#[cfg(feature = "billing")]
struct FinalizeHandler {
    usage_client: Arc<dyn UsageClient>,
}

#[cfg(feature = "billing")]
#[async_trait]
impl StageHandler for FinalizeHandler {
    fn stage(&self) -> StageName {
        StageName::Finalize
    }

    async fn handle(&self, context: &mut ExecutionContext) -> Result<(), CoreError> {
        if !context.hold_acquired {
            return Err(CoreError::Billing("finalize requires an acquired hold".to_string()));
        }

        match self
            .usage_client
            .finalize_charge(&context.request_id, context.billable_tokens)
            .await?
        {
            FinalizeResult::Committed | FinalizeResult::AlreadyCommitted => {
                context.charge_committed = context.billable_tokens > 0;
                context.external_ledger =
                    context.external_ledger.saturating_add(context.billable_tokens);
            }
            FinalizeResult::RecoveryRequired => {
                context.charge_recovery_required = context.billable_tokens > 0;
            }
            FinalizeResult::RecoveredExternally => {
                context.recovered_externally = context.billable_tokens > 0;
            }
        }

        context.hold_acquired = false;
        context.hold_released = true;
        context.response_completed = context.charge_committed || context.billable_tokens == 0;
        context.state =
            if context.response_completed { KernelState::Done } else { KernelState::Failed };

        Ok(())
    }
}

pub struct ExecutionEngine {
    provider: Arc<dyn ProviderClient>,
    #[cfg(feature = "billing")]
    usage_client: Arc<dyn UsageClient>,
    billing_enabled: bool,
}

impl ExecutionEngine {
    #[cfg(feature = "billing")]
    pub fn new(
        provider: Arc<dyn ProviderClient>,
        usage_client: Arc<dyn UsageClient>,
        billing_enabled: bool,
    ) -> Self {
        Self { provider, usage_client, billing_enabled }
    }

    #[cfg(not(feature = "billing"))]
    pub fn new(provider: Arc<dyn ProviderClient>, billing_enabled: bool) -> Self {
        let _ = billing_enabled;
        Self { provider, billing_enabled: false }
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
        let mut context = ExecutionContext::new(request, self.billing_enabled);

        let ingest = IngestHandler;
        self.run_stage(&ingest, &mut context, disconnect_at).await?;

        let tokenize = TokenizeHandler;
        self.run_stage(&tokenize, &mut context, disconnect_at).await?;

        #[cfg(feature = "billing")]
        if context.billing_enabled {
            let hold = HoldHandler { usage_client: Arc::clone(&self.usage_client) };
            self.run_stage(&hold, &mut context, disconnect_at).await?;
        }

        let generate =
            GenerateHandler { provider: Arc::clone(&self.provider), sender: sender.clone() };
        self.run_stage(&generate, &mut context, disconnect_at).await?;

        #[cfg(feature = "billing")]
        if context.billing_enabled {
            let finalize = FinalizeHandler { usage_client: Arc::clone(&self.usage_client) };
            self.run_stage(&finalize, &mut context, disconnect_at).await?;
        }

        if context.state != KernelState::Done {
            context.state = KernelState::Failed;
            return Err(CoreError::Billing(
                "terminal state reached without successful settlement".to_string(),
            ));
        }

        if let Some(tx) = sender {
            let _ = tx
                .send(Ok(ResponseEvent::ResponseCompleted {
                    id: context.request_id.clone(),
                    usage: Usage {
                        input_tokens: context.input_tokens,
                        output_tokens: context.output_tokens,
                        total_tokens: context.input_tokens + context.output_tokens,
                    },
                }))
                .await;
        }

        Ok(ResponsesResponse {
            id: context.request_id,
            status: "completed".to_string(),
            output_text: context.output_text,
            usage: Usage {
                input_tokens: context.input_tokens,
                output_tokens: context.output_tokens,
                total_tokens: context.input_tokens + context.output_tokens,
            },
        })
    }

    async fn run_stage<H: StageHandler>(
        &self,
        handler: &H,
        context: &mut ExecutionContext,
        disconnect_at: Option<StageName>,
    ) -> Result<(), CoreError> {
        let stage = handler.stage();
        let span = info_span!(
            "pipeline_stage",
            request_id = %context.request_id,
            stage = ?stage,
            billing_enabled = context.billing_enabled,
            model = %context.model
        );

        async move {
            if disconnect_at.as_ref() == Some(&stage) {
                context.client_connected = false;
                match stage {
                    StageName::Ingest | StageName::Tokenize | StageName::Hold => {
                        context.state = KernelState::Failed;
                        context.hold_acquired = false;
                        return Err(CoreError::ClientDisconnected(stage));
                    }
                    StageName::Generate | StageName::Finalize => {}
                }
            }

            handler.handle(context).await
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct CoreFixture<'a> {
        name: &'a str,
        model: &'a str,
        input: &'a str,
        provider: ProviderBehavior,
        disconnect: Option<StageName>,
        billing_enabled: bool,
        finalize_result: FinalizeResult,
    }

    impl<'a> CoreFixture<'a> {
        fn parse(raw: &'a str) -> Self {
            let mut fixture = Self {
                name: "unnamed",
                model: "fake",
                input: "world",
                provider: ProviderBehavior::Success,
                disconnect: None,
                billing_enabled: false,
                finalize_result: FinalizeResult::Committed,
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
                            "hold" => Some(StageName::Hold),
                            "generate" => Some(StageName::Generate),
                            "finalize" => Some(StageName::Finalize),
                            other => panic!("unsupported disconnect fixture value: {other}"),
                        }
                    }
                    "billing_enabled" => {
                        fixture.billing_enabled = match value {
                            "true" => true,
                            "false" => false,
                            other => panic!("unsupported billing_enabled fixture value: {other}"),
                        }
                    }
                    "finalize" => {
                        fixture.finalize_result = match value {
                            "committed" => FinalizeResult::Committed,
                            "already_committed" => FinalizeResult::AlreadyCommitted,
                            "recovery_required" => FinalizeResult::RecoveryRequired,
                            "recovered_externally" => FinalizeResult::RecoveredExternally,
                            other => panic!("unsupported finalize fixture value: {other}"),
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
        async fn generate(&self, _model: &str, input: &str) -> Result<ProviderOutcome, CoreError> {
            match self.behavior {
                ProviderBehavior::Success => {
                    let chunks = vec!["hello ".to_string(), input.to_string()];
                    Ok(ProviderOutcome { output_tokens: 2, chunks })
                }
                ProviderBehavior::Fail => Err(CoreError::Provider("provider failed".to_string())),
            }
        }
    }

    #[cfg(feature = "billing")]
    struct FakeUsage {
        finalize_result: FinalizeResult,
    }

    #[cfg(feature = "billing")]
    #[async_trait]
    impl UsageClient for FakeUsage {
        async fn acquire_hold(
            &self,
            _request_id: &str,
            _expected_tokens: u32,
        ) -> Result<(), CoreError> {
            Ok(())
        }

        async fn finalize_charge(
            &self,
            _request_id: &str,
            _billable_tokens: u32,
        ) -> Result<FinalizeResult, CoreError> {
            Ok(self.finalize_result.clone())
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
            Ok(response) => format!(
                "kind=ok\nstatus={}\noutput={}\nusage_total={}",
                response.status, response.output_text, response.usage.total_tokens
            ),
            Err(error) => format!("kind=err\nerror_kind={}\nerror={}", error_kind(&error), error),
        }
    }

    fn error_kind(error: &CoreError) -> &'static str {
        match error {
            CoreError::Validation(_) => "Validation",
            CoreError::Provider(_) => "Provider",
            CoreError::Billing(_) => "Billing",
            CoreError::ClientDisconnected(_) => "ClientDisconnected",
        }
    }

    fn build_provider(behavior: ProviderBehavior) -> Arc<dyn ProviderClient> {
        Arc::new(FakeProvider { behavior })
    }

    #[cfg(feature = "billing")]
    fn build_engine(fixture: &CoreFixture<'_>) -> ExecutionEngine {
        ExecutionEngine::new(
            build_provider(fixture.provider),
            Arc::new(FakeUsage { finalize_result: fixture.finalize_result.clone() }),
            fixture.billing_enabled,
        )
    }

    #[cfg(not(feature = "billing"))]
    fn build_engine(fixture: &CoreFixture<'_>) -> ExecutionEngine {
        let _ = fixture.billing_enabled;
        ExecutionEngine::new(build_provider(fixture.provider), false)
    }

    async fn check_fixture(raw_fixture: &str, expected_snapshot: &str) {
        let fixture = CoreFixture::parse(raw_fixture);
        let fixture_name = fixture.name;
        let disconnect = fixture.disconnect.clone();
        let engine = build_engine(&fixture);
        let request = ResponsesRequest {
            model: fixture.model.to_string(),
            input: fixture.input.to_string(),
            stream: false,
        };
        let result = engine.execute_with_disconnect(request, disconnect).await;
        let actual_snapshot = render_result(result);
        assert_snapshot(fixture_name, &actual_snapshot, expected_snapshot);
    }

    #[tokio::test]
    async fn core_non_billing_fixtures() {
        let fixtures = [
            (
                r#"
name=non_billing_success
model=fake
input=world
provider=success
disconnect=none
billing_enabled=false
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
billing_enabled=false
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
billing_enabled=false
"#,
                r#"
kind=err
error_kind=ClientDisconnected
error=client disconnected during ingest
"#,
            ),
            (
                r#"
name=disconnect_generate_does_not_cancel
model=fake
input=world
provider=success
disconnect=generate
billing_enabled=false
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

    #[cfg(feature = "billing")]
    #[tokio::test]
    async fn core_billing_fixtures() {
        let fixtures = [
            (
                r#"
name=billing_success_committed
model=fake
input=world
provider=success
disconnect=none
billing_enabled=true
finalize=committed
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
name=billing_recovery_required_fails
model=fake
input=world
provider=success
disconnect=none
billing_enabled=true
finalize=recovery_required
"#,
                r#"
kind=err
error_kind=Billing
error=billing error: terminal state reached without successful settlement
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected).await;
        }
    }
}
