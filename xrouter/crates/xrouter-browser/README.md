# WASM Usage

This document describes the current consumer-facing browser/WASM contract for XRouter.

Scope:

1. this is a library/runtime contract
2. this is not a UI contract
3. consumers are expected to provide their own host or browser UI

## Primary Crate

Use:

- `xrouter/crates/xrouter-browser`

This crate is the browser/WASM composition root for XRouter.

It provides:

1. browser model discovery
2. browser-safe provider runtime
3. streamed inference
4. request-scoped cancellation
5. wasm exports for JS consumers

## Current Supported Provider

Current first supported browser provider:

1. `deepseek`

This is the only provider that should be treated as part of the current stable acceptance slice.

## Rust API

Main Rust-facing types:

1. `BrowserModelDiscoveryClient`
2. `BrowserProviderRuntime`
3. `BrowserInferenceClient`
4. `BrowserProvider`
5. `BrowserError`

Current browser inference contract:

1. consumer supplies `request_id`
2. consumer supplies provider, base URL, API key, model, and input
3. stream events are emitted through `ResponseEventSink`
4. consumer may call `cancel(request_id)` to abort the active browser request

## WASM API

Current wasm-facing export:

1. `WasmBrowserClient`

Current methods:

1. `fetchModelIds()`
2. `runTextStream(requestId, model, input, onEvent)`
3. `runDemoPromptStream(requestId, model, onEvent)`
4. `cancel(requestId)`

Current event callback payloads are serialized `ResponseEvent` values from `xrouter-contracts`.

## Cancellation Semantics

Cancellation is a first-class browser capability.

Current behavior:

1. active requests are keyed by `request_id`
2. browser transport uses `AbortController`
3. `cancel(request_id)` is idempotent
4. cancellation stops further stream deltas for that request
5. cancellation is surfaced as a request-level cancellation outcome/error

What cancellation is not:

1. it is not a UI-only unsubscribe
2. it is not just dropping callbacks while the provider request keeps running

## Out of Scope

These items are intentionally outside the XRouter wasm library contract:

1. any specific host protocol for downstream applications
2. any specific `codex-rs` adapter shapes
3. any bundled browser UI requirement
4. multi-provider parity beyond the first accepted browser-safe provider path

## Packaging

Current recommended packaging strategy:

1. consume `xrouter-browser` as a source crate
2. build it with `wasm-pack`
3. consume the generated wasm/js package from the host application

This keeps the contract neutral for downstream browser hosts.
