# WASM Architecture

This document describes the high-level architecture of browser/WASM support in XRouter.

It assumes the main Rust refactor is already complete:

1. `xrouter-app` is the native composition root
2. `xrouter-core` exposes a sink-based streaming boundary
3. provider request shaping, native transport, and stream parsing are separated in
   `xrouter-clients-openai`

The goal is not to make the server "sort of compile to WASM".
The goal is to produce a second XRouter product that runs natively in the browser with
bring-your-own-key (BYOK), while sharing as much Rust logic as possible with the server product.

Important scope boundary:

1. `xrouter` owns the portable browser/WASM router library
2. `xrouter` does not own a UI as part of its required deliverable
3. any browser UI is only a consumer, smoke harness, or example app unless explicitly promoted
   into a separate product

Target product shape:

1. a server XRouter product
2. a browser/WASM XRouter product
3. one shared portable Rust core used by both

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
consumer UI or host runtime
  -> wasm crate facade
  -> browser-safe transport + stream reader
  -> shared provider protocol/parser logic
  -> xrouter-core execution engine
  -> xrouter-contracts DTOs
```

For the first demo, there is also a second browser path:

```text
consumer UI or host runtime
  -> model discovery adapter
  -> provider /models endpoint
```

The browser product is therefore a separate composition root.
It should reuse portable crates from the server product, but it must not depend on
`xrouter-app` or any native-only bootstrap layer.

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

### `xrouter/crates/xrouter-browser`

This crate is the production location for browser/WASM-specific Rust code.

It should contain:

1. browser-safe execution logic
2. browser runtime adapters
3. wasm exports
4. browser model discovery
5. browser-specific documentation for architecture, plan, and usage

The browser demo frontend should be a separate app, not part of the Rust crate itself.

Preferred split:

1. Rust/WASM browser crate under `xrouter/crates/`
2. separate Vite/Svelte/TypeScript demo app outside the Rust crate

This means:

1. the Rust crate owns browser-safe execution logic, wasm exports, and runtime adapters
2. the demo app, if present, owns UI, frontend tooling, and browser-dev workflow
3. `xrouter-browser` remains reusable even if the demo app is deleted or replaced

## Boundaries

These boundaries are the important browser-side architecture seams.

### Host -> Browser Crate

The host application or browser UI owns:

1. provider selection
2. API key entry and local storage policy
3. request initiation and cancellation intent
4. rendering streamed output

`xrouter-browser` owns:

1. browser-safe request execution
2. provider model discovery
3. provider request shaping reuse
4. stream parsing and event delivery
5. cancellation as a transport capability

This boundary must stay host-neutral.
`xrouter-browser` should not know about downstream application protocols.

### Browser Crate -> Shared Portable Crates

`xrouter-browser` should depend on portable boundaries only:

1. `xrouter-contracts` for canonical DTOs
2. `xrouter-core` for orchestration semantics
3. `xrouter-clients-openai` for request shaping, parsing, and provider-local behavior

Browser-specific transport and wasm export code should stay local to `xrouter-browser`.

### Browser Crate -> Native Server Crate

There should be no direct dependency from `xrouter-browser` to `xrouter-app`.

If browser work needs logic that currently lives in `xrouter-app`, that logic should move into
a shared portable crate or module. The browser crate must not import the native composition root.

## Architecture Invariants

These are the most important invariants to preserve.

1. `xrouter-browser` is a browser/WASM composition root, not a UI app.
2. browser execution must have zero dependency on `xrouter-app` and other native-only bootstrap
   layers.
3. provider-specific quirks should remain localized to provider/client code, not leak into host
   integrations.
4. browser transport should be replaceable without changing shared provider protocol or parser
   logic.
5. cancellation is a first-class transport capability, not a UI-only unsubscribe trick.
6. host-specific integration protocols are downstream concerns and should not shape
   `xrouter-browser`.

## Cross-Cutting Concerns

### Packaging

The preferred unit of reuse is the Rust crate `xrouter-browser`.

Downstream browser hosts may package it with `wasm-pack`, but packaging strategy should not change
the crate boundaries or leak host-specific assumptions into the crate.

### Demo UI

If a demo UI exists, it is only a smoke harness or consumer of `xrouter-browser`.

It should stay outside the Rust crate so the browser router library remains reusable and
host-neutral.
