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

## Observability

- `RUST_LOG` (optional override for filtering)
- `XR_LOG_LEVEL` (default: `info`)
- `XR_LOG_SPAN_EVENTS` (default: `false`)
- `XR_TRACE_ENABLED` (default: `false`)
- `XR_OTEL_TRACE_EXPORTER` (default: `otlp_grpc`, options: `otlp_grpc`, `otlp_http`)
- `XR_OTEL_TRACE_ENDPOINT`
  - default for `otlp_grpc`: `http://127.0.0.1:4317`
  - default for `otlp_http`: `http://127.0.0.1:4318/v1/traces`
- `XR_OTEL_TRACE_TIMEOUT_MS` (default: `3000`)
- `XR_OTEL_TRACE_HTTP_PROTOCOL` (for HTTP exporter, default: `binary`, options: `binary`, `json`)
- `XR_ENVIRONMENT` (default: `dev`, emitted as OTEL resource attribute)

When `XR_TRACE_ENABLED=true`, xrouter enables OpenTelemetry-compatible tracing layers, creates a
global SDK tracer provider, and installs W3C trace-context propagation for inbound/outbound
requests.

Startup preflight (soft mode):

- xrouter checks reachability of `XR_OTEL_TRACE_ENDPOINT` at startup.
- If endpoint is reachable, an info event is logged.
- If endpoint is unreachable, a warning is logged and xrouter continues running (no fail-fast).

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
