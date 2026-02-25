# xrouter Rust Architecture

This document describes stable architecture constraints for the Rust workspace
root (`xrouter/`).

The goal is explicit: migrate behavior from legacy Python into a typed Rust router
while keeping lifecycle semantics aligned with `formal/xrouter.tla`.

## Bird's-eye view

xrouter is a Responses-first LLM router with compatibility adapters.

- Core request/response model is Responses-first.
- Chat Completions is an adapter over Responses flow.
- Access-point mode is configurable:
  - `ENABLE_OPENAI_COMPATIBLE_API=false`: xrouter/openrouter-style paths (`/api/v1/...`),
  - `ENABLE_OPENAI_COMPATIBLE_API=true`: OpenAI-compatible paths (`/v1/...`).
- Core execution model: typed stage pipeline.
- Provider routing: model/provider resolution in orchestration layer, not in routes.
- Billing is out of active runtime scope.

Canonical stage sequence:

`ingest -> tokenize -> generate`

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
- `StageHandler` trait and stage handlers (`IngestHandler`, `TokenizeHandler`, `GenerateHandler`)
- `ProviderClient` trait as core-side boundary

Boundary intent:

- Core owns lifecycle semantics and stage transitions.
- Core depends on abstractions (`ProviderClient`) only.
- Core never depends on HTTP route handlers or provider-specific request schemas.

### `crates/xrouter-clients-openai`

Role: reusable OpenAI-compatible provider client implementation.

Boundary intent:

- Provider transport and provider payload behavior belong here.
- Route layer and core should consume this through `ProviderClient` trait.

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

### Lifecycle invariants

1. Stage names and ordering are canonical:
   `ingest -> tokenize -> generate`.
2. Disconnect behavior:
   - disconnect in `ingest|tokenize` fails fast,
   - disconnect in `generate` does not cancel the in-flight generation lifecycle.
3. Response completion is explicit and terminal (`done`).

### Contract invariants

1. Internal orchestration is Responses-first regardless of selected route surface.
2. OpenAI-compatible API is a route-layer feature toggle, not a provider identity.
3. Chat Completions is an adapter over Responses-first core flow.
4. Provider-specific contract quirks are isolated at boundaries (adapter/client), not in pipeline policy.
5. Streaming is first-class in core and exposed as typed events.

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

## Formal Model Scope

- Active lifecycle contract:
  - `formal/xrouter.tla`
  - `formal/xrouter.cfg`
  - `formal/property-map.md`
  - `formal/trace-schema.md`
- Legacy billing reference model (kept for future optional extension work):
  - `formal/xrouter_billing.tla`
  - `formal/xrouter_billing.cfg`
  - `formal/property-map-billing.md`
  - `formal/trace-schema-billing.md`

## Change policy

Any change that affects active lifecycle semantics must be reflected in:

- `formal/xrouter.tla`
- `formal/property-map.md`
- `formal/trace-schema.md`
- corresponding Rust code/tests in the same change

If billing semantics are intentionally reintroduced, update the legacy billing model set in the same change.

No contract-breaking behavior should be hidden behind silent fallback.
