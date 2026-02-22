# Configuration

Configuration is read from environment variables at app startup.

## Required

No required variables for local stub mode.

## Server

- `XR_HOST` (default: `127.0.0.1`)
- `XR_PORT` (default: `3000`)
- `XR_BILLING_ENABLED` (default: `false`)
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

## Local run

`xrouter-app` automatically loads `.env` from the workspace root via `dotenvy`.

```bash
cd /Users/olegromanchuk/Projects/xrouter/xrouter
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
