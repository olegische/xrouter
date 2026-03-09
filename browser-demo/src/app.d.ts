declare module './lib/wasm/xrouter_browser.js' {
  export interface WasmRunResult {
    request_id: string;
    text: string;
    output_tokens: number;
    reasoning?: string | null;
    emitted_live: boolean;
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

  export class WasmBrowserClient {
    constructor(
      provider: string,
      baseUrl?: string | null,
      apiKey?: string | null,
    );

    fetchModelIds(): Promise<string[]>;
    runDemoPromptStream(
      requestId: string,
      model: string,
      onEvent: (event: WasmResponseEvent) => void,
    ): Promise<WasmRunResult>;
    runTextStream(
      requestId: string,
      model: string,
      input: string,
      onEvent: (event: WasmResponseEvent) => void,
    ): Promise<WasmRunResult>;
    cancel(requestId: string): void;
  }

  export default function init(): Promise<void>;
}
