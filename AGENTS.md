# AGENTS.md

## Project Context

XRouter is a Rust codebase with the active workspace under `xrouter/`.

Rust is the source of truth.

Canonical project architecture is documented in `ARCHITECTURE.md`.

## Baseline Quality Contract

Every code change must:

1. compile successfully
2. pass formatting checks
3. pass lint checks with no warnings unless an exception is explicitly documented
4. include tests for impacted behavior, including happy path and failure path where applicable
5. avoid embedding secrets, tokens, private keys, or passwords in source code, fixtures, or logs

## Canonical Commands

If a `justfile` exists, use it as the preferred entry point.
Run Rust commands from `xrouter/` unless a different workspace root is explicitly required.

Minimum cycle after Rust code changes:

1. `just fmt` or `cargo fmt --all`
2. `cargo test -p <changed-crate>`
3. `cargo test --all-features` when shared/core/contract boundaries changed
4. `cargo clippy --all-targets --all-features -- -D warnings`

If auto-fix is needed:

1. `just fix -p <crate>` if available
2. rerun `just fmt` or `cargo fmt --all`

## Architecture Rules

1. `xrouter-app` is the composition root.
2. `xrouter-core` owns orchestration and lifecycle semantics.
3. `xrouter-contracts` owns canonical DTOs and shared typed boundaries.
4. provider transport and provider-specific normalization belong in client crates, not in routes
   or core orchestration
5. route handlers must depend on abstractions and canonical models, not on provider quirks
6. crate roots should stay thin; do not rebuild monolithic `lib.rs` files
7. prefer explicit wiring and trait boundaries over hidden DI or global mutable state

## Lifecycle Contract

The active lifecycle contract is defined by `formal/xrouter.tla`.

Required lifecycle semantics:

1. canonical flow is `ingest -> tokenize -> generate(stream) -> done|failed`
2. `ingest` is responsible for normalization and execution-context enrichment
3. `tokenize` is responsible for usage-related computation and metadata preparation
4. `generate` may emit stream events before terminal completion
5. disconnect in `ingest|tokenize` fails fast
6. disconnect in `generate` does not cancel in-flight generation lifecycle

If lifecycle semantics change, the same change must update:

1. `formal/xrouter.tla`
2. `formal/xrouter.cfg`
3. `formal/property-map.md`
4. `formal/trace-schema.md`
5. matching Rust tests

## API Contract Rules

1. OpenAI Responses API is the canonical external contract.
2. Chat Completions support is an adapter over the Responses flow.
3. Intentional contract deviations must be documented in `docs/`.
4. Do not let legacy or provider-specific payload shapes define internal domain models.

## Provider Rules

1. Keep provider-specific normalization scoped to the target provider only.
2. Perform canonicalization at boundaries, not in unrelated business logic.
3. Shared retry, timeout, auth, and transport behavior belongs in transport modules.
4. Fail-soft parsing is acceptable only when bounded, explicit, and observable.
5. Temporary compatibility hacks must explain:
   - why they exist
   - what keeps them safe
   - when they can be removed

## Testing Policy

Primary pattern: deterministic scenario tests through public APIs.

For important flows:

1. arrange local mocks/fakes only
2. execute one public operation
3. assert observable behavior at the relevant boundary

Preferred assertion layers:

1. returned value or returned error
2. emitted stream events
3. persistent or in-memory state when applicable
4. outbound request contract for provider/client crates

Reliability rules:

1. do not use real external APIs or real network in tests
2. do not rely on wall clock drift
3. serialize tests if they touch global process state
4. use behavior-first test names
5. keep tests co-located near implementation unless cross-crate coverage requires otherwise

## Observability Rules

1. provider request start/end, stream lifecycle, and error paths must remain observable
2. propagate trace context through internal services and outgoing provider calls
3. include request/model/provider identifiers as structured attributes
4. never log secrets or raw credentials

## Code Change Style

1. prefer changing existing architecture over adding one-off helper layers
2. keep changes minimal and sufficient
3. avoid unrelated refactors in the same change unless they are required to make the boundary
   coherent
4. do not hide failing checks; report the exact failed command and why it failed
5. when contracts, config, or public behavior change, update docs and tests in the same change

## Repository Structure Guidance

Current workspace direction:

```text
/
  AGENTS.md
  ARCHITECTURE.md
  PLAN.md
  formal/
  xrouter/
    Cargo.toml
    justfile
    crates/
      xrouter-app/
      xrouter-core/
      xrouter-contracts/
      xrouter-clients-openai/
      xrouter-observability/
```

Crate boundaries may evolve, but any boundary change must include:

1. a short rationale
2. updated documentation
3. preserved lifecycle and contract semantics unless explicitly changed

## Security and Operational Discipline

1. do not run destructive git commands without explicit request
2. do not bypass sandbox or permission controls
3. do not add secrets to code, tests, fixtures, or logs

## PR Acceptance Criteria

A change is ready when:

1. required commands pass
2. impacted behavior has both success and failure coverage where it matters
3. contract or config changes are reflected in docs and code
4. no temporary assumption is left unexplained
