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
The first milestone is this end-to-end browser acceptance slice.

The broader architectural target behind that demo is:

1. browser build has zero dependency on server-only crates
2. shared logic is reused through portable crates
3. browser-needed logic currently trapped in `xrouter-app` is extracted, not imported directly
4. the Rust browser crate and any demo frontend app are separate deliverables

Definition of done for the XRouter wasm track:

1. `xrouter-browser` is portable and browser-ready
2. the accepted BYOK browser-safe providers work in-browser with real streaming
3. browser model discovery works
4. request-scoped cancellation exists
5. the library is consumable by external hosts
6. a bundled UI is optional and not required

## Current Starting Point

Already completed in the main codebase:

1. `xrouter-app` is a thin native composition root
2. `xrouter-core` owns orchestration and exposes a sink-based streaming boundary
3. `xrouter-clients-openai` is split into protocol, parser, and native transport layers
4. provider request execution already supports per-request bearer override
5. route, provider, and transport concerns are no longer mixed the way they were before

Main gaps still visible from the codebase:

1. `xrouter-app` still owns parts of startup model catalog assembly
2. packaging guidance for downstream consumers needs to be documented explicitly

## Current Status

Implementation is complete for the current wasm scope.

Completed so far:

1. `xrouter-contracts` passes `cargo check -p xrouter-contracts --target wasm32-unknown-unknown`
2. `xrouter-core` passes `cargo check -p xrouter-core --target wasm32-unknown-unknown`
3. workspace `uuid` is configured with browser RNG support through the `js` feature
4. `xrouter-clients-openai` was split so `parser` and `protocol` are portable while native
   `clients` and `transport` stay target-gated
5. `xrouter-clients-openai` now passes `cargo check -p xrouter-clients-openai --target wasm32-unknown-unknown`
6. a portable `ProviderRuntime` boundary now exists between provider clients and native transport
7. native validation stayed green:
   - `cargo check -p xrouter-app`
   - `cargo test -p xrouter-clients-openai`
   - `cargo test --all-features`
   - `cargo clippy --all-targets --all-features -- -D warnings`

Current active focus:

1. keeping the accepted provider set stable
2. leaving downstream host integration out of `xrouter` scope
3. treating excluded providers as future work, not blockers

## Phase 0: Track Setup

Status:

- completed

Completed:

1. WASM planning moved out of the main architecture/plan
2. a dedicated browser track now exists under `xrouter/crates/xrouter-browser/`
3. main architecture is now stable enough to use as the baseline for browser work

## Phase 1: Portability Audit

Status:

- completed

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

Completed:

1. verified `xrouter-contracts` and `xrouter-core` compile for `wasm32-unknown-unknown`
2. verified the portable reuse surface inside `xrouter-clients-openai` can compile under wasm
   after separating native-only dependencies
3. identified the first concrete blocker and fixed it:
   `uuid` needed browser RNG support
4. identified the next extraction target:
   model discovery remains trapped inside `xrouter-app/startup`

## Phase 2: Browser Transport Boundary

Status:

- completed

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

Progress:

1. `xrouter-clients-openai` now exposes portable `parser` and `protocol` modules publicly
2. provider clients no longer depend directly on concrete native transport internals
3. a portable `ProviderRuntime` trait now defines the runtime seam for:
   - URL construction
   - streamed chat execution
   - streamed responses execution
   - form-post JSON execution
4. native `HttpRuntime` implements that trait
5. native-only constructors remain for server compatibility, while portable `with_runtime(...)`
   constructors exist for future browser injection
6. `gigachat` remains native-only for now because its OAuth/token lifecycle is not part of the
   first browser provider slice

## Phase 3: Browser Model Discovery Boundary

Status:

- completed

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

Progress:

1. provider model response DTOs, normalization, and registry fallback logic were moved into
   `xrouter-clients-openai`
2. portable request builders now exist for:
   - OpenRouter `/models`
   - generic provider `/models`
   - xrouter `/models`
   - Gigachat OAuth and `/models`
3. `xrouter-app` now acts as a native executor for those shared request shapes instead of owning
   the request construction itself
4. a dedicated `xrouter-browser` crate now exists as the browser/WASM composition root
5. `xrouter-browser` can execute shared model-discovery request shapes through browser `fetch`
   for:
   - OpenRouter `/models`
   - generic provider `/models`
   - xrouter `/models`
