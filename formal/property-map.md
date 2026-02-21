# Property Map (xrouter)

| Property ID | Type | Statement | Source | Status |
|---|---|---|---|---|
| P-XR-001 | invariant | Variables stay in declared domains (`TypeInv`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-002 | safety | `hold`/`finalize` are billing-gated and generation with billing requires acquired hold (`BillingGateInv`). | Flow rule: `ingest -> tokenize -> hold? -> generate -> finalize?` | REQUIRED |
| P-XR-003 | safety | Hold lifecycle is leak-safe: terminal states cannot keep acquired hold (`HoldLifecycleInv`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-004 | safety | Post-paid success rule: if billing is enabled and billable tokens exist, `done` requires committed charge (`BillingGateInv`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-005 | safety | No free-token terminal failure: with billable tokens under billing, terminal `failed` requires commit, explicit recovery obligation, or explicit external recovery settlement marker (`NoFreeTokensInv`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-006 | safety | Streaming is first-class: billable token chunks can occur before terminal state (`StreamingInv`, `GenerateChunk`). | `formal/xrouter.tla` | REQUIRED |
| P-XR-007 | safety | Client disconnect semantics: early stages require active connection; disconnected flow may continue only in generate/finalize for settlement (`DisconnectSafetyInv`). | `formal/xrouter.tla` (`ClientDisconnect`) | REQUIRED |
| P-XR-008 | safety | Reset cannot drop unresolved debt: reset is blocked while `chargeRecoveryRequired = TRUE`. | `formal/xrouter.tla` (`Reset`, `RecoveryResolved`) | REQUIRED |
| P-XR-009 | liveness | Settlement progress is fairness-constrained for `generate`, `finalize`, and `recovery_resolved_external`. | `formal/xrouter.tla` (`Spec`) | REQUIRED |
| P-XR-010 | liveness | Debt-progress temporal property: billed token generation under billing leads to either charge commit, recovery-required state, or explicit external recovery settlement (`DebtProgressLiveness`). | `formal/xrouter.tla`, `formal/xrouter.cfg` | REQUIRED |

## Model Status

`SCOPED`

Reason:
- Core safety invariants are modeled and checked.
- Bounded liveness/progress is checked in scoped form (`DebtProgressLiveness`), but the model is still single-request.
