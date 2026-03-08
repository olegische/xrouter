# XRouter Refactor Plan

This document is the implementation plan for the Rust architecture defined in `ARCHITECTURE.md`.

It is intentionally independent from WASM. WASM-specific follow-up lives under `xrouter/docs/wasm/`.

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
- WASM-specific architecture moved out of the main plan into `xrouter/docs/wasm/`

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

- pending

Objective:

- introduce an explicit builder layer for application assembly

Work:

1. Add `AppBuilder` or `StartupBuilder`.
2. Make startup dependencies explicit:
   - config
   - provider factory
   - model catalog service
   - router assembly
3. Keep alternate composition paths out of `AppState`.

Exit criteria:

- application assembly is a dedicated layer, not a side effect of state construction

## Phase 4: Clean `xrouter-core` Public Boundaries

Status:

- pending

Objective:

- remove runtime-specific assumptions from core-facing APIs

Work:

1. audit `tokio` types in public signatures
2. isolate stream/output boundaries behind typed abstractions
3. keep lifecycle semantics aligned with the formal model
4. reduce direct observability coupling in the hot path

Exit criteria:

- `xrouter-core` owns orchestration and lifecycle semantics, not runtime plumbing

## Phase 5: Split Provider Logic from Native Transport

Status:

- pending

Objective:

- separate provider behavior from HTTP/runtime adapters

Work:

1. isolate request shaping and response parsing
2. isolate retries, concurrency limits, and HTTP execution
3. keep provider quirks local to provider modules
4. remove route-layer knowledge of concrete providers

Exit criteria:

- provider logic is reusable and testable without native transport clients

## Phase 6: Testing and Contract Hardening

Status:

- pending

Objective:

- strengthen coverage around the stabilized seams

Work:

1. add deterministic scenario tests for streaming and failures
2. verify disconnect semantics through public APIs
3. keep outbound request assertions close to provider/client tests
4. update formal artifacts if lifecycle semantics ever change

Exit criteria:

- major flows have happy-path and failure-path coverage at the public API boundary

## Separate Track: WASM

WASM-specific architecture and delivery planning are tracked separately:

- `xrouter/docs/wasm/ARCHITECTURE.md`
- `xrouter/docs/wasm/PLAN.md`
- `xrouter/docs/wasm-build-plan.md`
