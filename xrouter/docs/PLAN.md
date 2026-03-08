# xrouter Refactor Plan for Portability and Modular Architecture

This document describes the implementation plan for preparing `xrouter` for future portability
features such as browser/WASM reuse, while delivering immediate architectural improvements for the
native server.

The plan assumes that the architecture in `docs/ARCHITECTURE.md` is the target state.

The key idea is simple:

- do not optimize for WASM directly in the first phase;
- first make the native server architecture explicit and modular;
- then extract portability boundaries from stable seams.

## Why This Work Matters

This refactor is worth doing even if no WASM target ships immediately.

It improves:

- maintainability of `xrouter-app`;
- isolation of provider-specific complexity;
- testability of orchestration and streaming behavior;
- ability to add new clients/transports;
- confidence when changing provider integrations;
- readiness for browser-hosted or embedded consumers.

## Current Problems

The current codebase has the following architectural pressure points:

1. `xrouter-app` mixes too many responsibilities.
   - route definitions
   - OpenAPI setup
   - provider wiring
   - model registry loading
   - HTTP-specific glue
   - transport/security details

2. `xrouter-core` contains runtime-specific assumptions.
   - `tokio::mpsc`
   - `tokio::spawn`
   - `tokio`-bound tests
   - direct request ID generation in orchestration flow

3. `xrouter-clients-openai` mixes provider behavior and native transport.
   - payload shaping
   - SSE parsing
   - retries/backoff
   - `reqwest`
   - semaphores
   - tracing/otel hot-path coupling

4. Model catalog loading is app-startup logic, not app-state responsibility, but it is currently
   entangled with state construction.

5. Portability blockers are not isolated.
   - changing one area risks regressions in unrelated server code
   - future WASM/browser work would likely spread `cfg(target_arch = "wasm32")` through multiple
     crates if we do not refactor first

## Target Outcomes

At the end of this plan, we want:

1. a clean composition root in `xrouter-app`;
2. a transport-agnostic orchestration core;
3. provider logic that can be reused independently from native HTTP clients;
4. a stable boundary for future `xrouter-browser` or `xrouter-wasm`;
5. tests that prove behavior without depending on real network access.

## Delivery Strategy

The refactor should be delivered as a sequence of small-to-medium PRs.

Do not attempt a single workspace-wide rewrite.

Guiding rules:

1. preserve behavior in each PR;
2. keep each PR narrowly scoped;
3. land structural moves before deeper abstraction changes;
4. update docs alongside code changes;
5. keep tests green after each step.

## Phase 0: Baseline and Guardrails

Objective:

- establish current-state checks and document active blockers.

Work:

1. Confirm workspace command entrypoints.
   - add or normalize `justfile` if missing
   - document fmt/clippy/test commands

2. Capture current architectural pain points.
   - document large modules
   - document runtime-coupled APIs
   - document provider client mixing of concerns

3. Add explicit portability audit checklist.
   - which crates must compile for `wasm32`
   - which dependencies are server-only

Deliverables:

- documented command baseline
- documented blocker list
- updated docs references

Exit criteria:

- team agrees on target layering and phase order

## Phase 1: Make `xrouter-app` a Real Composition Root

Status:

- completed
- completed:
  - provider/client wiring moved out of `AppState` into `startup/provider_factory.rs`
  - model loading moved out of `AppState` into `startup/model_catalog.rs`
  - `AppState::from_config` reduced to startup orchestration over dedicated modules
  - OpenAPI/router assembly moved into `http/docs.rs`
  - HTTP handlers split into `http/routes/basic.rs` and `http/routes/inference.rs`
  - `lib.rs` reduced to crate root and tests instead of holding startup and route glue
  - validation run completed:
    - `cargo fmt --all`
    - `cargo test -p xrouter-app`
    - `cargo clippy -p xrouter-app --all-targets -- -D warnings`

Deferred follow-up:

- `AppBuilder`/`StartupBuilder` is considered necessary because future composition paths will
  diverge at least between native app startup and WASM/browser startup
- this builder layer is intentionally deferred beyond Phase 1
- rationale:
  - Phase 1 focused on behavior-preserving modularization
  - builder introduction is more valuable once multiple composition paths are implemented or
    actively being introduced

Objective:

- refactor `xrouter-app` so it clearly owns startup and HTTP adaptation only.

Why first:

- this is the safest high-value refactor;
- it improves readability immediately;
- it creates stable seams for the later core/provider work.

