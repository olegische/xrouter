# Rust Test Pattern Playbook (Based on `codex-rs`)

This document describes a practical, reliability-first testing pattern used in a large Rust codebase (`codex-rs`).  
It is written so you can reuse the same pattern in another Rust project with minimal adaptation.

## 1. Core Philosophy

The pattern is not “many tiny unit tests for private helpers.”  
The pattern is “deterministic scenario tests over real module boundaries.”

In practice:

1. Test behavior through public APIs.
2. Isolate external dependencies via local mocks/fakes.
3. Assert both outcome and side effects.
4. Cover happy path and failure path symmetrically.
5. Keep tests deterministic and parallel-safe.

This gives high confidence without over-coupling tests to implementation details.

## 2. Test Pyramid Used Here

The repo effectively uses three complementary layers:

1. Unit tests
Small, focused checks for pure logic.

2. Scenario integration tests
The most important layer. These tests run realistic flows (auth refresh, retries, request/response cycles) with controlled external dependencies.

3. Snapshot tests (UI/text-heavy outputs)
Used heavily for TUI rendering and text formatting stability.

The reliability anchor is layer 2.

## 3. Structural Pattern for Scenario Tests

Use this shape consistently:

1. `Arrange`
- Start local mock server (`wiremock`).
- Mount expected endpoint behavior.
- Build test context/harness.
- Seed initial state (e.g., `auth.json`, in-memory state).

2. `Act`
- Call one public operation (example: `auth_manager.refresh_token().await`).

3. `Assert`
- Assert persisted state (storage).
- Assert in-memory/cached state.
- Assert outbound request count and payload shape.
- Assert specific errors for negative paths.

This is exactly what makes tests robust against internal refactors.

## 4. Concrete Example Pattern (OAuth Refresh)

The file:

- `<REPO_ROOT>/codex-rs/core/tests/suite/auth_refresh.rs`

Pattern highlights:

1. A dedicated `RefreshTokenTestContext` holds:
- temp `codex_home`
- shared `AuthManager`
- env guard for endpoint overrides

2. Tests mount mocked HTTP responses:
- 200 success with tokens
- 401 unauthorized (permanent failure)
- 500 server error (transient failure)

3. Tests assert:
- stored auth content changed correctly
- cached auth in `AuthManager` reflects persisted updates
- request body includes expected form/json fields
- no request is sent when token is still fresh

4. Time-based refresh behavior is tested explicitly:
- near-expiry token refreshes
- fresh token does not refresh

This gives high confidence in both logic and integration boundaries.

## 5. Example Template You Can Reuse

```rust
#[tokio::test]
async fn scenario_refresh_updates_storage_and_cache() -> anyhow::Result<()> {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth/token"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "new-access",
            "refresh_token": "new-refresh"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let ctx = TestContext::new(&server)?;
    ctx.seed_auth(/* stale token */)?;

    ctx.auth_manager.refresh_token().await?;

    let stored = ctx.load_auth()?;
    pretty_assertions::assert_eq!(stored.oauth.unwrap().access_token, "new-access");

    let cached = ctx.auth_manager.auth().await.unwrap();
    pretty_assertions::assert_eq!(cached.get_token().unwrap(), "new-access");

    server.verify().await;
    Ok(())
}
```

Notes:

1. Keep assertions deep (`assert_eq!` on full structs where possible).
2. Keep one dominant behavior per test.
3. Use explicit names that describe observable behavior.

## 6. Pattern for Request/Protocol Flows (SSE / App Events)

For richer protocol flows, this repo recommends helper utilities:

- `core_test_support::responses`

Preferred style:

1. Mount a scripted SSE stream with helper constructors.
2. Submit one user operation.
3. Inspect captured outbound request body via typed helper methods.

Example shape:

```rust
let mock = responses::mount_sse_once(&server, responses::sse(vec![
    responses::ev_response_created("resp-1"),
    responses::ev_function_call(call_id, "shell", &serde_json::to_string(&args)?),
    responses::ev_completed("resp-1"),
])).await;

codex.submit(Op::UserTurn { /* ... */ }).await?;

let request = mock.single_request();
// assert on structured fields, not ad-hoc string matching
```

