# xrouter WASM-Oriented Target Architecture

This document defines the target architecture for the Rust `xrouter/` workspace for the
WASM/browser track after the preparatory refactor for portability and modularity.

The immediate driver is browser-hosted reuse, but the architectural goal is broader:

- keep `xrouter-app` as a thin native composition root;
- keep routing and orchestration semantics explicit and testable;
- make provider behavior reusable outside the server binary;
- isolate transport/runtime details behind typed boundaries;
- preserve the formal lifecycle contract:
  `ingest -> tokenize -> generate -> done|failed`.

This document is not the canonical architecture for the whole repository. The general repository
architecture lives in the root `ARCHITECTURE.md`.

## Goals

1. Preserve current public behavior while improving internal boundaries.
2. Make core orchestration portable across native and browser-hosted runtimes.
3. Keep provider integrations reusable and independently testable.
4. Prevent `xrouter-app` from becoming the place where business logic, provider quirks, and
   transport code accumulate.
5. Enable future products such as:
   - browser/WASM SDKs,
   - alternative transports,
   - embedded library usage,
   - cleaner integration tests and mock transports.

## Non-Goals

1. Do not compile the full server app to WASM.
2. Do not move HTTP route concerns into core crates.
3. Do not introduce a service locator or container-heavy dependency injection.
4. Do not mix transport adaptation with orchestration policy.
5. Do not change lifecycle semantics without updating the formal model artifacts.

## Architectural Principles

### 1. Thin Composition Root

`xrouter-app` is responsible for:

- configuration loading;
- dependency wiring;
- route registration;
- transport adaptation between HTTP and core contracts;
- process startup and shutdown.

`xrouter-app` is not responsible for:

- provider-specific request shaping;
- provider-specific streaming parsing;
- orchestration policy;
- model normalization rules;
- runtime-independent domain logic.

### 2. Responses-First Core

The canonical internal flow remains Responses-first.

- `ResponsesRequest` and `ResponsesResponse` are the primary internal contract.
- Chat Completions remains an adapter at the route/provider boundary.
- Internal stages stay canonical:
  `ingest -> tokenize -> generate`.

### 3. Explicit Portability Boundary

Code intended for reuse across runtimes must not depend directly on:

- `axum`;
- `reqwest`;
- `tokio::sync::mpsc`;
- `tokio::spawn`;
- `tokio::time::sleep`;
- server-owned observability bootstrap;
- server-only TLS/network assumptions.

Instead, reusable code should depend on:

- typed contracts;
- traits for transport/runtime services;
- pure helper functions for provider normalization and parsing.

### 4. Provider Logic and Transport Are Separate

Provider code is split into two concerns:

1. provider behavior
   - request shaping
   - path selection
   - header requirements
   - event parsing
   - response normalization

2. transport/runtime implementation
   - HTTP execution
   - SSE byte acquisition
   - retry sleeping
   - bounded concurrency
   - cancellation and stream plumbing

This is the most important architectural split for long-term maintainability.

### 5. Observability Is Cross-Cutting, Not a Layer Violation

Tracing and OpenTelemetry are required, but they must not force native runtime assumptions into
portable logic.

Portable code may emit structured events or use lightweight instrumentation hooks, but runtime
ownership and exporter initialization must stay in `xrouter-observability` and the app layer.

## Target Workspace Shape

The exact crate names can be adjusted, but the target shape should look like this:

```text
xrouter/
  crates/
    xrouter-app/                   # native binary + HTTP composition root
    xrouter-contracts/             # canonical DTOs and wire contracts
    xrouter-core/                  # orchestration and lifecycle state machine
    xrouter-runtime/               # portable runtime traits/utilities (optional new crate)
    xrouter-provider-http/         # transport-neutral HTTP provider behavior (optional new crate)
    xrouter-clients-openai/        # native HTTP transport adapters for OpenAI-like providers
    xrouter-observability/         # tracing/OTel bootstrap
    xrouter-browser/               # future browser/WASM adapter crate
```

Two acceptable implementation paths exist:

1. Minimal-crate path
   - keep current crate count low,
   - refactor modules inside `xrouter-core` and `xrouter-clients-openai`,
   - add `xrouter-browser` later.

2. Cleaner-boundary path
   - introduce one or two new crates for portability boundaries,
   - keep transport-neutral code physically separate from native adapters.

