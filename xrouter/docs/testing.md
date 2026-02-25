# xrouter Rust Testing Strategy

Tests are co-located with implementation (`#[cfg(test)]`).

## Test style

Primary style is data-driven scenario testing with a single `check_fixture` entry point.

Each scenario is described as:

1. Fixture text (`key=value` lines) describing input.
2. Expected snapshot text describing observable output.

This keeps tests resilient to internal refactors and focused on behavior at boundaries.

## Fixture format

Current fixture format is line-based:

```text
name=responses_success
method=POST
path=/api/v1/responses
body={"model":"gpt-4.1-mini","input":"hello world","stream":false}
```

Core fixtures follow the same style, for example:

```text
name=disconnect_ingest_fails_fast
model=fake
input=world
provider=success
disconnect=ingest
```

## Snapshot format

Snapshots are normalized summaries, not raw full payload dumps.

Examples:

```text
status=200
json.status=completed
json.output_text=[openrouter] hello world
json.usage_total=4
```

```text
kind=err
error_kind=ClientDisconnected
error=client disconnected during ingest
```

This keeps snapshots stable and readable while still verifying external behavior.

## Current coverage

- Core pipeline happy path.
- Core provider failure path.
- Core disconnect fail-fast in early stages.
- Core disconnect during `generate` continues to terminal path.
- App routes:
  - `GET /health`
  - `GET /api/v1/models` in default mode
  - `POST /api/v1/responses` (non-stream + stream)
  - `POST /api/v1/chat/completions`
  - `/v1/*` path family returns `404` in default (non-OpenAI-compatible) mode.
  - In `ENABLE_OPENAI_COMPATIBLE_API=true` mode:
    - `GET /v1/models` works,
    - `/api/v1/*` paths return `404`.

## Next additions

- Add fixture-driven adapter edge cases (tools/function-calls) during migration.
- Extend stream snapshots with finer event assertions.
- Expand formal property-to-test mapping aligned with `formal/property-map.md`.
