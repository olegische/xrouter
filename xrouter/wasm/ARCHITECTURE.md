# WASM Architecture

This document describes the target architecture for the browser/WASM track inside `xrouter/`.

It assumes the main Rust refactor is already complete:

1. `xrouter-app` is the native composition root
2. `xrouter-core` exposes a sink-based streaming boundary
3. provider request shaping, native transport, and stream parsing are separated in
   `xrouter-clients-openai`

The goal is not to make the server "sort of compile to WASM".
The goal is to produce a second XRouter product that runs natively in the browser with
bring-your-own-key (BYOK), while sharing as much Rust logic as possible with the server product.

Target product shape:

1. a server XRouter product
2. a browser/WASM XRouter product
3. one shared portable Rust core used by both

## Demo Target

The first browser demo is intentionally narrow.

User flow:

1. user opens a browser page
2. user pastes an API key
3. user selects a provider
4. browser fetches that provider's available models
5. browser shows the model list
6. user selects a model
7. browser sends a canonical request with the prompt `Hello, what can you do?`
8. provider response is rendered as a live stream

This demo is the architectural slice to optimize for.
Anything outside this slice is secondary until this path works end to end.

## Product Model

The intended end state is two different products, not one product with a thin browser shell:

1. server product
   native HTTP router, native transport, server deployment
2. browser product
   browser runtime, browser transport, no server dependency

Both products should be assembled from shared portable crates wherever possible.

The portability target is therefore:

1. maximize reuse of existing logic
2. minimize browser-only reimplementation
3. keep browser build free of server-layer dependencies

## Bird's Eye View

The browser/WASM target should reuse:

1. canonical contracts from `xrouter-contracts`
2. orchestration semantics from `xrouter-core`
3. provider request shaping and response parsing from `xrouter-clients-openai`

The browser/WASM target must not depend on:

1. `xrouter-app`
2. Axum HTTP routes
3. Tokio runtime plumbing
4. native `reqwest` transport
5. server-owned observability bootstrap
6. startup-only model catalog assembly currently owned by `xrouter-app`

Important clarification:

1. browser work should maximize reuse of XRouter code
2. but reuse must come through portable crates or code extracted out of `xrouter-app`
3. browser code must not take a direct dependency on the server composition root

At a high level, the target shape is:

```text
browser UI
  -> wasm crate facade
  -> browser-safe transport + stream reader
  -> shared provider protocol/parser logic
  -> xrouter-core execution engine
  -> xrouter-contracts DTOs
```

For the first demo, there is also a second browser path:

```text
browser UI
  -> model discovery adapter
  -> provider /models endpoint
```

## Code Map

### `xrouter/crates/xrouter-contracts`

This is the primary shared contract boundary for the browser layer.

If you need:

1. Responses request/response types
2. Chat Completions adapter-facing types
3. stream event types

start here.

This crate is also the correct home for any browser-visible canonical DTOs added for the demo.

### `xrouter/crates/xrouter-core`

This is the reusable orchestration boundary.

Relevant types:

1. `ExecutionEngine`
2. `ProviderClient`
3. `ProviderOutcome`
4. `ResponseEventSink`
5. `ModelDescriptor`

What is already good for WASM:

1. public streaming boundary no longer exposes Tokio channel types
2. orchestration owns lifecycle semantics instead of runtime plumbing
3. per-request bearer override already exists in the execution flow

What still needs care:

1. compile audit for `wasm32-unknown-unknown`
2. request-id generation and any remaining portability-sensitive dependencies
3. whether sink implementations require async/runtime assumptions that do not hold in the browser

### `xrouter/crates/xrouter-clients-openai`

This crate already contains the split needed for browser work:

1. `protocol.rs`
   request shaping
2. `parser.rs`
   response parsing and SSE/event normalization
3. `transport.rs`
   native HTTP/runtime implementation
4. `clients/`
   provider-local behavior

For WASM, the important point is:

1. `protocol.rs` and `parser.rs` are the likely reuse surface
2. `transport.rs` is native-only and must not leak into the browser path
3. provider clients like `DeepSeekClient` already support per-request bearer override, which fits
   BYOK

### `xrouter/crates/xrouter-app`

This crate is explicitly out of scope for direct WASM reuse.

It should remain native-only.

Important consequence for the demo:

1. `startup/model_catalog.rs` and `startup/model_catalog_remote.rs` currently own model loading
2. that logic cannot remain server-owned if the browser must fetch provider models directly
3. browser model discovery therefore needs its own portable boundary
4. if `xrouter-app` contains logic the browser needs, that logic should move out into a shared
   crate instead of becoming a browser dependency on `xrouter-app`

### `xrouter/wasm`

