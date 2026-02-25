# Trace Schema (xrouter)

| Event | Preconditions | Outcome | TLA+ Action |
|---|---|---|---|
| Start request | `kstate = idle` | `kstate -> ingest`, client connected | `Start` |
| Ingest success | `kstate = ingest` | `kstate -> tokenize` | `IngestOK` |
| Ingest failure | `kstate = ingest` | `kstate -> failed` | `IngestFail` |
| Tokenize success | `kstate = tokenize` | `kstate -> generate` | `TokenizeOK` |
| Tokenize failure | `kstate = tokenize` | `kstate -> failed` | `TokenizeFail` |
| Generate streaming chunk | `kstate = generate` | `outputTokens += 1` | `GenerateChunk` |
| Generate done | `kstate = generate` | `kstate -> done`, response completed | `GenerateDone` |
| Generate failure | `kstate = generate` | `kstate -> failed` | `GenerateFail` |
| Client disconnect (early stage) | `kstate in {ingest, tokenize}` | immediate `kstate -> failed`, connection closed | `ClientDisconnect` |
| Client disconnect (generate stage) | `kstate = generate` | connection closed, generation may continue | `ClientDisconnect` |
| Reset | `kstate in {done, failed}` | `kstate -> idle`, counters reset | `Reset` |
