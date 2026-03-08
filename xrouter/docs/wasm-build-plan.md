# xrouter WASM Build Plan

This document describes the recommended path for making `xrouter` usable from a browser-hosted WASM application.

The target consumer is a browser runtime such as `codex-wasm`, where model/provider routing should work without depending on a native server process.

## Goal

Enable `xrouter` logic to be reused from a browser-hosted WASM application without porting the entire HTTP server into WASM.

Concretely, the desired outcome is:

- a browser app can select a provider-backed model such as `xrouter/zai-org/GLM-4.7-Flash`;
- routing, provider-specific request shaping, and stream normalization can be reused from `xrouter`;
- network I/O runs through browser-safe `fetch`, not a native server runtime;
- `xrouter-app` remains the server composition root and is not treated as the WASM target.

## Non-Goal

The goal is not to compile the entire workspace to WASM.

In particular, `crates/xrouter-app` should not be the primary WASM target because it is an Axum HTTP application and composition root, not a portable library boundary.

## Current State

The current workspace is already partially well-structured for this work:

- `crates/xrouter-contracts`
  - canonical DTOs and event types
  - good candidate for direct reuse in WASM
- `crates/xrouter-core`
  - orchestration and provider boundary (`ProviderClient`)
  - conceptually portable, but currently tied to Tokio and observability/runtime details
- `crates/xrouter-clients-openai`
  - provider-specific request/stream logic
  - currently coupled to `reqwest`, Tokio channels/semaphores, and native async runtime assumptions
- `crates/xrouter-app`
  - Axum routes, HTTP server, auth extraction, OpenAPI, SSE response layer
  - should remain native/server-only

## Recommended Architecture

The recommended architecture is:

1. Keep `xrouter-app` native-only.
2. Reuse `xrouter-contracts` directly in WASM.
3. Make `xrouter-core` portable enough to compile for `wasm32-unknown-unknown`.
4. Extract a new browser-safe client layer from `xrouter-clients-openai`.
5. Inject transport through a trait so the browser host can provide `fetch`.

The key rule is:

`xrouter` should expose a reusable library boundary for browser consumers, rather than expecting browser consumers to embed the server crate.

## What Should Be Reused

The following logic is worth reusing in a WASM build:

- model/provider identity normalization
- provider-specific payload shaping
- SSE/event parsing and normalization
- responses/chat-completions compatibility behavior
- typed request/response contracts from `xrouter-contracts`
- orchestration semantics from `xrouter-core`, if they can be kept portable

## What Should Not Be Reused Directly

The following pieces should not be ported into browser WASM as-is:

- Axum router and HTTP handlers from `xrouter-app`
- server bind/startup
- native environment/config bootstrap in the app layer
- native TLS/network assumptions tied to `reqwest` server-side usage
- observability bootstrap that assumes server/runtime ownership

## Recommended Refactor Boundary

The most important change is to separate:

- provider behavior
- transport implementation

Today, `xrouter-clients-openai` mixes:

- request shaping
- retry/backoff
- header injection
- SSE parsing
- `reqwest` transport
- Tokio concurrency primitives

For WASM, that crate should be split conceptually into:

### 1. Provider Logic Layer

A portable layer that knows:

- how to build upstream requests
- which path to call (`/responses`, `/chat/completions`, `/models`, etc.)
- how to parse streaming events
- how to normalize provider quirks

This layer should not know about `reqwest::Client`.

### 2. Transport Layer

A trait-based interface, for example:

```rust
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn send_json(
        &self,
        request: HttpRequest,
    ) -> Result<HttpResponse, TransportError>;

    async fn send_sse(
        &self,
        request: HttpRequest,
    ) -> Result<Box<dyn SseEventStream>, TransportError>;
}
```

Native/server builds can implement this with `reqwest`.

Browser/WASM builds can implement this with:

- `web_sys::Request`
- `fetch`
- `ReadableStream`
- browser-native SSE frame parsing

### 3. Optional Runtime Utilities Layer

