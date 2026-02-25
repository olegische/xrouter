# XRouter Rust Architecture

## Purpose

This document defines the target architecture for the Rust implementation in `xrouter/`.
Legacy Python code under `src/` is migration reference only.

## Architecture Principles

1. Responses API is the canonical external contract.
2. Chat Completions support exists as an adapter over the Responses flow.
3. Core orchestration is provider-agnostic and transport-agnostic.
4. Provider integrations are reusable client crates.
5. Composition root is explicit in the app crate; no hidden global DI.
6. All cross-boundary mappings are explicit and tested.
7. The active formal model in `formal/xrouter.tla` is the lifecycle source of truth.

## Active Lifecycle Contract

Canonical active stage flow:

`ingest -> tokenize -> generate(stream) -> done|failed`

Required semantics:

1. `ingest` validates and normalizes request input/context.
2. `tokenize` computes token metrics used for observability and output `usage`.
3. `generate` may stream chunks before terminal result.
4. Disconnect behavior:
   - disconnect in `ingest|tokenize` fails fast,
   - disconnect in `generate` does not cancel in-flight generation lifecycle.
5. Terminal success is explicit via `done` and response completion signaling.

## Legacy Billing Formalization

Billing semantics are not part of active runtime scope.

Billing formalization is preserved as reference material only:

- `formal/xrouter_billing.tla`
- `formal/xrouter_billing.cfg`
- `formal/property-map-billing.md`
- `formal/trace-schema-billing.md`

If billing is reintroduced later, those files are the starting point.

## Target Workspace Layout

This layout is a scaffold, not a fixed end state.

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
    xrouter-storage/              # Redis/cache/repository adapters
    xrouter-observability/        # tracing/logging/metrics
```

## Core Execution Flow

1. Parse and validate inbound request (Responses or Chat Completions adapter).
2. Resolve provider and model via model registry.
3. Build `ExecutionContext`.
4. Execute handler chain:
   - ingest and metadata enrichment,
   - tokenize,
   - generate.
5. Emit response stream or final payload in canonical contract.
6. Persist non-financial analytics and cleanup request resources.

## Provider and Client Model

1. `xrouter-core` depends on provider traits, not concrete HTTP clients.
2. Client crates expose typed request/response models and streaming parsers.
3. Provider adapters in core convert canonical request models to client models.
4. No route-level provider branching; provider selection happens in orchestration.
5. Shared behavior (retries, timeouts, headers, auth) belongs to client crates.

## Testing Strategy

1. Scenario integration tests are primary.
2. Required scenario matrix per critical flow:
   - success non-stream,
   - success stream,
   - provider error,
   - timeout/cancellation,
   - disconnect handling.
3. Keep property-driven test matrix aligned with `formal/property-map.md`.
4. Any lifecycle change must include TLC rerun and matching Rust tests.

## Change Policy

Any change that affects active lifecycle semantics must update:

- `formal/xrouter.tla`
- `formal/xrouter.cfg`
- `formal/property-map.md`
- `formal/trace-schema.md`

No contract-breaking behavior should be hidden behind silent fallback.
