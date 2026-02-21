# XRouter Rust Architecture

## Purpose

This document defines the target architecture for the new Rust implementation in `xrouter/`.
Legacy Python code under `src/` is used only as migration reference.

## Key Findings From Legacy Code

### Patterns to keep

1. Handler pipeline for request processing with stable stage semantics.
2. Clear request/response mapping boundaries for provider-specific formats.
3. Explicit context object carrying request lifecycle state.
4. Multi-provider routing with feature flags and model resolution.

### Patterns to replace

1. Global/container-heavy dependency injection and service locator usage.
2. Tight coupling between API routes, provider factories, and provider-specific branches.
3. Provider abstractions that mix transport, orchestration, and policy logic.
4. Transitional Responses API compatibility hacks as permanent behavior.

## Known Legacy Shortcuts Not To Migrate

1. Placeholder token estimation defaults used as business values.
2. Implicit synthetic-success fallbacks for external billing/usage calls.
3. Route-level compatibility hacks that blur canonical contract semantics.

## Provider Normalization Patterns (Reference Baseline)

1. Apply normalization only for the intended provider; keep other providers pass-through.
2. Normalize contracts at ingestion/output boundaries, not in unrelated core logic.
3. Prefer bounded fail-soft parsing for legacy payloads:
   - known-safe repair attempts only,
   - no hidden semantic rewrites,
   - preserve original payload when repair is not applicable.
4. Keep compatibility hacks isolated and explicitly temporary.
5. Implement normalization with small pure helpers and exhaustive scenario tests.
6. Emit structured request/normalization summaries with sensitive-data redaction.

## Architecture Principles

1. Responses API is the canonical external contract.
2. Chat Completions support exists as an adapter over the Responses flow.
3. Core orchestration is provider-agnostic and transport-agnostic.
4. Provider integrations are reusable client crates.
5. Composition root is explicit in the app crate; no hidden global DI.
6. All cross-boundary mappings are explicit and tested.
7. The formal model in `formal/xrouter.tla` is the lifecycle source of truth for request execution and billing settlement semantics.

## Target Workspace Layout

This layout is an implementation scaffold, not a fixed end state.
During delivery, the Codex implementer may split, merge, or rename crates if it improves maintainability and delivery speed.
When this happens, update docs in the same change and keep canonical flow/contract principles unchanged.

```text
xrouter/
  Cargo.toml
  crates/
    xrouter-app/                  # binary crate, HTTP server, composition root
    xrouter-core/                 # use-cases, handler chain, domain services
    xrouter-contracts/            # canonical DTOs (Responses-first + internal)
    xrouter-clients-openai/       # reusable OpenAI-compatible client
    xrouter-clients-openrouter/   # reusable OpenRouter client
    xrouter-clients-gigachat/     # reusable GigaChat client
    xrouter-clients-usage/        # reusable usage/billing client
    xrouter-storage/              # Redis/cache/repository adapters
    xrouter-observability/        # tracing/logging/metrics
  # no top-level tests directory by default; prefer co-located tests
```

## Core Execution Flow

1. Parse and validate inbound request (Responses or Chat Completions adapter).
2. Resolve provider and model via model registry service.
3. Build `ExecutionContext`.
4. Execute handler chain:
   - request ingest and metadata enrichment,
   - token estimation,
   - hold / limit enforcement (billing feature-gated),
   - model generation execution,
   - finalize (usage accounting, hold finalization, analytics) (billing feature-gated).
5. Emit response stream or final payload in canonical contract.
6. Persist analytics/usage and cleanup resources.

## Canonical Handler Stage Names

Use neutral, contract-independent stage names:

1. `ingest`: normalize request shape and enrich metadata/context.
2. `tokenize`: estimate token usage for policy and billing decisions.
3. `hold` (optional): enforce limits / create hold when billing is enabled.
4. `generate`: execute model inference/generation (not named `completion`).
5. `finalize` (optional): finalize hold and send usage/analytics when billing is enabled.

## Formal-to-Implementation Semantics

Implementation must preserve these model-level guarantees:

1. Billing gate:
   - no billed generation without acquired hold.
2. Streaming post-paid:
   - generation may emit chunks before final settlement.
