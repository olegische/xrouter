import initWasm, { WasmBrowserClient } from './wasm/xrouter_browser.js';

export type DemoProvider = 'deepseek';

export interface DemoClientConfig {
  provider: DemoProvider;
  apiKey: string;
  baseUrl?: string;
}

export type WasmResponseEvent =
  | { type: 'output_text_delta'; id: string; delta: string }
  | { type: 'reasoning_delta'; id: string; delta: string }
  | {
      type: 'response_completed';
      id: string;
      finish_reason: string;
      output: unknown[];
      usage: {
        input_tokens: number;
        output_tokens: number;
        total_tokens: number;
      };
    }
  | { type: 'response_error'; id: string; message: string };

export interface WasmRunResult {
  request_id: string;
  text: string;
  output_tokens: number;
  reasoning?: string | null;
  emitted_live: boolean;
}

let initPromise: Promise<unknown> | null = null;

async function ensureInit(): Promise<void> {
  if (!initPromise) {
    initPromise = initWasm();
  }
  await initPromise;
}

export async function createDemoClient(config: DemoClientConfig): Promise<WasmBrowserClient> {
  await ensureInit();
  return new WasmBrowserClient(
    config.provider,
    config.baseUrl ?? defaultBaseUrl(config.provider),
    config.apiKey,
  );
}

export function defaultBaseUrl(provider: DemoProvider): string {
  switch (provider) {
    case 'deepseek':
      return 'https://api.deepseek.com';
  }
}

export function createRequestId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `request-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
