# xrouter

Rust implementation of XRouter with OpenAI Responses API as the canonical external contract.

## What Is Included

- Core orchestration with an explicit pipeline: `ingest -> tokenize -> generate`
- Chat Completions adapter on top of the Responses flow
- Provider clients as separate Rust crates
- Browser/WASM-compatible router library in `xrouter-browser`
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
|  |  |- xrouter-browser    # browser/WASM composition root
|  |  |- xrouter-observability
|  |- docs/                 # Rust workspace documentation
|- browser-demo/            # optional Vite/Svelte demo harness for xrouter-browser
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

## Browser/WASM

XRouter now also has a browser/WASM-compatible router library:

- crate: `xrouter/crates/xrouter-browser`
- architecture: `xrouter/crates/xrouter-browser/ARCHITECTURE.md`
- plan/status: `xrouter/crates/xrouter-browser/PLAN.md`
- consumer usage: `xrouter/crates/xrouter-browser/README.md`

Current accepted browser-safe providers:

- `deepseek`
- `openai`
- `openrouter`
- `zai`

Currently excluded from the wasm slice:

- `yandex`
- `gigachat`

Optional browser demo harness:

```bash
just demo-install
just demo-dev
```

## Main Commands

From the repository root:

```bash
just fmt       # cargo fmt --all (inside xrouter/)
just check     # cargo check --workspace
just clippy    # cargo clippy --all-targets --all-features -- -D warnings
just test      # cargo test --all-features
just run       # run xrouter-app
just dev       # development script
just demo-dev  # run optional browser demo harness
```

Provider checks:

```bash
just models <provider>
just smoke <provider>
just smoke-stream <provider>
just smoke-byok <provider>
just smoke-byok-stream <provider>
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
- `XR_BYOK_ENABLED` (default: `false`)
- `<PROVIDER>_ENABLED`, `<PROVIDER>_BASE_URL`
- credentials:
  - most providers: `<PROVIDER>_API_KEY`
  - gigachat: `GIGACHAT_CREDENTIALS` (OAuth credentials)

`<PROVIDER>` should match one of the prefixes above (for example, `OPENROUTER`, `DEEPSEEK`, `GIGACHAT`).

Details: `xrouter/docs/configuration.md`.

### BYOK

`XR_BYOK_ENABLED=true` enables strict BYOK mode:

- request must include `Authorization: Bearer <token>`;
- router does not fallback to configured provider keys;
- `gigachat` expects a ready access token from client;
- `yandex` BYOK is not supported and returns `400`.

Smoke examples:

```bash
XR_BYOK_ENABLED=true BYOK_API_KEY=<token> just smoke-byok deepseek
XR_BYOK_ENABLED=true BYOK_API_KEY=<token> just smoke-byok-stream deepseek
XR_BYOK_ENABLED=true BYOK_API_KEY=<any> just smoke-byok yandex
```

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
- Browser/WASM architecture: `xrouter/crates/xrouter-browser/ARCHITECTURE.md`
- Browser/WASM usage: `xrouter/crates/xrouter-browser/README.md`
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
