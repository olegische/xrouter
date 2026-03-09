<script lang="ts">
  import {
    createDemoClient,
    createRequestId,
    defaultBaseUrl,
    type DemoProvider,
    type WasmResponseEvent,
  } from './lib/wasm-client';

  type LoadState = 'idle' | 'loading' | 'ready' | 'running' | 'error';

  const providerOptions: Array<{ value: DemoProvider; label: string }> = [
    { value: 'deepseek', label: 'DeepSeek' },
    { value: 'openai', label: 'OpenAI' },
    { value: 'openrouter', label: 'OpenRouter' },
    { value: 'zai', label: 'ZAI' },
  ];

  let provider: DemoProvider = 'deepseek';
  let apiKey = '';
  let baseUrl = defaultBaseUrl(provider);
  let models: string[] = [];
  let selectedModel = '';
  let prompt = 'Hello, what can you do?';
  let streamText = '';
  let reasoningText = '';
  let status: LoadState = 'idle';
  let errorMessage = '';
  let usageSummary = '';
  let currentRequestId = '';
  let activeClient: Awaited<ReturnType<typeof createDemoClient>> | null = null;

  function resetStreamState(): void {
    streamText = '';
    reasoningText = '';
    usageSummary = '';
    errorMessage = '';
  }

  function onProviderChange(nextProvider: DemoProvider): void {
    provider = nextProvider;
    baseUrl = defaultBaseUrl(provider);
    apiKey = '';
    models = [];
    selectedModel = '';
    resetStreamState();
    status = 'idle';
  }

  async function loadModels(): Promise<void> {
    if (!apiKey.trim()) {
      status = 'error';
      errorMessage = 'API key is required before loading models.';
      return;
    }

    status = 'loading';
    errorMessage = '';
    models = [];
    selectedModel = '';
    resetStreamState();

    try {
      const client = await createDemoClient({
        provider,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim() || undefined,
      });
      activeClient = client;
      models = (await client.fetchModelIds())
        .slice()
        .sort((left: string, right: string) => left.localeCompare(right));
      selectedModel = models[0] ?? '';
      status = 'ready';
      if (models.length === 0) {
        errorMessage = 'Provider returned an empty model list.';
      }
    } catch (error) {
      status = 'error';
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  function handleEvent(event: WasmResponseEvent): void {
    if (event.type === 'output_text_delta') {
      streamText += event.delta;
      return;
    }

    if (event.type === 'reasoning_delta') {
      reasoningText += event.delta;
      return;
    }

    if (event.type === 'response_completed') {
      usageSummary = `${event.usage.output_tokens} output tokens`;
      return;
    }

    if (event.type === 'response_error') {
      errorMessage = event.message;
      status = 'error';
    }
  }

  async function runPrompt(): Promise<void> {
    if (!selectedModel) {
      status = 'error';
      errorMessage = 'Select a model before starting the stream.';
      return;
    }

    status = 'running';
    resetStreamState();
    currentRequestId = createRequestId();

    try {
      const client = await createDemoClient({
        provider,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim() || undefined,
      });
      activeClient = client;
      const result = await client.runTextStream(currentRequestId, selectedModel, prompt, handleEvent);
      if (!streamText) {
        streamText = result.text;
      }
      if (!usageSummary) {
        usageSummary = `${result.output_tokens} output tokens`;
      }
      if (!reasoningText && result.reasoning) {
        reasoningText = result.reasoning;
      }
      status = 'ready';
      currentRequestId = '';
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes('canceled')) {
        status = 'ready';
        errorMessage = 'Request canceled.';
      } else {
        status = 'error';
        errorMessage = message;
      }
      currentRequestId = '';
    }
  }

  function cancelPrompt(): void {
    if (status !== 'running' || !currentRequestId || activeClient === null) {
      return;
    }

    activeClient.cancel(currentRequestId);
  }
</script>

<svelte:head>
  <title>XRouter Browser Demo</title>
  <meta
    name="description"
    content="BYOK browser demo for XRouter with provider model discovery and live stream output."
  />
</svelte:head>

<div class="shell">
  <section class="hero">
    <p class="eyebrow">XRouter / Browser</p>
    <h1>Bring your key, fetch the provider models, stream the answer in-browser.</h1>
    <p class="lede">
      This demo runs the router path in the browser. No server layer. The first vertical slice is
      wired for BYOK providers: DeepSeek, OpenAI, OpenRouter, and ZAI. Yandex and GigaChat are
      intentionally excluded from the wasm slice for now.
    </p>
  </section>

  <section class="grid">
    <div class="panel controls">
      <div class="panel-head">
        <h2>Session</h2>
        <span class:live={status === 'running'}>
          {status === 'running' ? 'streaming' : status}
        </span>
      </div>

      <label>
        <span>Provider</span>
        <select bind:value={provider} on:change={(event) => onProviderChange((event.currentTarget as HTMLSelectElement).value as DemoProvider)}>
          {#each providerOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>

      <label>
        <span>Base URL</span>
        <input bind:value={baseUrl} spellcheck="false" />
      </label>

      <label>
        <span>API Key</span>
        <input bind:value={apiKey} type="password" placeholder="sk-..." spellcheck="false" />
      </label>

      <div class="actions">
        <button class="primary" on:click={loadModels} disabled={status === 'loading' || status === 'running'}>
          {status === 'loading' ? 'Loading models...' : 'Load Models'}
        </button>
      </div>

      <label>
        <span>Model</span>
        <select bind:value={selectedModel} disabled={models.length === 0 || status === 'loading'}>
          <option value="" disabled selected={selectedModel === ''}>Select a model</option>
          {#each models as model}
            <option value={model}>{model}</option>
          {/each}
        </select>
      </label>

      <label>
        <span>Prompt</span>
        <textarea bind:value={prompt} rows="4"></textarea>
      </label>

      <div class="actions">
        <button class="primary" on:click={runPrompt} disabled={status === 'loading' || status === 'running' || !selectedModel}>
          {status === 'running' ? 'Streaming...' : 'Send Prompt'}
        </button>
        <button class="secondary" on:click={cancelPrompt} disabled={status !== 'running'}>
          Cancel
        </button>
      </div>

      {#if errorMessage}
        <p class="error">{errorMessage}</p>
      {/if}
    </div>

    <div class="panel models">
      <div class="panel-head">
        <h2>Models</h2>
        <span>{models.length} loaded</span>
      </div>

      {#if models.length > 0}
        <ul>
          {#each models as model}
            <li class:selected={model === selectedModel}>
              <button type="button" on:click={() => (selectedModel = model)}>{model}</button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="placeholder">Load models to populate the provider catalog.</p>
      {/if}
    </div>

    <div class="panel stream">
      <div class="panel-head">
        <h2>Live Stream</h2>
        <span>{usageSummary || 'waiting for output'}</span>
      </div>

      <pre>{streamText || 'The first stream delta will appear here.'}</pre>

      {#if reasoningText}
        <div class="reasoning">
          <h3>Reasoning</h3>
          <p>{reasoningText}</p>
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  .shell {
    width: min(1180px, calc(100vw - 2rem));
    margin: 0 auto;
    padding: 2.4rem 0 4rem;
  }

  .hero {
    padding: 1rem 0 2rem;
  }

  .eyebrow {
    margin: 0 0 0.6rem;
    color: var(--accent-cool);
    text-transform: uppercase;
    letter-spacing: 0.18em;
    font-size: 0.72rem;
  }

  h1 {
    margin: 0;
    max-width: 12ch;
    font-family: "Iowan Old Style", "Georgia", serif;
    font-size: clamp(2.7rem, 5vw, 5rem);
    line-height: 0.96;
  }

  .lede {
    max-width: 62ch;
    margin: 1rem 0 0;
    color: var(--text-soft);
    font-size: 1.03rem;
    line-height: 1.55;
  }

  .grid {
    display: grid;
    gap: 1rem;
    grid-template-columns: minmax(0, 0.95fr) minmax(0, 0.8fr) minmax(0, 1.25fr);
  }

  .panel {
    background: linear-gradient(180deg, rgba(13, 19, 31, 0.88), var(--panel));
    border: 1px solid var(--line);
    border-radius: 1.4rem;
    padding: 1.15rem;
    box-shadow: 0 18px 50px rgba(3, 5, 10, 0.28);
    backdrop-filter: blur(18px);
  }

  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .panel-head h2,
  .reasoning h3 {
    margin: 0;
    font-size: 0.92rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--text-faint);
  }

  .panel-head span {
    color: var(--text-faint);
    font-size: 0.82rem;
  }

  .panel-head span.live {
    color: var(--accent);
  }

  .controls {
    display: grid;
    gap: 0.95rem;
  }

  label {
    display: grid;
    gap: 0.45rem;
  }

  label span {
    color: var(--text-soft);
    font-size: 0.82rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  input,
  select,
  textarea {
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.09);
    background: var(--panel-strong);
    border-radius: 0.95rem;
    padding: 0.9rem 1rem;
    color: inherit;
  }

  textarea {
    resize: vertical;
    min-height: 8rem;
  }

  .actions {
    display: flex;
    gap: 0.75rem;
  }

  button {
    cursor: pointer;
  }

  .primary {
    border: 0;
    border-radius: 999px;
    padding: 0.92rem 1.25rem;
    background: linear-gradient(135deg, var(--accent), #f3ba59);
    color: #1d1303;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .primary:disabled {
    cursor: wait;
    opacity: 0.6;
  }

  .secondary {
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 999px;
    padding: 0.92rem 1.25rem;
    background: rgba(255, 255, 255, 0.04);
    color: inherit;
  }

  .secondary:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .error {
    margin: 0;
    color: var(--danger);
    line-height: 1.45;
  }

  .models ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.55rem;
    max-height: 28rem;
    overflow: auto;
  }

  .models li button {
    width: 100%;
    text-align: left;
    border: 1px solid transparent;
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.04);
    color: inherit;
    padding: 0.85rem 0.95rem;
  }

  .models li.selected button {
    border-color: rgba(240, 153, 73, 0.55);
    background: rgba(240, 153, 73, 0.11);
  }

  .placeholder {
    margin: 0;
    color: var(--text-faint);
  }

  .stream pre {
    margin: 0;
    min-height: 22rem;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: "SFMono-Regular", "Menlo", monospace;
    font-size: 0.96rem;
    line-height: 1.6;
    color: #fff8ef;
  }

  .reasoning {
    margin-top: 1.2rem;
    border-top: 1px solid var(--line);
    padding-top: 1rem;
  }

  .reasoning p {
    margin: 0.45rem 0 0;
    color: var(--text-soft);
    line-height: 1.55;
  }

  @media (max-width: 980px) {
    .grid {
      grid-template-columns: 1fr;
    }

    .shell {
      width: min(100vw - 1rem, 52rem);
      padding-top: 1rem;
    }

    h1 {
      max-width: 14ch;
    }
  }
</style>
