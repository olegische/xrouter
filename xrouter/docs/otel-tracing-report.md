# xrouter Tracing and OpenTelemetry Report

This document reflects the actual tracing implementation in the Rust workspace (`xrouter/`).

Scope:

- request lifecycle spans
- provider request/stream spans
- event classification (`otel.name`)
- trace context propagation (inbound/outbound)
- runtime OTEL/tracing bootstrap

## Executive summary

xrouter now has an end-to-end tracing chain:

1. HTTP request root span in `xrouter-app`
2. stage and provider spans in `xrouter-core`
3. provider HTTP/stream spans in `xrouter-clients-openai`
4. OTEL SDK tracer + propagator wiring in `xrouter-observability`
5. trace context extraction from inbound headers and injection to outbound provider calls

## 1. Observability bootstrap (`xrouter-observability`)

Source: `crates/xrouter-observability/src/lib.rs`

Behavior:

- `XR_TRACE_ENABLED=true` enables OTEL tracing layer.
- Tracer provider is real `SdkTracerProvider` (not noop).
- OTLP trace exporter is configured from env:
  - `XR_OTEL_TRACE_EXPORTER` (`otlp_grpc` / `otlp_http`)
  - `XR_OTEL_TRACE_ENDPOINT`
  - `XR_OTEL_TRACE_TIMEOUT_MS`
  - `XR_OTEL_TRACE_HTTP_PROTOCOL` (HTTP mode)
- Global W3C propagator is installed:
  - `TraceContextPropagator`
- Startup preflight checks reachability of `XR_OTEL_TRACE_ENDPOINT`:
  - logs `observability.trace.preflight.ok` when reachable
  - logs `observability.trace.preflight.failed` when unreachable (soft mode, no crash)

Resource attributes include:

- `service.name`
- `service.version`
- `deployment.environment`

## 2. Root request span and inbound propagation (`xrouter-app`)

Source: `crates/xrouter-app/src/lib.rs`

For:

- `POST /api/v1/responses`
- `POST /api/v1/chat/completions`

xrouter creates a root span:

- `http.request`
- attributes:
  - `otel.name=http.request`
  - `route`
  - `model`
  - `provider`
  - `stream`

Incoming headers are parsed with OTEL propagator and attached as parent context.
This makes xrouter continue upstream traces when `traceparent` is provided.

## 3. Core lifecycle and provider spans (`xrouter-core`)

Source: `crates/xrouter-core/src/lib.rs`

Existing stage span:

- `pipeline_stage`
- attributes include `request_id`, `stage`, `model`

Provider execution span:

- `provider_generate`
- attributes:
  - `otel.name=provider_generate`
  - `request_id`
  - `provider_model`
  - `output_tokens`
  - `chunk_count`

## 4. Outbound provider tracing and context injection (`xrouter-clients-openai`)

Source: `crates/xrouter-clients-openai/src/lib.rs`

HTTP span:

- `provider_http_request`
- attributes:
  - `otel.name=provider_http_request`
  - `provider`
  - `http.method`
  - `http.url`
  - `http.retry_count`
  - `http.response.status_code`

Stream request span:

- `provider_stream_request`
- attributes:
  - `otel.name=provider_stream_request`
  - `provider`
  - `request_id`
  - `stream_kind` (`chat_completions` / `responses`)

Outbound propagation:

- current span context is injected into provider HTTP headers through global propagator
- `traceparent`/`tracestate` are included when context exists

## 5. Event classification spans (`otel.name`)

Source: `crates/xrouter-app/src/lib.rs`

During stream mapping, each `ResponseEvent` is classified via span:

- span name: `handle_responses`
- attributes:
  - `otel.name`
  - `from` (`responses_sse` / `chat_completions_sse`)
  - `route`
  - `provider`
  - `tool_name` (set for completed events with function call output)

Current `otel.name` mapping:

- `ResponseEvent::OutputTextDelta` -> `output_text_delta`
- `ResponseEvent::ReasoningDelta` -> `reasoning_delta`
- `ResponseEvent::ResponseCompleted` -> `completed`
- `ResponseEvent::ResponseError` -> `error`

## 6. Tests

Source: `crates/xrouter-clients-openai/src/lib.rs` (`#[cfg(test)]`)

Coverage includes:

- trace header injection uses current span context (happy path)
- trace header injection is safe without active span (failure/edge path)

## 7. Notes and boundaries

- This report documents trace/propagation behavior, not billing semantics.
- Stage model remains canonical: `ingest -> tokenize -> generate`.
- No secrets are emitted in tracing attributes by design.
