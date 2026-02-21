# Trace Schema (xrouter)

| Event | Preconditions | Outcome | TLA+ Action |
|---|---|---|---|
| Start request | `kstate = idle` | `kstate -> ingest`, billing mode chosen | `Start` |
| Ingest success | `kstate = ingest` | `kstate -> tokenize` | `IngestOK` |
| Ingest failure | `kstate = ingest` | `kstate -> failed` | `IngestFail` |
| Tokenize success (billing on) | `kstate = tokenize`, `billingEnabled = TRUE` | `kstate -> hold` | `TokenizeOK` |
| Tokenize success (billing off) | `kstate = tokenize`, `billingEnabled = FALSE` | `kstate -> generate` | `TokenizeOK` |
| Tokenize failure | `kstate = tokenize` | `kstate -> failed` | `TokenizeFail` |
| Hold success | `kstate = hold` | `kstate -> generate`, hold marked acquired | `HoldOK` |
| Hold failure | `kstate = hold` | `kstate -> failed` | `HoldFail` |
| Generate streaming chunk | `kstate = generate` | `billableTokens += 1` | `GenerateChunk` |
| Generate done (billing on) | `kstate = generate`, `billingEnabled = TRUE`, hold acquired | `kstate -> finalize` | `GenerateDone` |
| Generate done (billing off) | `kstate = generate`, `billingEnabled = FALSE` | `kstate -> done`, response completed | `GenerateDone` |
| Generate failure (no billable tokens) | `kstate = generate`, no billable tokens | `kstate -> failed` | `GenerateFail` |
| Generate failure (billable tokens, billing on) | `kstate = generate`, billable tokens exist | `kstate -> finalize` (settlement path) | `GenerateFail` |
| Finalize success | `kstate = finalize`, hold acquired | `kstate -> done`, charge committed, hold released | `FinalizeOK` |
| Finalize failure | `kstate = finalize`, hold acquired | `kstate -> failed`, hold released, recovery obligation may be set | `FinalizeFail` |
| Client disconnect (early stage) | `kstate in {ingest, tokenize, hold}` | immediate `kstate -> failed`, connection closed | `ClientDisconnect` |
| Client disconnect (settlement stage) | `kstate in {generate, finalize}` | connection closed, pipeline remains active for post-paid settlement | `ClientDisconnect` |
| Recovery resolved (external settlement) | `kstate = failed`, recovery required | recovery obligation cleared; debt marked as externally settled | `RecoveryResolved` |
| Reset | `kstate in {done, failed}`, no recovery required | `kstate -> idle` | `Reset` |