This directory is the WASM track area.

It should contain:

1. architecture and plan documents
2. browser-track notes/checklists if needed
3. preparation material for the eventual code implementation

It is not the final location of the Rust crate.

If a real Rust crate is introduced, it should still live under `xrouter/crates/`, for example:

1. `xrouter/crates/xrouter-browser`
2. or `xrouter/crates/xrouter-wasm`

## Required Runtime Boundaries

### Core vs Browser Transport

`xrouter-core` should continue to own:

1. lifecycle semantics
2. provider orchestration
3. stream event semantics

The browser layer should own:

1. `fetch`-based HTTP execution
2. browser-safe request header injection
3. browser stream reading from `ReadableStream`
4. browser-specific retry/timer/concurrency adapters if needed

### Provider Logic vs Transport

This is the most important WASM boundary.

Provider modules should describe:

1. request shape
2. upstream path selection
3. provider-specific normalization
4. stream parsing rules

Transport adapters should describe:

1. how bytes are fetched
2. how streaming frames are obtained
3. how retries and timing are implemented
4. how browser APIs are bridged into Rust-friendly abstractions

### Browser Inference vs Browser Model Discovery

These are related but not identical flows.

Inference path responsibilities:

1. execute canonical Responses request through `xrouter-core`
2. pass BYOK bearer token per request
3. stream normalized response events to the UI

Model discovery responsibilities:

1. call provider model-list endpoint
2. normalize provider model entries into browser-usable descriptors
3. filter unsupported or unusable entries where needed

For the first demo, it is acceptable for model discovery to use a smaller dedicated adapter than
the full inference path, as long as ownership stays explicit and provider quirks do not leak into
the UI.

### Native App vs WASM Consumer

`xrouter-app` remains the server composition root.

The browser/WASM track must produce a separate consumer-facing layer, not attempt to compile the
server crate to WASM.

This is the core product rule:

1. server product may depend on shared crates plus `xrouter-app`
2. browser product may depend on shared crates plus a browser composition root
3. browser product must have zero dependency on server-only crates

## Demo-Oriented Design Decisions

### BYOK First

The browser demo is bring-your-own-key first, not server-secret first.

This means:

1. browser-facing APIs must accept per-request auth material
2. no server-owned config bootstrap should be required for the demo path
3. logs, errors, and debug output must never expose raw API keys

### Direct Provider First

The first implementation should use a direct provider path, not the full router-of-routers story.

Recommended first provider:

1. `deepseek`

Why:

1. there is already a focused provider client
2. the path is OpenAI-compatible enough for reuse
3. model selection and streaming are simpler than more specialized providers

### UI Should Stay Thin

The browser UI should not own:

1. provider payload construction
2. SSE frame parsing
3. canonical event semantics

The UI should own:

1. API key input
2. provider/model selection
3. prompt submission
4. live rendering of normalized stream events

### Portability Is Achieved by Extraction

If logic currently lives in `xrouter-app` but is needed by both products, the fix is:

1. extract it into a shared portable crate or module
2. make both products depend on that shared code

The fix is not:

1. making the browser crate depend directly on `xrouter-app`

## Architecture Invariants

1. `xrouter-app` stays native-only.
2. Responses remains the canonical internal contract.
3. Chat Completions remains an adapter, not a second core flow.
4. browser work must reuse stable seams, not reintroduce mixed provider/transport logic.
5. no browser-specific `cfg` spray across unrelated crates if a dedicated adapter crate can absorb
   the difference.
6. model discovery ownership must become explicit; it cannot stay hidden inside server startup if
   the browser is the caller.
7. the first browser demo must prove real streaming, not buffered fake streaming after completion.
8. the final architecture must support two products built from one portable core.
9. browser build must have zero dependency on server-only crates.

## Known Gaps

These are the current architectural gaps between the codebase and the demo target:

1. there is no browser-safe transport implementation yet
2. there is no dedicated browser crate/facade yet
3. model catalog loading is currently assembled inside `xrouter-app`
4. there is no documented browser-safe observability strategy yet
5. portability of current dependencies for `xrouter-core` and provider reuse is not yet proven
6. the extraction boundary for browser-needed logic still trapped in `xrouter-app` is not yet
   defined

## Near-Term Direction

The first real implementation steps should be:

1. compile audit for `xrouter-contracts`, `xrouter-core`, and the intended reusable provider pieces
   on `wasm32-unknown-unknown`
2. identify the minimal browser transport trait surface for streamed inference
3. split or wrap model discovery so the browser can fetch provider models without reusing
   `xrouter-app`
4. build one end-to-end provider path, likely `deepseek`

Do not start by trying to compile the whole workspace to WASM.
