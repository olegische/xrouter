# xrouter

Rust implementation of XRouter with OpenAI Responses API as the canonical external contract.

## What Is Included

- Core orchestration with an explicit pipeline: `ingest -> tokenize -> generate`
- Chat Completions adapter on top of the Responses flow
- Provider clients as separate Rust crates
- Formal lifecycle model in TLA+

## Repository Structure

```text
.
|- xrouter/                 # Rust workspace
|  |- crates/
|  |  |- xrouter-app        # HTTP entrypoint + composition root
|  |  |- xrouter-core       # orchestration/use-cases
|  |  |- xrouter-contracts  # canonical DTOs/contracts
|  |  |- xrouter-clients-openai
|  |  |- xrouter-observability
|  |- docs/                 # Rust workspace documentation
|- formal/                  # formal model and property mapping
|- docs/                    # project playbooks
```

## Quick Start

Requirements:

- Rust toolchain (stable)
- `just` (recommended command entrypoint)

Run:

```bash
cd xrouter
cp .env.example .env
just run
```

By default, the server starts on `127.0.0.1:3000`.

## Main Commands

From the repository root:

```bash
just fmt       # cargo fmt --all (inside xrouter/)
just check     # cargo check --workspace
just clippy    # cargo clippy --all-targets --all-features -- -D warnings
just test      # cargo test --all-features
just run       # run xrouter-app
just dev       # development script
```

Provider checks:

```bash
just models <provider>
just smoke <provider>
just smoke-stream <provider>
```

## Supported Providers

- `openrouter`
- `deepseek`
- `gigachat`
- `yandex`
- `ollama`
- `zai`
- `xrouter`

## Configuration

Key environment variables:

- `XR_HOST` (default: `127.0.0.1`)
- `XR_PORT` (default: `3000`)
- `ENABLE_OPENAI_COMPATIBLE_API` (default: `false`)
- `<PROVIDER>_ENABLED`, `<PROVIDER>_API_KEY`, `<PROVIDER>_BASE_URL`

`<PROVIDER>` should match one of the prefixes above (for example, `OPENROUTER`, `DEEPSEEK`, `GIGACHAT`).

Details: `xrouter/docs/configuration.md`.

## API Modes

- `ENABLE_OPENAI_COMPATIBLE_API=false`:
  - `GET /api/v1/models`
  - `POST /api/v1/responses`
  - `POST /api/v1/chat/completions`
- `ENABLE_OPENAI_COMPATIBLE_API=true`:
  - `GET /v1/models`
  - `POST /v1/responses`
  - `POST /v1/chat/completions`

Swagger/OpenAPI:

- `/openapi.json`
- `/docs`

## Documentation

- Architecture: `ARCHITECTURE.md`
- Rust workspace architecture: `xrouter/docs/architecture.md`
- Testing strategy: `xrouter/docs/testing.md`
- Formal model: `formal/xrouter.tla`
- Property-to-test mapping: `formal/property-map.md`

## Change Quality Checklist

For any Rust code change, the minimum cycle is:

1. `just fmt`
2. `cargo test -p <changed-crate>`
3. For shared/core areas: `cargo test --all-features`
4. `cargo clippy --all-targets --all-features -- -D warnings`

Detailed rules: `AGENTS.md`.
