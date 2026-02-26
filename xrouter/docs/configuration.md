# Configuration

Configuration is read from environment variables at app startup.

## Required

No required variables for local stub mode.

## Server

- `XR_HOST` (default: `127.0.0.1`)
- `XR_PORT` (default: `3000`)
- `ENABLE_OPENAI_COMPATIBLE_API` (default: `false`)
  - `false`: xrouter/openrouter-style access points (`/api/v1/...`)
  - `true`: OpenAI-compatible access points (`/v1/...`)

## Provider settings

For each provider prefix (`OPENROUTER`, `DEEPSEEK`, `GIGACHAT`, `YANDEX`, `OLLAMA`, `ZAI`, `XROUTER`):

- `<PREFIX>_ENABLED` (`true`/`false`, default: `true`)
- `<PREFIX>_API_KEY`
- `<PREFIX>_BASE_URL`

Example:

- `OPENROUTER_API_KEY`
- `OPENROUTER_BASE_URL`

## Generic OpenAI-compatible upstream via `XROUTER`

Use `XROUTER_*` when you want to connect any OpenAI-compatible provider through the generic
`xrouter` provider slot.

- `XROUTER_BASE_URL` (example: `https://<provider-host>/v1`)
- `XROUTER_API_KEY`

Example:

```bash
cd xrouter
XROUTER_BASE_URL=https://<provider-host>/v1 \
XROUTER_API_KEY=... \
cargo run -p xrouter-app
```

Model id format for requests:

- `xrouter/<upstream-model-id>`

Example request model:

- `xrouter/gpt-4o-mini`

## Local run

`xrouter-app` automatically loads `.env` from the workspace root via `dotenvy`.

```bash
cd xrouter
cp .env.example .env
just run
```

or inline:

```bash
OPENROUTER_API_KEY=... XR_PORT=8900 cargo run -p xrouter-app
```

## API docs

When server is running, OpenAPI and Swagger UI are available at:

- `http://<XR_HOST>:<XR_PORT>/openapi.json`
- `http://<XR_HOST>:<XR_PORT>/docs`
