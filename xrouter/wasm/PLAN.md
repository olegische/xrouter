# WASM Plan

This is the implementation plan for the browser/WASM track in `xrouter/`.

The preparatory Rust architecture refactor is already done in the main codebase.
This plan starts from that baseline and optimizes for the first real browser demo.

The intended outcome is two separate products:

1. server XRouter
2. browser/WASM XRouter

Both should be built from a shared portable Rust core.

## Demo Goal

Build a browser-safe vertical slice where:

1. the user pastes an API key
2. the user selects a provider
3. the browser fetches models for that provider
4. the user selects a model
5. the browser sends `Hello, what can you do?`
6. the provider response is displayed as a live stream

The first milestone is not "general WASM readiness".
The first milestone is this end-to-end browser demo.

The broader architectural target behind that demo is:

1. browser build has zero dependency on server-only crates
2. shared logic is reused through portable crates
3. browser-needed logic currently trapped in `xrouter-app` is extracted, not imported directly

## Current Starting Point

Already completed in the main codebase:

1. `xrouter-app` is a thin native composition root
2. `xrouter-core` owns orchestration and exposes a sink-based streaming boundary
3. `xrouter-clients-openai` is split into protocol, parser, and native transport layers
4. provider request execution already supports per-request bearer override
5. route, provider, and transport concerns are no longer mixed the way they were before

Main gaps still visible from the codebase:

1. browser-safe transport does not exist yet
2. browser model discovery does not exist yet
3. `xrouter-app` currently owns startup model catalog assembly
4. no dedicated browser crate exists yet
5. extraction targets inside `xrouter-app` are not yet explicitly mapped

## Phase 0: Track Setup

Status:

- completed

Completed:

1. WASM planning moved out of the main architecture/plan
2. a dedicated `xrouter/wasm/` track area exists for this work
3. main architecture is now stable enough to use as the baseline for browser work

## Phase 1: Portability Audit

Status:

- pending

Objective:

Confirm which crates and modules already compile for `wasm32-unknown-unknown` and capture exact
blockers.

Work:

1. check `xrouter-contracts`
2. check `xrouter-core`
3. inspect reusable parts of `xrouter-clients-openai`:
   - `protocol.rs`
   - `parser.rs`
   - selected provider modules
4. inspect dependency feature flags for portability-sensitive crates
5. identify browser-needed logic still trapped in `xrouter-app`
6. document blockers with exact crate/module ownership

Exit criteria:

1. a concrete blocker list exists
2. reusable browser-safe surface is identified precisely
3. browser work is no longer based on guesses
4. extraction candidates from `xrouter-app` are explicitly listed

## Phase 2: Browser Transport Boundary

Status:

- pending

Objective:

Define the minimal browser-safe runtime abstraction needed by shared provider logic for streamed
inference.

Work:

1. identify what `transport.rs` currently owns that must become browser-replaceable
2. define a small trait surface for:
   - JSON request/response execution
   - streaming byte or event acquisition
   - retry/timing hooks if needed
3. decide whether this abstraction lives:
   - inside `xrouter-clients-openai`
   - or in a small shared runtime/browser-facing crate
4. ensure per-request bearer injection works cleanly for browser BYOK

Exit criteria:

1. native transport and browser transport can target the same logical provider surface
2. abstraction is small and explicit
3. the abstraction supports true streaming, not fake post-buffered replay

## Phase 3: Browser Model Discovery Boundary

Status:

- pending

Objective:

Make provider model fetching available to the browser without depending on `xrouter-app` startup.

Why this is a separate phase:

1. the demo requires listing provider models before inference
2. current model loading is assembled in `xrouter-app/startup`
3. browser inference can be correct while model discovery is still incorrectly owned

Work:

1. identify reusable parts of current model discovery logic
2. decide the ownership model:
   - a small shared model-discovery module
   - or a browser-local adapter with explicit provider normalization
3. normalize fetched model data into `ModelDescriptor` or a browser-specific equivalent
4. support at least the first demo provider end to end
5. if logic is shared with the server product, move it into a portable crate instead of keeping it
   under `xrouter-app`

Exit criteria:

1. browser can fetch and display models for the first provider
2. provider quirks do not leak into the UI layer
3. model discovery no longer depends on `xrouter-app`
4. shared model-discovery logic, if any, is owned outside `xrouter-app`

## Phase 4: First Browser-Safe Provider Path

Status:

- pending

Objective:

Get one provider working end to end through the browser-safe path.

Recommended first provider:

- `deepseek`

Why:

1. already exercised heavily in smoke flows
2. simpler first target than router-of-routers behavior
3. existing client already supports per-request bearer override

Work:

1. reuse `protocol.rs` request shaping where possible
2. reuse `parser.rs` stream/response normalization where possible
3. implement browser transport adapter
4. wire a browser-safe sink for live stream events
5. validate one prompt round trip using `Hello, what can you do?`

Exit criteria:

1. one provider works from browser/WASM through shared logic
2. browser receives real stream events as they arrive
3. native behavior remains intact

## Phase 5: Dedicated WASM Crate

Status:

- pending

Objective:

Introduce a dedicated crate for the browser-facing layer after the vertical slice proves the
boundary.

Preferred location:

- `xrouter/crates/xrouter-browser`
  or
- `xrouter/crates/xrouter-wasm`

Why not `xrouter/wasm` as a crate:

1. `xrouter/wasm` is the track/document area
2. code crates should continue to live under `xrouter/crates/` with the rest of the workspace

Work:

1. add crate skeleton
2. expose minimal browser-facing API
3. keep crate scope narrow:
   - configure provider/auth
   - fetch models
   - run one streamed request

Exit criteria:

1. browser consumer does not depend on native app code
2. browser support has a clear workspace home
3. crate API is small enough that UI code does not rebuild transport/provider logic
4. browser crate depends only on portable/shared crates plus browser-specific adapters

## Phase 6: Demo UI Integration

Status:

- pending

Objective:

Wire the browser crate into a minimal demo UI.

Work:

1. API key input
2. provider selector
3. model list loading state
4. model selector
5. prompt trigger for `Hello, what can you do?`
6. streamed text rendering
7. visible error state for:
   - invalid key
   - model fetch failure
   - stream/inference failure

Exit criteria:

1. a user can complete the demo flow manually in the browser
2. stream output is visible incrementally
3. failure states are observable and debuggable

## Phase 7: Expand Provider Support

Status:

- pending

Objective:

Add more providers after the first browser-safe path is proven.

Order should be pragmatic:

1. simple OpenAI-compatible direct providers first
2. more specialized providers after the transport boundary is stable
3. `xrouter` endpoint support only after direct-provider paths are solid

Exit criteria:

1. adding providers reuses the same browser-safe architecture
2. provider work does not reintroduce transport/provider mixing

## Testing Expectations

For implementation phases, the minimum quality bar remains:

1. portability checks for affected crates
2. deterministic tests for request shaping and stream parsing reuse
3. happy-path and failure-path coverage for model discovery where applicable
4. happy-path and failure-path coverage for streamed inference where applicable

Browser-specific tests should focus on:

1. model fetch success/failure
2. auth propagation without key leakage
3. stream event forwarding
4. disconnect behavior at the browser adapter boundary

## Delivery Rules

1. do not reopen the already completed preparatory refactor in the name of WASM
2. keep native behavior stable while introducing browser support
3. prefer one provider end to end over broad partial support
4. keep model discovery ownership explicit; do not leave it hidden inside server startup
5. extract shared logic out of `xrouter-app` when needed; do not make browser code depend on it
6. update this plan when blocker ownership or phase order changes
