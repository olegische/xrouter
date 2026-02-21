# Configuration

Configuration is read from environment variables at app startup.

## Required

No required variables for local stub mode.

## Server

- `XR_HOST` (default: `127.0.0.1`)
- `XR_PORT` (default: `3000`)
- `XR_BILLING_ENABLED` (default: `false`)

## Provider settings

For each provider prefix (`OPENAI`, `OPENROUTER`, `DEEPSEEK`, `GIGACHAT`, `YANDEX`, `OLLAMA`, `ZAI`, `AGENTS`, `XROUTER`):

- `<PREFIX>_ENABLED` (`true`/`false`, default: `true`)
- `<PREFIX>_API_KEY`
- `<PREFIX>_BASE_URL`

Example:

- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`

## Local run

`xrouter-app` automatically loads `.env` from the workspace root via `dotenvy`.

```bash
cd /Users/olegromanchuk/Projects/xrouter/xrouter
cp .env.example .env
just run
```

or inline:

```bash
OPENAI_API_KEY=sk-... XR_PORT=3000 cargo run -p xrouter-app
```
