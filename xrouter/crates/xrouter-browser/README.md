# WASM Usage

This document describes the current consumer-facing browser/WASM contract for XRouter.

Scope:

1. this is a library/runtime contract
2. this is not a UI contract
3. consumers are expected to provide their own host or browser UI

## Primary Crate

Use:

- `xrouter/crates/xrouter-browser`

This crate is the browser/WASM composition root for XRouter.

It provides:

1. browser model discovery
2. browser-safe provider runtime
3. streamed inference
4. request-scoped cancellation
5. wasm exports for JS consumers

## Current Supported Provider

Current browser-supported providers:

1. `deepseek`
2. `openai`
3. `openrouter`
4. `zai`

These providers were manually validated in the browser for:

1. model discovery
2. streamed inference
3. request-scoped cancellation

Currently unsupported in wasm:

1. `yandex`
   requires extra project/folder configuration beyond simple API key BYOK
2. `gigachat`
   requires an additional OAuth/token flow that is intentionally excluded for now

## Rust API

Main Rust-facing types:

1. `BrowserModelDiscoveryClient`
2. `BrowserProviderRuntime`
3. `BrowserInferenceClient`
4. `BrowserProvider`
5. `BrowserError`

Current browser inference contract:

1. consumer supplies `request_id`
2. consumer supplies provider, base URL, API key, model, and input
3. stream events are emitted through `ResponseEventSink`
4. consumer may call `cancel(request_id)` to abort the active browser request

## WASM API

Current wasm-facing export:

1. `WasmBrowserClient`

Current methods:

1. `fetchModelIds()`
2. `runTextStream(requestId, model, input, onEvent)`
3. `runResponsesStream(requestId, request, onEvent)`
4. `runDemoPromptStream(requestId, model, onEvent)`
5. `cancel(requestId)`

Current event callback payloads are serialized `ResponseEvent` values from `xrouter-contracts`.

`runResponsesStream(...)` is the canonical browser entrypoint for OpenAI Responses-compatible
agent flows. It accepts a serialized `ResponsesRequest`, including:

1. top-level `instructions`
2. optional `previous_response_id`
1. structured `input`
3. `tools`
4. `tool_choice`
5. optional `parallel_tool_calls`
6. optional `reasoning`
7. optional `store`, `include`, `service_tier`, `prompt_cache_key`, and `text`

Accepted `ResponsesRequest.input` forms:

1. plain input string
2. item array containing `message` items with `role` and either string `content` or structured
   content-part arrays
3. item array containing `function_call` items with `call_id`, `name`, and string `arguments`
4. item array containing `function_call_output` items with `call_id` and `output` encoded as:
   - plain string
   - structured content-part array
   - JSON object/array payload
5. item array containing Codex/OpenAI-style history items such as `reasoning`,
   `custom_tool_call_output`, `mcp_tool_call_output`, and `tool_search_output`

Round-trippable browser contract guarantees:

1. `response_completed.output` may emit `message`, `reasoning`, and `function_call` items
2. emitted `function_call` items can be sent back in follow-up `input` with the same `call_id`,
   `name`, and `arguments`
3. follow-up `function_call_output.output` preserves structured payloads on the browser contract;
   browser hosts do not need to flatten them before sending the next turn
4. `message.content` remains structured on the browser contract when callers supply content-part
   arrays
5. downstream provider adapters may still serialize rich tool output into string form when the
   upstream provider only supports string tool messages
6. top-level `instructions` remain separate from `input` on the browser contract and are not
   expected to be flattened by the host

Event behavior:

1. live text and reasoning deltas are forwarded as `ResponseEvent`
2. terminal completion is always emitted as `ResponseCompleted`
3. tool-calling responses are surfaced through `ResponseCompleted.output` function-call items
4. provider/request failures are surfaced as `ResponseError`

## Cancellation Semantics

Cancellation is a first-class browser capability.

Current behavior:

1. active requests are keyed by `request_id`
2. browser transport uses `AbortController`
3. `cancel(request_id)` is idempotent
4. cancellation stops further stream deltas for that request
5. cancellation is surfaced as a request-level cancellation outcome/error

What cancellation is not:

1. it is not a UI-only unsubscribe
2. it is not just dropping callbacks while the provider request keeps running

## Out of Scope

These items are intentionally outside the XRouter wasm library contract:

1. any specific host protocol for downstream applications
2. any bundled browser UI requirement
3. multi-provider parity beyond the first accepted browser-safe provider path

## Packaging

Current recommended packaging strategy:

1. consume `xrouter-browser` as a source crate
2. build it with `wasm-pack`
3. consume the generated wasm/js package from the host application

This keeps the contract neutral for downstream browser hosts.

For consumers that want a ready-built browser package, use the rolling release asset:

1. release page:
   `https://github.com/olegische/xrouter/releases/tag/xrouter-browser-main`
2. direct package download:
   `https://github.com/olegische/xrouter/releases/download/xrouter-browser-main/xrouter-browser-main.tar.gz`
3. checksum:
   `https://github.com/olegische/xrouter/releases/download/xrouter-browser-main/xrouter-browser-main.tar.gz.sha256`

Do not use:

1. `Source code (zip)`
2. `Source code (tar.gz)`

Those GitHub-generated source archives contain repository sources, not the built browser/WASM package.

After unpacking `xrouter-browser-main.tar.gz`, the host application should serve the unpacked
`xrouter-browser/` directory as static assets and import the generated JS glue module. The
generated `xrouter_browser.js` and `xrouter_browser_bg.wasm` files must stay together.

Minimal browser runtime flow:

```ts
import initXrouterBrowser, { WasmBrowserClient } from "/assets/xrouter-browser/xrouter_browser.js";

await initXrouterBrowser();

const client = new WasmBrowserClient(
  "deepseek",
  "https://api.deepseek.com",
  apiKey,
);

const modelIds = await client.fetchModelIds();
```

Runtime notes:

1. initialize the wasm module before calling `new WasmBrowserClient(...)`
2. load it as an ESM module in the browser
3. if the page already hosts another `wasm-bindgen` runtime, align `wasm-bindgen`,
   `wasm-bindgen-futures`, `js-sys`, and `web-sys` versions across modules
