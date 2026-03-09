# WASM Plan

This is the implementation plan for the WASM/browser track in `xrouter/`.

The preparatory architecture refactor is already done in the main codebase.
This plan starts from that new baseline.

## Goal

Build a browser-safe consumer layer that can reuse XRouter contracts, orchestration, and provider
logic without embedding the native server app.

## Current Starting Point

Already completed in the main codebase:

1. `xrouter-app` is a thin native composition root
2. `xrouter-core` owns orchestration and exposes a sink-based streaming boundary
3. `xrouter-clients-openai` is split into protocol, parser, and native transport layers
4. route, provider, and transport concerns are no longer mixed the way they were before

This means the WASM track can start directly with portability work.

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

Confirm which crates already compile for `wasm32-unknown-unknown` and capture exact blockers.

Work:

1. check `xrouter-contracts`
2. check `xrouter-core`
3. inspect dependency feature flags for portability-sensitive crates
4. document blockers with exact crate/module ownership

Exit criteria:

1. a concrete blocker list exists
2. browser work is no longer based on guesses

## Phase 2: Browser Transport Boundary

Status:

- pending

Objective:

Define the minimal browser-safe transport/runtime abstraction needed by shared provider logic.

Work:

1. identify what `transport.rs` currently owns that must become browser-replaceable
2. define a small trait surface for:
   - JSON request/response execution
   - streaming event acquisition
   - retry/timing hooks if needed
3. decide whether this abstraction lives:
   - inside `xrouter-clients-openai`
   - or in a small shared runtime/browser-facing crate

Exit criteria:

1. native transport and browser transport can target the same logical provider surface
2. abstraction is small and explicit

## Phase 3: First Browser-Safe Provider Path

Status:

- pending

Objective:

Get one provider working end-to-end through the browser-safe path.

Recommended first provider:

- `deepseek`

Why:

1. already exercised heavily in smoke flows
2. simpler first target than router-of-routers behavior

Work:

1. reuse `protocol.rs` request shaping where possible
2. reuse `parser.rs` stream/response normalization where possible
3. implement browser transport adapter
4. validate streamed turn end-to-end

Exit criteria:

1. one provider works from browser/WASM through shared logic
2. native behavior remains intact

## Phase 4: Dedicated WASM Crate

Status:

- pending

Objective:

Introduce a dedicated crate for the browser-facing layer if Phase 3 proves the boundary.

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
3. keep crate scope narrow

Exit criteria:

1. browser consumer does not depend on native app code
2. browser support has a clear workspace home

## Phase 5: Expand Provider Support

Status:

- pending

Objective:

Add more providers after the first browser-safe path is proven.

Order should be pragmatic:

1. simple OpenAI-compatible paths first
2. more specialized providers after the transport boundary is stable
3. `xrouter` endpoint support only after direct-provider paths are solid

Exit criteria:

1. adding providers reuses the same browser-safe architecture
2. provider work does not reintroduce transport/provider mixing

## Delivery Rules

1. do not reopen the already completed preparatory refactor in the name of WASM
2. keep native behavior stable while introducing browser support
3. prefer one provider end-to-end over broad partial support
4. update this plan when blocker ownership or phase order changes
