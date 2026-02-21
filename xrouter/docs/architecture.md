# xrouter Rust Architecture

This document describes stable architecture constraints for the Rust workspace in
`/Users/olegromanchuk/Projects/xrouter/xrouter`.

The goal is explicit: migrate behavior from legacy Python into a typed Rust router
while keeping lifecycle semantics aligned with `formal/xrouter.tla`.

## Bird's-eye view

xrouter is a Responses-first LLM router with compatibility adapters.

- Canonical external contract: OpenAI Responses API.
- Compatibility contract: Chat Completions as an adapter over Responses flow.
- Core execution model: typed stage pipeline.
- Provider routing: model/provider resolution in orchestration layer, not in routes.
- Billing stages are optional and feature-gated.

Canonical stage sequence:

`ingest -> tokenize -> hold? -> generate -> finalize?`

## Code map

### Workspace root

- `Cargo.toml`: workspace members and shared dependency versions.
- `justfile`: project command entry points (`fmt`, `clippy`, `test`, `run`).
- `rustfmt.toml`: formatting policy.
- `.env.example`: local development configuration template.

### `crates/xrouter-contracts`

Role: canonical DTOs and wire-level event types.

Important types:

- `ResponsesRequest`, `ResponsesResponse`
- `ResponseEvent` (`output_text_delta`, `response_completed`, `response_error`)
- `ChatCompletionsRequest`, `ChatCompletionsResponse`

Boundary intent:

- This crate is pure data contract and mapping helpers.
- No provider transport logic and no orchestration policy.

### `crates/xrouter-core`

Role: orchestration and lifecycle state machine.

Important types:

- `ExecutionEngine`
- `ExecutionContext`
- `CoreError`
- `StageHandler` trait and stage handlers (`IngestHandler`, `TokenizeHandler`, `GenerateHandler`, and billing handlers behind feature flag)
- `ProviderClient` and `UsageClient` traits as core-side boundaries

Boundary intent:

- Core owns lifecycle semantics and stage transitions.
- Core depends on abstractions (`ProviderClient`, `UsageClient`) only.
- Core never depends on HTTP route handlers or provider-specific request schemas.

### `crates/xrouter-clients-openai`

Role: reusable OpenAI-compatible provider client implementation.

Boundary intent:

- Provider transport and provider payload behavior belong here.
- Route layer and core should consume this through `ProviderClient` trait.

### `crates/xrouter-clients-usage`

Role: reusable usage/billing client abstraction implementation.

Boundary intent:

- Billing finalization semantics and idempotency helpers stay here.
- Enabled in core only when billing feature is turned on.

### `crates/xrouter-observability`

Role: tracing and OpenTelemetry setup.

Boundary intent:

- Observability bootstrap and layers are centralized.
- Business logic and route handlers emit spans, but configuration lives here.

### `crates/xrouter-app`

Role: binary entry point and HTTP composition root.

Important modules:

- `main.rs`: startup, `.env` loading, config parsing, server bind.
- `config.rs`: environment-based configuration model.
- `lib.rs`: route wiring, adapter mapping, and app state composition.

Boundary intent:

- Routes handle transport concerns and request/response adaptation only.
- Routes do not implement provider-specific branches beyond model/provider resolution.
- Business flow always executes through `ExecutionEngine`.

## Architecture invariants (stable)

### Lifecycle and financial invariants

1. Stage names and ordering are canonical:
   `ingest -> tokenize -> hold? -> generate -> finalize?`.
2. `hold` and `finalize` are enabled only when billing integration is on.
3. Billing path may not generate billable output without acquired hold.
4. Disconnect behavior:
   - disconnect in `ingest|tokenize|hold` fails fast,
   - disconnect in `generate|finalize` does not cancel settlement.
5. Terminal states may not keep an acquired hold.
6. Billed terminal outcome must be explicit: commit OR recovery-required OR external recovery.
7. Finalize path must be idempotency-safe (no double commit).

### Contract invariants

1. Responses API is the source-of-truth external contract.
2. Chat Completions is an adapter over Responses-first core flow.
3. Provider-specific contract quirks are isolated at boundaries (adapter/client), not in pipeline policy.
4. Streaming is first-class in core and exposed as typed events.

### Dependency and layering invariants

1. `xrouter-core` depends on traits, not concrete HTTP clients.
2. Provider clients are reusable crates/modules, not route-level conditionals.
3. Composition root is explicit (`xrouter-app`), no service locator/container magic.
4. Transport (HTTP/SSE) is separated from orchestration semantics.

### Observability invariants

1. Stage execution emits spans with request/model/stage metadata.
2. Stream lifecycle and provider invocation paths are observable from day one.
3. Secret material must not be logged.

## Cross-cutting concerns

### Configuration

- App reads environment variables at startup (`AppConfig::from_env`).
- Local development `.env` is supported via `dotenvy`.
- Production should inject secrets through process environment/secrets manager.

### Testing

- Tests are co-located with implementation (`#[cfg(test)]`).
- Primary style is deterministic scenario tests through public boundaries.
- Current suites use data-driven fixtures and snapshot-like expected outputs.

## Change policy

Any change that affects lifecycle or financial semantics must be reflected in:

- `formal/xrouter.tla`
- `formal/property-map.md`
- `formal/trace-schema.md`
- corresponding Rust code/tests in the same change

No contract-breaking behavior should be hidden behind silent fallback.