3. Disconnect behavior:
   - early stages (`ingest`, `tokenize`, `hold`) fail fast on disconnect,
   - `generate`/`finalize` continue settlement even after disconnect.
4. Hold lifecycle:
   - `done`/`failed` cannot retain acquired hold.
5. Recovery lifecycle:
   - unresolved recovery blocks reset,
   - recovery resolution is explicit and observable.
6. Financial outcome:
   - billed token generation must eventually end in one of:
     - committed charge,
     - explicit recovery-required state,
     - explicit external recovery settlement marker.

## Financial State Semantics

1. `externalLedger` represents committed charge effects produced by this service path.
2. External recovery settlement is a separate semantic outcome and must not be silently merged into `externalLedger`.
3. Reset logic must never erase financial history (`externalLedger`) or bypass unresolved debt obligations.
4. Finalize/commit path must be idempotency-safe (no double charge commit).

## Handler Chain Design

1. Each handler implements a typed async trait.
2. Each handler declares preconditions and failure semantics.
3. Handlers only modify fields owned by the context contract.
4. Handlers must be deterministic and side effects must be observable in tests.
5. Billing handlers are feature-gated and inserted explicitly.

## Provider and Client Model

1. `xrouter-core` depends on provider traits, not concrete HTTP clients.
2. Client crates expose:
   - typed request/response models,
   - streaming parsers,
   - typed error mapping from HTTP/provider errors.
3. Provider adapters in core convert canonical request models to client models.
4. No route-level provider branching; provider selection happens in orchestration.
5. Shared behavior (retries, timeouts, headers, auth) belongs to client crates.

## API Contract Strategy

1. Canonical contract: OpenAI Responses API.
2. Chat Completions route is a compatibility layer:
   - input mapped to canonical internal request,
   - output mapped back to Chat Completions schema.
3. Legacy xrouter-openrouter deviations are transitional and must be documented.
4. Streaming must use explicit event typing and completion semantics.
5. Contract compatibility tests are mandatory for both streaming and non-streaming paths.

## Dependency Injection Strategy

1. Use constructor injection and explicit app wiring (`AppState`/`ServiceGraph`).
2. Prefer generics for compile-time composition where practical.
3. Use trait objects (`Arc<dyn Trait>`) only at runtime boundaries.
4. Avoid global mutable singletons and hidden container state.

## Error and Resilience Model

1. Define a typed domain error enum with transport/domain/validation variants.
2. Preserve provider error codes and request identifiers in mapped errors.
3. Make fallback behavior explicit, narrow, and observable.
4. Fallbacks must emit structured warnings and metrics.
5. No silent degradation for contract-breaking conditions.

## Observability

1. Use structured tracing with request, model, provider, and generation identifiers.
2. Add spans for each handler stage and external API call.
3. Use OpenTelemetry spans for every LLM invocation (request, stream lifecycle, completion/error).
4. Propagate trace context to downstream provider client calls.
5. Record latency and failure metrics per provider and endpoint.
6. Redact secrets and credentials in logs by default.

## Testing Strategy

1. Scenario integration tests are primary.
2. Test matrix per critical flow:
   - success non-stream,
   - success stream,
   - provider error,
   - billing error and recovery,
   - timeout/cancellation.
3. Assert four layers where applicable:
   - returned payload/events,
   - persisted state,
   - in-memory context state,
   - outbound request contract.
4. Keep primary tests co-located with implementation modules (`#[cfg(test)] mod tests`).
5. Use a separate integration `tests/` directory only for cross-crate/e2e scenarios.
6. Use local mocks/fakes only; avoid real network in CI.
7. For core flow, keep a property-driven test matrix aligned with `formal/property-map.md`.
8. Any lifecycle change must include a TLC rerun and corresponding Rust test updates for affected properties.

## Migration Plan

1. Implement `xrouter-contracts` and `xrouter-core` with Responses-first models.
2. Implement one provider client crate end-to-end (recommended: OpenRouter or OpenAI-compatible).
3. Implement handler chain and Responses endpoint in `xrouter-app`.
4. Add Chat Completions compatibility adapter.
5. Add billing/usage integration as optional handler stage.
6. Migrate remaining providers as independent client crates.
7. Remove legacy Python `src/` once parity criteria are met.
