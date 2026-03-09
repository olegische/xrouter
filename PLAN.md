# XRouter Refactor Plan

This document is the implementation plan for the Rust architecture defined in `ARCHITECTURE.md`.

It is intentionally independent from WASM. WASM-specific follow-up lives with the browser crate
under `xrouter/crates/xrouter-browser/`.

## Goals

1. Keep `xrouter-app` as an explicit composition root.
2. Move orchestration, provider logic, and transport concerns into clear boundaries.
3. Preserve the formal lifecycle contract:
   `ingest -> tokenize -> generate(stream) -> done|failed`.
4. Improve testability without changing external behavior.
5. Prepare the codebase for future consumers by stabilizing seams first.

## Delivery Rules

1. Land changes in small, reviewable PR slices.
2. Keep behavior stable unless a phase explicitly changes contracts.
3. Update docs together with boundary changes.
4. Run fmt, tests, and clippy for impacted Rust crates after code changes.

## Phase 0: Baseline

Status:

- completed

Outcome:

- target architecture documented in `ARCHITECTURE.md`
- migration guardrails captured in `AGENTS.md`
- WASM-specific architecture moved out of the main plan into `xrouter/crates/xrouter-browser/`

## Phase 1: Make `xrouter-app` a Real Composition Root

Status:

- completed

Completed work:

- provider/client wiring moved out of `AppState`
- model loading moved into `startup/model_catalog.rs`
- `AppState::from_config` reduced to startup orchestration
- OpenAPI/router assembly moved into `http/docs.rs`
- HTTP handlers split into focused route modules
- `lib.rs` reduced to crate root plus tests

Exit result:

- `xrouter-app` now reads as startup plus HTTP adaptation, not as a mixed architecture bucket

Deferred follow-up:

- introduce `AppBuilder` or `StartupBuilder` when we formalize multiple composition paths and entrypoints

## Phase 2: Extract Model Catalog Boundary

Status:

- completed

Completed work:

- introduced `ModelCatalogService`
- introduced `ModelCatalogContext`
- introduced `ModelCatalogSource`
- separated remote registry/fetch code into `startup/model_catalog_remote.rs`
- separated provider-specific catalog sources into `startup/model_catalog_sources.rs`
- reduced `startup/model_catalog.rs` to service orchestration plus public startup entrypoint

Exit criteria:

- app startup requests a catalog from a service boundary
- provider-specific registry logic is no longer mixed with startup orchestration

## Phase 3: Formalize Application Composition

Status:

- completed

Objective:

- introduce an explicit builder layer for application assembly

Completed work:

- introduced `AppBuilder` as the application assembly layer
- made startup dependencies explicit inside the builder:
  - config
  - provider factory
  - model catalog service
  - router assembly
- switched `main` to assemble the app through `AppBuilder`
- reduced `AppState::from_config` to a compatibility wrapper over the builder
- kept routing behavior unchanged while moving composition out of `AppState`

Exit criteria:

- application assembly is a dedicated layer, not a side effect of state construction

## Phase 4: Clean `xrouter-core` Public Boundaries

Status:

- completed

Objective:

- remove runtime-specific assumptions from core-facing APIs

Completed work:

- audited the main runtime-coupled public boundaries in `xrouter-core`
- replaced the provider stream request dependency on `tokio::mpsc::Sender` with a core-owned
  `ResponseEventSink` abstraction
- replaced the public `ReceiverStream` return type in `ExecutionEngine::execute_stream*` with a
  core-owned stream alias
- moved runtime-owned stream spawning and channel assembly out of `xrouter-core` and into
  `xrouter-app`
- replaced `ExecutionEngine::execute_stream*` with `execute_stream_to_sink(...)` so core owns
  orchestration while the app owns runtime adapters
- updated `xrouter-clients-openai` to depend on the core sink abstraction instead of Tokio sender
  types
- removed direct `opentelemetry::Status` and `tracing_opentelemetry::OpenTelemetrySpanExt`
  coupling from `xrouter-core` while preserving tracing spans and structured events
- reduced `xrouter-core` production dependencies to architecture-relevant crates only:
  `async-trait`, `serde_json`, `thiserror`, `tracing`, `uuid`, and `xrouter-contracts`
- added explicit success/failure tests for the new sink-based streaming boundary

Exit criteria:

- `xrouter-core` owns orchestration and lifecycle semantics, not runtime plumbing

## Phase 5: Split Provider Logic from Native Transport

Status:

- completed

Objective:

- separate provider behavior from HTTP/runtime adapters

Completed work:

- extracted native HTTP runtime, retry/backoff, trace-header injection, and stream transport
  execution into `xrouter-clients-openai/src/transport.rs`
- extracted request-shaping helpers into `xrouter-clients-openai/src/protocol.rs`
- extracted response parsing and stream normalization into
  `xrouter-clients-openai/src/parser.rs`
- kept provider-specific clients pointed at the new modules without changing external behavior
- preserved existing app integration and regression coverage while reducing `lib.rs` mixing of
  transport and provider concerns
- moved transport/protocol/parser tests next to their modules so `lib.rs` is no longer the default
  home for cross-layer test logic
- switched provider client modules from crate-root convenience imports to explicit
  `protocol`/`transport` module dependencies
- moved provider-specific invariants out of the crate root and into provider modules so
  `xrouter-clients-openai/src/lib.rs` is now a thin module root plus public exports

Exit criteria:

- provider logic is reusable and testable without native transport clients

## Phase 6: Testing and Contract Hardening

Status:

- completed

Objective:

- strengthen coverage around the stabilized seams

Completed work:

1. added deterministic route-level failure coverage in `xrouter-app` for:
   - non-stream provider failure on Responses API
   - Responses SSE failure without false completion
   - Chat Completions SSE failure with terminal done marker
2. added explicit `xrouter-core` public API tests for disconnect semantics on the sink-based
   streaming boundary:
   - ingest disconnect emits stream error without completion
   - generate disconnect preserves successful completion
3. hardened the mock provider path so public API failure scenarios are exercised without external
   network or provider dependencies
4. kept formal artifacts unchanged because lifecycle semantics did not change; tests now better pin
   the existing contract

Exit criteria:

- major flows have happy-path and failure-path coverage at the public API boundary

## Separate Track: WASM

WASM-specific architecture and delivery planning are tracked separately:

- `xrouter/crates/xrouter-browser/ARCHITECTURE.md`
- `xrouter/crates/xrouter-browser/PLAN.md`
