# AGENTS.md

## Project Context

This repository is treated as a new Rust project.
The legacy Python code in `src/` is temporary and does not define architectural rules for the new codebase.
The primary goal is to migrate domain logic to Rust with predictable quality and testability.
Rust code is expected to live under `xrouter/`.

## Baseline Quality Contract

Every change must:

1. Compile successfully.
2. Pass formatting checks.
3. Pass lint checks with no warnings (or with an explicitly documented exception).
4. Include tests for impacted behavior (happy path + failure path).
5. Avoid embedding secrets, tokens, private keys, or passwords in source code.

## Canonical Commands

If a `justfile` exists, use it as the single entry point.
Run Rust commands from `xrouter/` unless a different workspace root is explicitly introduced.

Minimum cycle after Rust code changes:

1. `just fmt` (or `cargo fmt --all`).
2. `cargo test -p <changed-crate>` for the impacted crate.
3. If shared/core/protocol areas were touched: `cargo test --all-features`.

Lints:

1. `cargo clippy --all-targets --all-features -- -D warnings`.
2. If auto-fix is needed: `just fix -p <crate>` (or equivalent).
3. After `fix`: run `just fmt` again.

## Code Change Style

1. Prefer editing existing architecture over adding one-off helper layers.
2. Keep changes minimal and sufficient; avoid unrelated refactors in the same change.
3. Do not hide failing checks: always report the exact failed command and reason.
4. When config/schema/API contracts change, update related artifacts and docs in the same PR.

## Migration Architecture Rules

1. Treat the Rust codebase as the source of truth; legacy Python code is reference material only.
2. Keep request execution as a handler pipeline (Chain of Responsibility), but make handlers explicit and typed.
3. Use canonical stage names: `ingest -> tokenize -> hold? -> generate -> finalize?`.
4. `ingest` is responsible for normalization and metadata enrichment (not only field remapping).
5. `hold` and `finalize` are enabled only when billing integration feature flags are on.
6. Do not use stage name `completion` in new Rust core flow; use `generate`.
7. Replace container-heavy DI with an explicit composition root:
   - configuration loading,
   - dependency wiring,
   - trait-based interfaces for services.
8. Separate orchestration from transport:
   - orchestration/use-cases in core crates,
   - HTTP/provider clients in dedicated client crates.
9. Provider integrations must be reusable client packages, not tightly coupled router internals.
10. Avoid provider-specific condition branches in API routers; route layer should depend on abstractions only.
11. Use one canonical internal request/response model and map adapters at boundaries.

## Formal Model Contract (Mandatory)

The formal model in `formal/xrouter.tla` is the lifecycle contract for scaffold and implementation.

1. Keep lifecycle semantics aligned with the formal stages:
   - `ingest -> tokenize -> hold? -> generate(stream) -> finalize? -> done|failed`.
2. Preserve post-paid settlement behavior:
   - if billing is enabled and billable tokens were generated, terminal success requires charge commit or explicit recovery path.
3. Preserve disconnect behavior:
   - disconnect in `ingest|tokenize|hold` fails fast,
   - disconnect in `generate|finalize` does not cancel settlement.
4. Preserve hold lifecycle constraints:
   - no terminal state may retain an acquired hold.
5. Preserve debt safety constraints:
   - `reset` is forbidden while `chargeRecoveryRequired = true`.
6. Preserve finalize commit safety:
   - finalize path must prevent double commit/idempotency violations.
7. Preserve financial scope semantics:
   - `externalLedger` tracks commits performed by this service path,
   - external debt settlement is represented separately (for example `recoveredExternally`) and must not be silently conflated with local ledger commits.
8. Any lifecycle or financial semantics change requires:
   - update of `formal/xrouter.tla`,
   - update of `formal/property-map.md` and `formal/trace-schema.md`,
   - successful TLC run with `formal/xrouter.cfg`,
   - matching code/test updates in the same change.

## API Contract Baseline

1. The canonical external contract is OpenAI Responses API.
2. Legacy xrouter/openrouter-specific contract behavior is considered transitional and should not drive new Rust domain models.
3. Chat Completions compatibility should be implemented as an adapter layer over the core Responses flow.
4. Any intentional contract deviation must be documented in `docs/` with migration rationale and removal plan.

## Migration Guardrails