6. browser model discovery now has explicit HTTP-status and JSON-parse failure handling
7. browser-specific execution for streamed inference now exists through a browser implementation of
   `ProviderRuntime`
8. `xrouter-browser` now exposes a minimal inference API for the browser-safe provider set:
   - `BrowserInferenceClient`
   - `BrowserProvider::{DeepSeek, OpenAi, OpenRouter, Zai}`
   - `DEFAULT_DEMO_PROMPT = "Hello, what can you do?"`
9. `xrouter-browser` now also exposes a wasm-consumable API surface:
   - `WasmBrowserClient`
   - `fetchModelIds()`
   - `runTextStream()`
   - `runDemoPromptStream()`

Exit criteria:

1. browser can fetch and display models for the first provider
2. provider quirks do not leak into the UI layer
3. model discovery no longer depends on `xrouter-app`
4. shared model-discovery logic, if any, is owned outside `xrouter-app`

## Phase 4: First Browser-Safe Provider Path

Status:

- completed

Objective:

Get one provider working end to end through the browser-safe path.

Accepted browser-safe provider set:

1. `deepseek`
2. `openai`
3. `openrouter`
4. `zai`

Explicitly excluded for now:

1. `yandex`
   requires extra project/folder configuration beyond a simple API key
2. `gigachat`
   requires an OAuth/token flow that is intentionally out of the current wasm slice

Work:

1. wire the accepted BYOK providers through browser-safe request building
2. wire browser transport into the provider clients
3. verify stream parsing in the browser
4. verify request-scoped cancellation
5. expose a minimal wasm-consumable API for host runtimes

Completed:

1. `BrowserProviderRuntime` executes streamed provider requests in the browser
2. `BrowserInferenceClient` drives the accepted browser-safe provider set end to end
3. `WasmBrowserClient` exposes the browser-safe provider set to JS consumers
4. request-scoped cancellation now exists through `cancel(request_id)`
5. manual smoke validated real browser model loading, live streaming, and cancellation for:
   - `deepseek`
   - `openai`
   - `openrouter`
   - `zai`

Exit criteria:

1. the accepted provider set works from browser to upstream with live streaming
2. BYOK is supported without any server relay
3. browser cancellation is part of the contract

## Phase 5: Browser Packaging

Status:

- completed

Objective:

Make the browser router consumable by external hosts without coupling it to any one UI.

Work:

1. keep `xrouter-browser` as the browser/WASM Rust crate
2. document its public consumer-facing API
3. document recommended `wasm-pack` packaging for downstream hosts
4. keep host-specific adapters out of `xrouter-browser`

Progress:

1. `xrouter-browser` exists as the browser/WASM composition root
2. consumer-facing usage is documented in `README.md`
3. downstream host integrations are explicitly out of scope for `xrouter`

Exit criteria:

1. browser library can be consumed without relying on the demo app
2. host-specific integration remains a downstream concern

## Current Closeout

The wasm track is considered complete inside `xrouter` for the current accepted scope.

Frozen scope:

1. supported browser-safe providers:
   - `deepseek`
   - `openai`
   - `openrouter`
   - `zai`
2. intentionally unsupported for now:
   - `yandex`
   - `gigachat`
3. downstream host integration remains out of scope for `xrouter`

Future work, if resumed later:

1. add new providers only after real browser CORS and streaming acceptance
2. evaluate whether npm packaging is worth doing as a first-party deliverable
3. implement a dedicated browser slice for `gigachat` only if its OAuth flow is worth the cost

## Phase 6: Demo Frontend

Status:

- optional

Objective:

Provide a browser harness to manually prove the acceptance slice.

Notes:

1. the demo frontend is not part of the required XRouter wasm deliverable
2. it may exist as a separate example app or smoke harness
3. its existence does not redefine `xrouter-browser` into a UI-owned product

Completed:

1. a separate Vite/Svelte demo app exists outside the Rust crate
2. the demo app proves `BYOK -> models -> select -> stream -> cancel`

## Out of Scope

These items are intentionally out of scope for the XRouter wasm track itself:

1. downstream application-specific protocols
2. `codex-rs`-specific adapters
3. shipping a required browser UI
4. parity with every provider before the first accepted browser slice is stable