The second path is cleaner for long-term reuse. The first path is acceptable if module boundaries
remain explicit and readable.

## Target Responsibilities by Crate

### `xrouter-contracts`

Responsibilities:

- canonical request/response DTOs;
- stream event types;
- adapter-facing wire models;
- serialization and schema metadata where supported.

Must not contain:

- HTTP clients;
- orchestration policy;
- provider-specific transport behavior;
- runtime primitives.

Portability expectations:

- should compile for `wasm32-unknown-unknown`;
- feature usage should be audited for `uuid`/schema derives;
- schema generation support may need feature gating if a dependency is not portable.

### `xrouter-core`

Responsibilities:

- execution lifecycle and stage semantics;
- model/provider identity normalization;
- validation and stage transitions;
- provider-facing trait boundary;
- stream/event orchestration;
- disconnect handling semantics.

Must not contain:

- `axum` route logic;
- provider-specific JSON shapes;
- direct `reqwest` usage;
- direct `tokio` channel types in public APIs;
- transport-owned retry/timer implementations.

Target shape inside the crate:

- `engine/`
- `pipeline/`
- `stages/ingest.rs`
- `stages/tokenize.rs`
- `stages/generate.rs`
- `models.rs`
- `provider.rs`
- `stream.rs`
- `errors.rs`

Important direction:

- replace runtime-specific stream plumbing with portable abstractions;
- keep the public provider interface typed and transport-agnostic.

### `xrouter-runtime` (optional new crate)

Responsibilities:

- runtime/service traits used by portable code.

Example traits:

- `HttpTransport`
- `SseEventStream`
- `Sleep`
- `ConcurrencyLimiter`
- `EventSink`
- `RequestIdGenerator`

This crate is useful if `xrouter-core` and provider logic both need the same abstractions and we
want to avoid circular layering.

### `xrouter-clients-openai`

Responsibilities after refactor:

- native adapters for OpenAI-compatible providers;
- transport binding to `reqwest`;
- native retry/backoff/concurrency mechanisms where needed;
- assembly of provider logic + transport implementation.

Must stop being the place where all concerns are mixed together.

Internal split should become explicit:

- `logic/`
  - payload shaping
  - normalization
  - event parsing
- `transport/native/`
  - `reqwest` transport
  - native retry/backoff
  - native stream decoding
- `providers/`
  - DeepSeek
  - OpenRouter
  - ZAI
  - Yandex
  - GigaChat
  - Xrouter

### `xrouter-observability`

Responsibilities:

- subscriber setup;
- OTel exporter setup;
- propagation bootstrap;
- app-owned instrumentation configuration.

Must not own:

- orchestration semantics;
- provider normalization;
- transport concerns.

Portable crates may depend on `tracing`, but should avoid requiring observability bootstrap code as
part of their correctness.

### `xrouter-app`

Responsibilities:

- process startup;
- config parsing;
- provider/client wiring;
- app state construction;
- route definitions;
- HTTP request extraction;
- HTTP response mapping;
- OpenAPI registration;
- auth extraction and HTTP-specific errors.

Recommended internal module layout:

```text
xrouter-app/src/
  main.rs
  lib.rs
  config.rs
  startup/
    mod.rs
    app_builder.rs
    provider_factory.rs
    model_catalog.rs
  http/
    mod.rs
    auth.rs
    errors.rs
    routes/
      mod.rs
      health.rs
      models.rs
      responses.rs
      chat_completions.rs
    docs.rs
  state.rs
```

This keeps `lib.rs` small and makes the composition root explicit.

## Target Dependency Direction

The dependency rule is:

```text
xrouter-app
  -> xrouter-observability
  -> xrouter-core
  -> xrouter-contracts

xrouter-app
  -> xrouter-clients-openai
  -> xrouter-core
  -> xrouter-contracts
```

Future browser direction:

```text
xrouter-browser
  -> xrouter-core
  -> xrouter-contracts
```

Potential shared abstraction direction:

```text
xrouter-core
  -> xrouter-runtime

xrouter-clients-openai
  -> xrouter-runtime
```

Forbidden dependency direction:

- `xrouter-core -> xrouter-app`
- `xrouter-core -> axum`
- `xrouter-core -> reqwest`
- provider logic depending on route handlers
- route handlers branching on provider-specific payload rules