If concurrency limits, retries, and timers stay in shared code, they should depend on portable abstractions rather than directly on:

- `tokio::time::sleep`
- `tokio::sync::Semaphore`
- `tokio::sync::mpsc`

If needed, introduce lightweight abstractions for:

- sleep/backoff
- bounded concurrency
- event sink / stream sink

## Proposed Crate Direction

The cleanest path is to add a new crate instead of forcing the existing app crate into WASM.

Recommended addition:

- `crates/xrouter-browser` or `crates/xrouter-wasm`

Its job would be:

- provide browser-safe transport adapters
- expose a minimal public API for browser consumers
- reuse `xrouter-contracts`
- reuse portable parts of `xrouter-core`
- reuse refactored provider logic from `xrouter-clients-openai`

An acceptable alternative is feature-gating `xrouter-clients-openai`, but only if the code remains readable. If feature flags make the crate hard to maintain, a dedicated browser crate is better.

## Suggested Milestones

### Milestone 1: Contracts Compile for WASM

Target:

- `xrouter-contracts` compiles for `wasm32-unknown-unknown`

Acceptance:

- `cargo check -p xrouter-contracts --target wasm32-unknown-unknown`

### Milestone 2: Core Compile Audit

Target:

- determine whether `xrouter-core` already compiles for `wasm32`
- identify exact blockers

Likely blockers:

- Tokio runtime assumptions
- tracing/opentelemetry coupling
- UUID feature configuration for WASM if random generation is used

Acceptance:

- documented blocker list
- first `cargo check -p xrouter-core --target wasm32-unknown-unknown`

### Milestone 3: Transport Abstraction

Target:

- remove direct `reqwest` dependency from provider behavior layer
- introduce transport trait(s)

Acceptance:

- provider logic compiles without `reqwest`
- native implementation still works via an adapter

### Milestone 4: Browser-Safe Client Layer

Target:

- implement a browser transport adapter
- support at least one provider end-to-end

Recommended first provider:

- `deepseek`

Why:

- simpler than full router-first adoption
- immediately useful for external browser consumers

Acceptance:

- browser/WASM build can execute one streamed turn against DeepSeek through the shared client logic

### Milestone 5: xrouter Provider in Browser

Target:

- support `xrouter` itself as a provider target from a browser consumer

This means a browser app can point at:

- `base_url = ".../api/v1"`
- `model_provider = "xrouter"`

and use the shared browser-safe client path.

Acceptance:

- browser/WASM consumer completes a streamed turn through an `xrouter` endpoint

## Integration Guidance for codex-wasm

For a browser-hosted Codex integration, the recommended order is:

1. Implement provider override support in the browser host config surface.
2. Validate direct provider usage first, for example DeepSeek.
3. After that, integrate `xrouter` as another provider endpoint.
4. Only then decide whether deeper `xrouter` library reuse is worth the maintenance cost.

This order matters because it separates:

- provider override support in the consumer
- library reuse inside `xrouter`

Those are related, but not the same task.

## Why Not Compile the Whole App to WASM

Compiling `xrouter-app` to WASM would create the wrong dependency direction:

- browser code would embed route handlers and server composition concerns;
- server-only dependencies would leak into the portability boundary;
- the resulting API would be harder to reuse than a dedicated browser-facing library.

The WASM target should be a library product, not a server binary in disguise.

## Risks

The main engineering risks are:

- overusing `cfg(target_arch = "wasm32")` inside existing crates until they become hard to reason about
- coupling portable logic to `reqwest` or Tokio details
- keeping observability in the hot path of portability work
- trying to solve browser transport and server transport in the same abstraction layer too early

## Recommendation

The recommendation to the `xrouter` team is:

- do not port `xrouter-app` to WASM;
- make `xrouter-contracts` and selected `xrouter-core` paths portable;
- extract provider logic away from `reqwest`;
- create a dedicated browser/WASM library boundary;
- treat browser transport as an injected capability, not as a special case of the server app.

That gives the best chance of reusing `xrouter` in `codex-wasm` without creating a second, divergent implementation.
