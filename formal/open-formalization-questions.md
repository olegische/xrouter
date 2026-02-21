# Open Formalization Questions (xrouter)

Assumption:
- Billing mode is post-paid.

## Q1. Progress guarantee

Do we require that every started request eventually reaches `done` or `failed`?

Status: `OPEN`

Note:
- Current model includes weak fairness for generate/finalize/recovery actions.
- Open decision: are these fairness assumptions acceptable for production SLOs.

## Q2. Retry semantics

Should retries for `hold` and `finalize` be part of this state machine,
or modeled as external resilience wrappers?

Status: `OPEN`

## Q3. Cancellation semantics

How should client disconnect and upstream cancellation map into terminal states?

Status: `OPEN`

Note:
- Current model keeps settlement active after disconnect in `generate/finalize`.
- Open decision: do we require bounded settlement retries before setting recovery-required terminal failure?

## Q4. Fairness assumptions

Should fairness remain weak (`WF`) or be strengthened (`SF`) for:
- `generate` termination actions,
- `finalize` settlement actions,
- `recovery_resolved`?

Status: `OPEN`

## Q5. Scope upgrade

When should the model move from single-request FSM to:
- multi-request map,
- richer streaming emission state (chunk timing, ordering, backpressure),
- shared billing/rate-limit resources?

Status: `OPEN`

## Q6. Recovery obligation processing

When `chargeRecoveryRequired = TRUE` on terminal failure, should recovery be:
- in-model explicit action,
- or an external guaranteed processor with separate formal model?

Status: `OPEN`