This pattern avoids brittle JSON-string tests.

## 7. Snapshot Testing Pattern (UI/TUI)

Snapshot tests are used when output is primarily presentation.

Flow:

1. Run tests to generate `.snap.new`.
2. Review snapshot diffs.
3. Accept snapshots only for intentional UI/text changes.

Commands used in this repo:

```bash
cargo test -p codex-tui
cargo insta pending-snapshots -p codex-tui
cargo insta show -p codex-tui path/to/file.snap.new
cargo insta accept -p codex-tui
```

Rule of thumb:

1. Never accept snapshots blindly.
2. Pair snapshots with targeted behavioral tests for critical flows.

## 8. Determinism and Flake Prevention Rules

Use these rules aggressively:

1. Avoid real network and real third-party APIs in tests.
2. Use local temp directories for filesystem state.
3. Serialize tests that mutate global state:
- use `serial_test` when touching process env.
4. Prefer explicit time setup over relying on wall clock drift.
5. Assert exact failure categories (transient vs permanent, etc.).
6. Keep test fixtures minimal and local to the test module.

## 9. Assertion Strategy

Reliable tests in this style assert all critical observable layers:

1. Return value / Result.
2. Persistent state (disk/db).
3. Runtime state (cache/memory manager state).
4. Outbound interaction contract (URL/method/body/headers/count).

If one layer is not asserted, regressions can hide there.

## 10. Naming and Organization Strategy

Use behavior-first names:

1. `refresh_oauth_token_succeeds_updates_storage`
2. `auth_load_does_not_refresh_oauth_when_token_is_fresh`
3. `refresh_token_returns_transient_error_on_server_failure`

Organize tests by feature suite:

1. One suite file per domain (`auth_refresh.rs`, `thread_resume.rs`, etc.).
2. Shared context object per suite.
3. Helpers for fixture construction at the bottom of the file.

## 11. How to Adapt This Pattern to Another Rust Project

Minimal migration plan:

1. Introduce one `tests/suite/<feature>.rs` per critical feature.
2. Build one reusable harness/context per suite.
3. Add a local mock server abstraction for external dependencies.
4. Define a consistent assertion checklist:
- result
- storage
- cache/runtime
- outbound contract
5. Add serial guards for global state tests.
6. Add snapshot tests only where rendering/text stability matters.

Do not start by splitting everything into tiny unit tests.  
Start with a few high-value scenario tests that lock key behavior.

## 12. Test Execution Pipeline (Repo-Specific + Generalizable)

### 12.1 Pipeline used in this repository (`codex-rs`)

1. Format
```bash
cd codex-rs
just fmt
```

2. Run tests for changed crates
```bash
cargo test -p codex-core
cargo test -p codex-tui
# (or whichever crate changed)
```

3. If changes touched shared/core/protocol surfaces, run full suite
```bash
cargo test --all-features
```

4. Run clippy auto-fix for changed crate(s)
```bash
just fix -p codex-core
just fix -p codex-tui
```

5. Format again after fixes
```bash
just fmt
```

In this repo, the guidance says not to re-run tests after `fix/fmt` in the same cycle unless explicitly needed.

### 12.2 Practical CI pipeline for another Rust project

Fast PR gate:

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -D warnings`
3. targeted scenario suites for touched domains

Full gate (merge/nightly):

1. full test matrix (all features/platform variants as needed)
2. snapshot diff verification
3. optional mutation/fuzz subsets for critical parsing/protocol modules

## 13. Final Checklist for “Production-Grade” Test Quality

Before merging, ask:

1. Did I test both success and failure modes?
2. Did I verify no unintended external call happens?
3. Did I assert both persisted and in-memory states?
4. Are tests deterministic under parallel execution?
5. Are names explicit about behavior?
6. Are snapshots reviewed, not blindly accepted?

If all six are true, your test suite is usually strong enough to support fast iteration safely.