## Target Runtime Abstractions

To support portability, the runtime-facing surface should be explicit.

### HTTP Transport

Portable provider logic should work against a trait such as:

```rust
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send_json(&self, request: HttpRequest) -> Result<HttpResponse, TransportError>;

    async fn open_sse(&self, request: HttpRequest)
        -> Result<Box<dyn SseEventStream>, TransportError>;
}
```

### Streaming Output

Core orchestration should not expose `tokio::mpsc` in its public interface.

Instead, use an event sink abstraction such as:

```rust
#[async_trait]
pub trait EventSink: Send {
    async fn emit(&self, event: ResponseEvent) -> Result<(), SinkError>;
}
```

Native route handlers can adapt this to Tokio channels or SSE responses.
Browser adapters can map it to browser stream/event mechanisms.

### Sleep and Retry

Backoff logic should depend on a trait, not directly on Tokio:

```rust
#[async_trait]
pub trait Sleep: Send + Sync {
    async fn sleep(&self, duration: Duration);
}
```

### Request Identity

Request ID generation should be injected or isolated so portability/testing does not depend on
global random generation behavior.

## Model Catalog Architecture

Model discovery must stop living implicitly inside app state assembly.

Target split:

- `ModelCatalogSource`
  - default static catalog
  - remote registry source
  - provider-specific registry adapter
- `ModelCatalogService`
  - merge/fallback/filter logic
- `AppState`
  - stores the resulting catalog only

Benefits:

- easier testing;
- app startup logic is clearer;
- remote registry concerns stop leaking into app state;
- browser/native reuse becomes possible later if needed.

## Provider Integration Architecture

Each provider integration should be organized around a consistent shape:

1. config
2. request builder
3. response parser
4. stream parser
5. normalization helpers
6. client adapter

Example:

```text
providers/deepseek/
  mod.rs
  config.rs
  request.rs
  response.rs
  stream.rs
  normalize.rs
  client.rs
```

This prevents provider-specific quirks from accumulating inside one very large `lib.rs`.

## HTTP Route Architecture

Route handlers should be thin adapters.

Each handler should do only:

1. parse request
2. resolve auth overrides
3. call app service / execution engine
4. map typed result into HTTP response

Route handlers should not:

- build provider-specific payloads;
- decide retry behavior;
- perform model catalog fetches;
- know about provider streaming wire quirks.

## Streaming Architecture

Streaming remains first-class, but the layers should be clearer.

Target layers:

1. provider transport stream
   - bytes/chunks from upstream
2. provider parser
   - upstream bytes to typed response deltas
3. core stream orchestration
   - typed deltas to canonical `ResponseEvent`
4. transport adapter
   - `ResponseEvent` to SSE/HTTP/browser surface

This split is essential for testing and portability.

## Testing Architecture

The testing strategy should follow the repository policy and align with the new boundaries.

### Unit Tests

Best targets:

- provider normalization helpers;
- SSE frame parsing;
- model identity canonicalization;
- config parsing;
- route adapter mapping.

### Scenario Tests

Best targets:

- request execution through the public engine boundary;
- HTTP route behavior using mock provider clients;
- model catalog fallback behavior;
- streaming success and failure paths.

### Portability Checks

Required when portability work starts:

- `cargo check -p xrouter-contracts --target wasm32-unknown-unknown`
- `cargo check -p xrouter-core --target wasm32-unknown-unknown`

Later:

- browser-target crate checks and tests.

## Migration Constraints

The following constraints remain mandatory during the refactor:

1. Stage semantics must stay aligned with the formal model.
2. `generate` remains the canonical final active stage, not `completion`.
3. Disconnect semantics must remain unchanged.
4. Temporary compatibility hacks must stay narrow, explicit, and removable.
5. Provider normalization must remain local to each provider boundary.

## Success Criteria

The architecture refactor is successful when:

1. `xrouter-app` is visibly a thin composition root.
2. `xrouter-core` no longer exposes native runtime types in its key public boundaries.
3. provider behavior is separable from native transport code.
4. route handlers are transport adapters only.
5. model catalog loading is a dedicated service/module.
6. future browser/WASM work can be added as a new adapter crate rather than as invasive changes
   across the whole workspace.
