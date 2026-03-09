# WASM Architecture

This document describes the target architecture for the WASM/browser track inside `xrouter/`.

It assumes the main Rust refactor is already complete:

1. `xrouter-app` is the native composition root
2. `xrouter-core` exposes a sink-based streaming boundary
3. provider request shaping, native transport, and stream parsing are separated in
   `xrouter-clients-openai`

The goal now is not to "prepare for WASM".
The goal is to build a browser-safe consumer layer on top of the stable seams that already exist.

## Bird's Eye View

The browser/WASM target should reuse:

1. canonical contracts from `xrouter-contracts`
2. orchestration semantics from `xrouter-core`
3. provider request shaping and response parsing from `xrouter-clients-openai`

The browser/WASM target should not reuse:

1. `xrouter-app`
2. Axum HTTP routes
3. Tokio runtime plumbing
4. native `reqwest` transport
5. server-owned observability bootstrap

At a high level, the target shape is:

```text
browser host
  -> browser-safe transport adapter
  -> portable provider logic
  -> portable orchestration
  -> canonical contracts
```

## Code Map

### `xrouter/crates/xrouter-contracts`

This is the primary shared contract boundary for a future WASM layer.

If you need:

1. Responses types
2. Chat Completions adapter-facing types
3. stream event types

start here.

### `xrouter/crates/xrouter-core`

This is the reusable orchestration boundary.

Relevant types:

1. `ExecutionEngine`
2. `ProviderClient`
3. `ProviderOutcome`
4. `ResponseEventSink`

What is already good for WASM:

1. public streaming boundary no longer exposes Tokio channel types
2. orchestration owns lifecycle semantics instead of runtime plumbing

What still needs care:

1. compile audit for `wasm32-unknown-unknown`
2. request-id generation and any remaining portability-sensitive dependencies

### `xrouter/crates/xrouter-clients-openai`

This crate already contains the split we needed:

1. `protocol.rs`
   request shaping
2. `parser.rs`
   response parsing and SSE/event normalization
3. `transport.rs`
   native HTTP/runtime implementation
4. `clients/`
   provider-local behavior

For WASM, the important point is:

- `protocol.rs` and `parser.rs` are the likely reuse surface
- `transport.rs` is the native-only implementation to replace with a browser transport

### `xrouter/crates/xrouter-app`

This crate is explicitly out of scope for WASM reuse.

It should remain native-only.

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

## Boundaries

### Core vs Browser Transport

`xrouter-core` should continue to own:

1. lifecycle semantics
2. provider orchestration
3. stream event semantics

The browser layer should own:

1. `fetch`-based HTTP execution
2. browser-safe SSE byte/event acquisition
3. browser-specific retry/timer/concurrency adapters if needed

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

### Native App vs WASM Consumer

`xrouter-app` remains the server composition root.

The WASM/browser track should produce a separate consumer-facing layer, not attempt to compile the
server crate to WASM.

## Architecture Invariants

1. `xrouter-app` stays native-only.
2. Responses remains the canonical internal contract.
3. Chat Completions remains an adapter, not a second core flow.
4. browser work must reuse stable seams, not reintroduce mixed provider/transport logic.
5. no browser-specific `cfg` spray across unrelated crates if a dedicated adapter crate can absorb
   the difference.

## Near-Term Direction

The first real implementation step should be:

1. compile audit for `xrouter-contracts` and `xrouter-core` on `wasm32-unknown-unknown`
2. identify the minimal browser transport trait surface
3. build one end-to-end provider path, likely `deepseek`

Do not start by trying to compile the whole workspace to WASM.
