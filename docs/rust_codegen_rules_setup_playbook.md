# How to Set Up “Excellent” Code-Generation Rules in Any Rust Repository

This guide is a reusable template for making AI-generated Rust code consistent, safe, testable, and maintainable.

It combines:

1. Repository-local policy (`AGENTS.md`-style rules).
2. Rust toolchain enforcement (`rustfmt`, `clippy`, tests).
3. CI gates that block low-quality output.
4. A practical workflow for humans + AI coding agents.

---

## 1. Define a Single Source of Truth for Rules

Create a top-level policy file (for example `AGENTS.md`) with:

1. Required commands after code changes:
- `cargo fmt` (or `just fmt`)
- `cargo test` for changed crates
- optional full suite gate (`cargo test --all-features`)

2. Lint/style constraints:
- clippy rules you care about (`collapsible_if`, `uninlined_format_args`, etc.)
- naming and module conventions
- test conventions (assert full objects, not single fields)

3. Safety constraints:
- no destructive git commands by default
- no secret embedding in source
- no bypass of security/sandbox controls

4. Domain-specific conventions:
- UI style rules
- protocol/API naming rules
- docs/schema update rules

Why: agents follow explicit local policy better than implied style.

---

## 2. Enforce Formatting and Lints in Tooling

## 2.1 `rustfmt`

Add `rustfmt.toml` (minimal example):

```toml
edition = "2021"
imports_granularity = "Item"
group_imports = "StdExternalCrate"
use_field_init_shorthand = true
```

## 2.2 Clippy lints as hard errors

In CI, run:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

If you need strict repo-wide lint policy, add in crate roots:

```rust
#![deny(clippy::collapsible_if)]
#![deny(clippy::uninlined_format_args)]
#![deny(clippy::redundant_closure_for_method_calls)]
```

Use cautiously: start with CI-level enforcement first if migration cost is high.

---

## 3. Standardize Commands with `justfile`

Create a `justfile` so both humans and agents run the same commands:

```make
set shell := ["bash", "-cu"]

fmt:
    cargo fmt --all

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-features

fix:
    cargo clippy --fix --all-features --tests --allow-dirty

check: fmt lint test
```

This removes ambiguity in instructions like “run checks.”

---

## 4. Add a Deterministic Test Strategy (Not Just “More Tests”)

Adopt these patterns:

1. Scenario integration tests for critical flows.
2. Local mock servers/fakes for external APIs.
3. Assertions on:
- return value,
- persisted state,
- in-memory state,
- outbound request contract.

4. Snapshot tests only for stable UI/text output.
5. Serial execution for tests touching global process state (env vars, global singletons).

---

## 5. CI Pipeline Template

Use two levels: fast PR gate + full gate.

### Fast PR gate

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. targeted tests for changed crates/modules

### Full gate (merge/nightly)

1. `cargo test --all-features`
2. snapshot checks (if applicable)
3. optional platform matrix (linux/macos/windows)

GitHub Actions skeleton:

```yaml
name: rust-ci
on: [pull_request, push]

jobs:
  fast:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo test -p your-core-crate

  full:
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features
```

---

## 6. Agent-Facing Rules That Actually Work

To get high-quality generated code, include these explicit instructions in your policy file:

1. “Always run formatter after Rust code changes.”
2. “Run tests for touched crate; full suite if shared/core modules changed.”
3. “Prefer editing existing architecture over introducing one-off helpers.”
4. “Compare full structs in tests with `assert_eq!` where practical.”
5. “If config schema changes, regenerate schema artifacts.”
6. “Do not silently ignore failing checks; report exact failing command and reason.”

If you skip explicit instructions, agent output quality drops fast.

---

## 7. Baseline Quality Contract for Generated Code

Require each change to satisfy:

1. Builds cleanly.
2. Lints cleanly (or justified exceptions).
3. Tests pass for impacted area.
4. No secrets hardcoded.
5. Behavior covered by tests for both success and failure path.
6. Related docs/config/schema updated.

You can encode this in PR template checkboxes.

---

## 8. Recommended Repo Layout for Policy and Automation

```text
/
  AGENTS.md                  # repo-specific coding/testing rules
  justfile                   # canonical commands
  rustfmt.toml               # formatting config
  .github/workflows/rust-ci.yml
  docs/
    testing.md               # project test strategy
    codegen-policy.md        # optional AI policy details
```

---

## 9. Rollout Plan for Existing Repos

If repo is messy, do this incrementally:

1. Add `fmt` + `clippy` CI gates first.
2. Standardize commands via `justfile`.
3. Add/upgrade scenario tests for top 3 critical flows.
4. Add policy file (`AGENTS.md`) with concrete, enforceable rules.
5. Move from “warnings allowed” to stricter clippy policy.
6. Add full-suite/nightly gates after test stability improves.

---

## 10. Copy/Paste Starter Policy Snippet

Use as a base in your `AGENTS.md`:

```md
# Rust Repo Rules

- After Rust changes, run:
  - `just fmt`
  - `cargo test -p <changed-crate>`
  - if shared/core changed: `cargo test --all-features`

- Lints:
  - Prefer collapsed `if` where clippy suggests.
  - Inline `format!` args when possible.
  - Prefer method refs over redundant closures.

- Tests:
  - Prefer `pretty_assertions::assert_eq`.
  - Prefer full-object equality assertions.
  - Avoid global env mutation unless serialized and justified.

- Safety:
  - Never hardcode secrets.
  - Never run destructive git commands unless explicitly requested.
```

---

## 11. “Done” Definition for AI-Assisted Rust Changes

A change is done only when:

1. Code compiles.
2. Formatting is clean.
3. Lints are clean (or documented exceptions).
4. Impacted tests pass.
5. Any generated artifacts (schema/docs/snapshots) are updated intentionally.
6. Final report includes exactly what changed and what was verified.

That is the practical setup that makes generated Rust code consistently production-grade.