1. Do not port placeholder business logic (for example fixed token estimates or hardcoded defaults used as temporary stubs).
2. Do not hide degraded behavior behind implicit fallback; fallback behavior must be explicit, narrow, and observable.
3. Any temporary compatibility hack must include:
   - why it exists,
   - when it can be removed,
   - how correctness is protected until removal.

## Provider Normalization Rules

1. Keep provider-specific normalization scoped to the target provider only; non-target providers must preserve input/output unchanged.
2. Perform canonicalization at boundaries (ingest/output adaptation), not inside unrelated business logic.
3. Use fail-soft parsing with bounded repair steps for legacy payloads:
   - retry known-safe repairs,
   - do not silently mutate semantics,
   - return original payload when normalization is not applicable.
4. Keep temporary compatibility hacks local, explicit, and removable.
5. Prefer small pure helper functions for normalization steps.
6. Log structured payload summaries for debugging; never log secrets or raw sensitive payloads.

## Observability Baseline

1. Instrument LLM calls with OpenTelemetry spans from day one.
2. Create dedicated spans for provider request start/end, stream lifecycle, and error paths.
3. Propagate trace context through internal services and outgoing provider client calls.
4. Include request/model/provider/generation identifiers as span attributes (without secrets).

## Testing Policy (Primary Priority)

Primary pattern: deterministic scenario tests through public APIs.

For each key scenario:

1. `Arrange`: start local mock/fake dependencies, prepare context, seed state.
2. `Act`: execute one public operation.
3. `Assert`: verify observable behavior across all relevant layers.

Required assertion layers (where applicable):

1. Return value or error.
2. Persistent state (file/database/storage).
3. In-memory state (cache/managers/sessions).
4. Outbound request contract (method/path/body/headers/count).

Reliability rules:

1. Do not use real external APIs or network in tests.
2. For global process state (for example env vars), serialize tests (`serial_test` or equivalent).
3. Do not rely on wall clock drift; control time explicitly in tests.
4. Use behavior-first test names (what should happen, not how it is implemented).
5. Default to co-located tests near implementation (`#[cfg(test)] mod tests`) for autonomous delivery.
6. Introduce a separate integration `tests/` directory only when cross-crate/e2e coverage requires it.

## Snapshot Tests

Use snapshot tests only for stable text/UI output.

Rules:

1. Never accept snapshots automatically.
2. Review diffs first, then accept only intended changes.
3. Do not rely only on snapshots for critical behavior; add behavioral tests as well.

## CI Gates

PR gate (fast):

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. Targeted tests for changed crates/domains

Full gate (merge/nightly):

1. `cargo test --all-features`
2. Snapshot checks (if used)
3. Platform matrix if needed

## Security and Operational Discipline

1. Do not run destructive git commands without explicit request (`git reset --hard`, mass deletes, and similar).
2. Do not bypass sandbox/permission controls.
3. Do not add secrets to code, tests, fixtures, or logs.

## Recommended New Rust Repository Structure

This structure is a starting scaffold, not a frozen target architecture.
The Codex implementer may revise crate boundaries and layout during implementation.
Any scaffold change must include:

1. Short rationale for the change.
2. Updated documentation reflecting the new structure.
3. Confirmation that canonical flow and contract rules remain intact.

```text
/
  AGENTS.md
  docs/
    architecture.md
    testing.md
    codegen-policy.md
  xrouter/
    Cargo.lock
    Cargo.toml
    justfile
    rustfmt.toml
    crates/
      xrouter-app/                  # binary crate (axum/actix entrypoint, composition root)
      xrouter-core/                 # orchestration, handler chain, domain services
      xrouter-contracts/            # canonical API/domain DTOs (Responses-first)
      xrouter-clients-openai/       # reusable OpenAI-compatible client
      xrouter-clients-openrouter/   # reusable OpenRouter client
      xrouter-clients-gigachat/     # reusable GigaChat client
      xrouter-clients-usage/        # reusable billing/usage client
      xrouter-storage/              # cache/repository adapters
      xrouter-observability/        # logging/tracing/metrics adapters
    # no top-level tests directory by default; co-located tests are preferred
    .github/workflows/rust-ci.yml
```

## PR Acceptance Criteria

A PR is ready when:

1. All required commands pass.
2. Tests cover both success and failure behavior for impacted areas.
3. Contract/config changes are reflected in code and documentation.
4. No temporary assumptions remain without an explicit TODO, context, and removal plan.