Work:

1. Split `lib.rs` into focused modules.
   - `startup/`
   - `http/routes/`
   - `http/docs.rs`
   - `state.rs`

2. Move provider/client wiring out of app state.
   - introduce `ProviderFactory`
   - defer `AppBuilder` or `StartupBuilder` to the next stage where multiple composition paths are
     introduced explicitly

3. Move model loading into a dedicated service/module.
   - remote fetch
   - fallback policy
   - filtering/merging

4. Keep `AppState` as assembled state only.
   - config-derived flags
   - engines
   - model catalog

5. Keep route handlers thin.
   - request extraction
   - engine invocation
   - HTTP mapping only

Deliverables:

- smaller `xrouter-app` modules
- explicit startup/composition root
- no behavior change

Tests:

- existing route tests still pass
- new tests for app builder/provider factory/model catalog behavior

Exit criteria:

- app wiring no longer lives inside one large state constructor
- `lib.rs` becomes mostly module exports and top-level router assembly
- builder-based composition is not required to close Phase 1

## Phase 2: Extract Model Catalog Boundary

Objective:

- separate model discovery and catalog policy from app startup plumbing.

Work:

1. Introduce model catalog source abstractions.
   - static defaults
   - provider registry fetchers
   - fallback registry adapters

2. Introduce a catalog service.
   - merge sources
   - filter by enabled providers
   - preserve provider-specific supported-model rules

3. Move provider-specific model registry code out of route/app glue.

Deliverables:

- dedicated model catalog service
- simpler app state construction

Tests:

- happy path: remote registry results included
- failure path: fallback catalog used
- provider filtering behaves deterministically

Exit criteria:

- app startup asks for a finished model catalog instead of building it inline

## Phase 3: Prepare `xrouter-core` for Portability

Objective:

- remove native runtime assumptions from the core public boundary.

This is the first major architectural step.

Work:

1. Audit current public API surface in `xrouter-core`.
   - identify `tokio` types in signatures
   - identify stream-plumbing responsibilities mixed with orchestration

2. Introduce portable abstractions.
   - event sink
   - runtime stream output boundary
   - request ID generation abstraction if needed

3. Refactor engine execution to depend on abstractions rather than Tokio channels.

4. Keep formal lifecycle semantics unchanged.
   - `ingest -> tokenize -> generate`
   - disconnect semantics preserved
   - terminal success remains explicit

5. Reduce direct observability coupling in hot-path orchestration.
   - keep spans/events where useful
   - avoid forcing OTel-specific extensions into the essential flow

Deliverables:

- transport-agnostic core interfaces
- clearer stage/pipeline modules

Tests:

- scenario tests through public engine APIs
- streaming success and failure paths
- disconnect behavior tests

Exit criteria:

- `xrouter-core` no longer requires Tokio channel types in core-facing APIs
- core behavior remains unchanged from an external perspective

## Phase 4: Split Provider Logic from Native Transport

Objective:

- refactor `xrouter-clients-openai` into explicit provider behavior and native transport layers.

This is likely the largest implementation phase.

Work:

1. Inventory provider responsibilities currently mixed in the crate.
   - payload shaping
   - event parsing
   - retries
   - concurrency limiting
   - header injection
   - HTTP transport

2. Move transport-neutral logic into dedicated modules.
   - request builders
   - normalization helpers
   - response mapping
   - SSE/event parsing

3. Isolate native transport adapter.
   - `reqwest` execution
   - response body acquisition
   - native retry sleep
   - native semaphore use

4. Standardize provider layout.
   - one directory per provider or equivalent modular structure

5. Keep provider-specific quirks local.
   - DeepSeek normalization
   - GigaChat OAuth flow
   - Yandex/Xrouter/OpenRouter differences

Deliverables:

- explicit separation between provider logic and native transport
- smaller provider modules

Tests:

- unit tests for normalization/parsing
- transport adapter tests with mocked responses
- regression tests for existing providers

Exit criteria:

- provider behavior can be compiled/tested without depending directly on `reqwest::Client`
- native transport remains a replaceable adapter

## Phase 5: Introduce Shared Runtime/Transport Abstractions

Objective:

- create a stable portability seam for native and browser implementations.

This phase may introduce a new crate if needed.

Work:

1. Define transport-facing contracts.
   - HTTP request/response model
   - SSE stream abstraction
   - transport errors

