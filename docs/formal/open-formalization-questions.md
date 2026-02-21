# Open Formalization Questions (Kernel State Machine)

## Q1. Liveness obligations

What progress guarantees are mandatory for kernel flow?

Examples:
- Must every admitted act eventually end in `emitted` or refusal?
- Under what fairness assumptions for environment/tool actions?

Status: `BLOCKED`  
Reason: explicit progress requirements are not yet grounded in accepted FR/AT artifacts.

## Q2. Fairness model

Which actions are weak/strong fair?

Candidates:
- observation commits (`CommitObservationSuccess`)
- escalation requests with explicit grant payload (`RequestEscalationStart`)
- emission attempt (`EmitSuccess`)

Status: `OPEN`

## Q3. Session lifecycle in formal model

Should unknown session / session teardown be first-class in this state machine model,
or remain out of scope for this module and modeled separately?

Status: `OPEN`

## Q4. Claim coverage abstraction

Current model abstracts claim coverage as boolean state (`claimCoverageOK`).
Do we need a stricter structural model of `claims[] -> ground keys[]` as state variables
in the next formal iteration?

Status: `OPEN`
