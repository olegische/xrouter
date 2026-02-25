# Property Map (xrouter)

| Property ID | Type | Statement | Source | Status |
|---|---|---|---|---|
| P-XR-001 | invariant | Variables stay in declared domains (`TypeInv`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-002 | safety | Canonical non-billing flow only: `ingest -> tokenize -> generate`. | `formal/xrouter.tla` (`TokenizeOK`) | REQUIRED |
| P-XR-003 | safety | Completion flag is terminal-safe: `responseCompleted => kstate = done` (`FlowInv`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-004 | safety | Streaming remains first-class: token chunks can be emitted before terminal state (`StreamingInv`, `GenerateChunk`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-005 | safety | Client disconnect semantics: early stages fail fast; generate may continue after disconnect (`DisconnectSafetyInv`, `ClientDisconnect`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-006 | liveness | Generation eventually reaches terminal outcome (`GenerateProgressLiveness`). | `formal/xrouter.tla`, `formal/xrouter.cfg` | REQUIRED |

## Model Status

`SCOPED`

Reason:
- Core non-billing lifecycle and disconnect semantics are modeled and checked.
- Model is intentionally single-request and excludes settlement semantics.