2. Define runtime support contracts if still needed in shared code.
   - sleep/backoff
   - concurrency limiting
   - event sink

3. Decide whether these abstractions live in:
   - `xrouter-core`, or
   - a new crate such as `xrouter-runtime`

Decision rule:

- if both core and provider logic need the abstractions, prefer a dedicated shared crate.

Deliverables:

- stable portability boundary

Tests:

- fake transport implementations for deterministic tests
- transport contract coverage for success/failure/streaming

Exit criteria:

- the boundary needed by browser/native adapters is explicit and stable

## Phase 6: Portability Audit and WASM Readiness

Objective:

- confirm that the shared layers are ready for future browser work.

Work:

1. Add compile checks for portable crates.
   - `xrouter-contracts`
   - `xrouter-core`
   - any shared provider-logic crate/module

2. Audit dependency features.
   - `uuid`
   - `utoipa`
   - tracing-related dependencies
   - any hidden native-only dependency usage

3. Fix remaining wasm portability blockers.
   - feature flags
   - conditional implementations
   - dependency replacement where necessary

Deliverables:

- portable crates compile for `wasm32-unknown-unknown`
- blocker list reduced to browser-specific adapter work only

Tests:

- target-specific compile checks

Exit criteria:

- future `xrouter-browser` work no longer requires deep server-side refactors

## Phase 7: Optional Browser/WASM Adapter Crate

Objective:

- add a dedicated browser-facing crate only after boundaries are ready.

Work:

1. Create `xrouter-browser` or `xrouter-wasm`.
2. Implement browser-safe transport using `fetch`.
3. Adapt browser stream handling to shared SSE/event parser logic.
4. Support at least one provider end-to-end.
5. Later add `xrouter` as a provider endpoint if needed.

Deliverables:

- new adapter crate with browser-safe transport

Exit criteria:

- browser consumer can execute a streamed turn through shared logic

## Cross-Cutting Workstreams

These run through multiple phases.

### Documentation

Update in the same PR when boundaries change:

- `docs/ARCHITECTURE.md`
- `docs/testing.md`
- `docs/configuration.md` if config surfaces move
- formal docs if lifecycle semantics ever change

### Observability

Keep:

- spans for provider request lifecycle
- spans for stream lifecycle
- structured error attribution

But:

- avoid making OTel-specific APIs mandatory for portability-sensitive logic

### Testing

Each affected area needs both happy-path and failure-path coverage.

Required focus:

- streaming
- disconnect behavior
- model catalog fallback
- provider normalization
- route adapter behavior

### Security

Preserve:

- no secrets in logs
- no real external APIs in tests
- narrow handling of insecure TLS exceptions

## Proposed PR Slices

Recommended order:

1. Docs and target structure agreement
2. `xrouter-app` module split without behavior changes
3. provider factory extraction
4. model catalog service extraction
5. `xrouter-core` stream/runtime boundary refactor
6. provider logic vs transport split
7. shared runtime abstractions
8. wasm compile audit
9. browser adapter crate

Each PR should stay small enough to review coherently.

## Risks

### Risk 1: Over-refactoring too early

Mitigation:

- keep each phase behavior-preserving;
- avoid inventing abstractions before they are needed by at least one real caller.

### Risk 2: Excessive `cfg(target_arch = "wasm32")`

Mitigation:

- prefer new adapter modules/crates over scattering conditional compilation through core logic.

### Risk 3: Regressions in streaming behavior

Mitigation:

- add deterministic streaming tests before or during refactor;
- keep provider parser tests close to implementation.

### Risk 4: Losing architecture clarity through partial moves

Mitigation:

- only land module moves that improve dependency direction;
- update docs immediately when boundaries change.

### Risk 5: Hidden provider-specific coupling

Mitigation:

- split provider code explicitly;
- review each provider for custom normalization and auth flows before generalizing abstractions.

## Definition of Done

The refactor plan is complete when:

1. `xrouter-app` is a thin composition root.
2. model catalog loading is a dedicated service/module.
3. `xrouter-core` exposes transport-agnostic orchestration boundaries.
4. provider logic is separable from native transport code.
5. portable crates can pass target compile checks for `wasm32-unknown-unknown`.
6. future browser work can be added as an adapter crate rather than a workspace-wide rewrite.

## Immediate Next Step

Start with Phase 1.

It gives the highest signal-to-risk ratio:

- visible architecture improvement;
- no semantic change required;
- creates clean seams for every later phase.
