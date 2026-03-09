# Architecture

This document describes the high-level architecture of XRouter.

If you need process rules, testing discipline, or delivery constraints, see `AGENTS.md`.
This file is the codemap: it should help answer "where is the thing that does X?" and "what is
this part of the codebase supposed to do?".

## Bird's Eye View

XRouter is a Rust router around LLM providers.

At the highest level:

1. `xrouter-app` accepts HTTP requests in OpenAI-compatible formats.
2. `xrouter-core` executes the canonical request lifecycle.
3. provider client crates talk to upstreams and normalize provider-specific behavior.
4. responses are returned either as final JSON payloads or as SSE streams.

The canonical external contract is the OpenAI Responses API.
Chat Completions support exists as an adapter over the Responses flow.

The active lifecycle is:

`ingest -> tokenize -> generate(stream) -> done|failed`

This lifecycle is specified formally in:

- `formal/xrouter.tla`
- `formal/property-map.md`
- `formal/trace-schema.md`

## Code Map

This section lists the important crates, modules, and types.

### `xrouter/crates/xrouter-app`

This is the application entrypoint and composition root.

If you are looking for:

- startup wiring: `AppBuilder`, `startup/app_builder.rs`
- provider/model startup assembly: `startup/`
- HTTP route registration: `http/docs.rs`
- request handlers: `http/routes/`
- auth/header handling: `http/auth.rs`
- HTTP error mapping: `http/errors.rs`

`xrouter-app` should know about:

1. configuration
2. dependency wiring
3. HTTP and SSE adaptation
4. runtime-owned stream spawning

**Architecture Invariant:** `xrouter-app` is allowed to know about Axum and Tokio runtime details.
It is not allowed to own provider-specific request normalization or core lifecycle semantics.

### `xrouter/crates/xrouter-core`

This crate is the orchestration layer.

Important types:

- `ExecutionEngine`
- `ExecutionContext`
- `ProviderClient`
- `ResponseEventSink`
- `ProviderOutcome`

If you are looking for:

- canonical request execution: `ExecutionEngine`
- lifecycle stages and disconnect behavior: `run_stage`, `execute_internal`
- provider abstraction: `ProviderClient`
- stream boundary: `ResponseEventSink`

**Architecture Invariant:** `xrouter-core` owns lifecycle semantics but does not own HTTP concerns
or runtime-specific public API types.

**Architecture Invariant:** streaming is a first-class boundary in core, not an app-only wrapper
around non-stream execution.

### `xrouter/crates/xrouter-contracts`

This crate contains shared DTOs and typed contract structures.

If you are looking for:

- Responses API types
- Chat Completions request/response types
- stream event payloads
- shared enums like `StageName`

look here first.

**Architecture Invariant:** this crate is a shared contract boundary, not a place for provider
behavior or orchestration logic.

### `xrouter/crates/xrouter-clients-openai`

This crate contains reusable provider client logic for OpenAI-compatible upstreams and provider
variants built on top of that shape.

Important modules:

- `clients/` for provider-specific modules
- `protocol.rs` for request shaping
- `transport.rs` for native HTTP/runtime execution
- `parser.rs` for response parsing and stream normalization

If you are looking for:

- how requests are shaped for upstream chat/responses payloads: `protocol.rs`
- how HTTP calls, retries, and stream transport work: `transport.rs`
- how SSE chunks and provider payloads are parsed: `parser.rs`
- where a specific provider quirk lives: `clients/<provider>.rs`

**Architecture Invariant:** provider quirks should stay local to provider modules.

**Architecture Invariant:** shared transport behavior should stay outside provider modules.

**Architecture Invariant:** crate root should stay thin; it is not a dumping ground for mixed
transport, parsing, and provider logic.

### `xrouter/crates/xrouter-observability`

This crate contains observability setup and adapters.

If you are looking for:

- exporter configuration
- OTLP/stdout setup
- observability preflight checks

start here.

## Boundaries

These boundaries are important and should stay visible in code.

### App -> Core

`xrouter-app` translates HTTP requests into canonical core requests and adapts core results back
into JSON or SSE.

The app owns:

1. request parsing
2. auth extraction
3. route mapping
4. runtime stream plumbing

Core owns:

1. lifecycle progression
2. provider invocation
3. canonical output semantics

### Core -> Provider Clients

`xrouter-core` depends on `ProviderClient`, not on concrete HTTP clients.

Provider crates own:

1. upstream payload shaping
2. provider-specific normalization
3. transport execution
4. stream parsing

Core should not branch on provider-specific HTTP behavior.

### Responses -> Chat Completions

Responses is the canonical flow.
Chat Completions is an adapter layer over the same core execution semantics.

This means:

1. new behavior should be designed against Responses-first canonical models
2. chat-specific mapping should stay in adapter code, not leak into core semantics

## Architecture Invariants

These are the most important invariants to preserve.

1. `xrouter-app` is the only layer that should know about HTTP server details.
2. `xrouter-core` is the layer that defines request lifecycle semantics.
3. disconnect in `ingest|tokenize` fails fast; disconnect in `generate` does not cancel in-flight
   generation lifecycle.
4. Responses is the canonical contract; Chat Completions is an adapter.
5. provider-specific behavior should be localized to provider/client code, not route handlers.
6. streaming should remain a first-class execution path, not a bolted-on post-processing layer.
7. crate roots should stay thin; logic should live in named modules near the layer that owns it.

## Cross-Cutting Concerns

### Testing

The primary testing style in this repository is deterministic scenario testing through public
boundaries.

In practice this means:

1. route-level behavior is tested in `xrouter-app`
2. lifecycle and streaming semantics are tested in `xrouter-core`
3. normalization and outbound request behavior are tested in provider/client crates

### Observability

Provider requests, stream lifecycle, and error paths are expected to remain observable.

If you change a boundary and observability disappears from that boundary, treat that as an
architectural regression, not just a logging change.

### Formal Model

If you change lifecycle semantics, update the formal artifacts in `formal/`.

If you only refactor code while preserving lifecycle semantics, the formal model should stay
unchanged.

## Near-Term Direction

The large structural refactor is complete.

Near-term work should mostly be:

1. strengthening behavior coverage
2. adding capabilities without weakening existing boundaries
3. keeping composition explicit
4. keeping provider logic separate from transport and route adaptation

WASM-specific architecture is tracked separately under `xrouter/docs/wasm/`.
