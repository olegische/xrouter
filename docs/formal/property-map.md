# Property Map (Kernel State Machine)

| Property ID | Type | Statement | Source | Status |
|---|---|---|---|---|
| P-KSM-001 | invariant | Kernel variables remain in declared domains (`TypeInv`). | Observation: `src/acer_runtime/types.py`, `src/acer_runtime/runtime.py` | REQUIRED |
| P-KSM-002 | safety | Active act consistency by state (`idle/emitted => no active act`, transitional states => active act). | Observation: `src/acer_runtime/runtime.py` | REQUIRED |
| P-KSM-003 | safety | Observation grounds never include `model` source kind. | Observation: `src/acer_runtime/runtime.py` (`commit_observation` reject model source) | REQUIRED |
| P-KSM-004 | safety | Escalation and emit gates enforce modality-minimum grounds (stage-local checks, not global state invariant). | Observation: `src/acer_runtime/abi.py` (`MIN_GROUNDS_BY_MODALITY`), `src/acer_runtime/runtime.py` | REQUIRED |
| P-KSM-005 | safety | Escalation success implies allowed modality transition (`CanTransition`). | Observation: `src/acer_runtime/abi.py` (`MODALITY_TRANSITION_TABLE`), `src/acer_runtime/runtime.py` | REQUIRED |
| P-KSM-006 | safety | Escalation grant use never exceeds max uses. | Observation: `src/acer_runtime/runtime.py` (`grant` checks) | REQUIRED |
| P-KSM-007 | safety | Escalation rollback may only return to `{act_submitted, observing}` (`PrevStateRollbackDomainInv`). | Observation: `src/acer_runtime/runtime.py` (request escalation rollback path) | REQUIRED |
| P-KSM-008 | safety | No implicit modality escalation: modality upgrade can exist only in `emission_pending` with active act (`NoImplicitEscalationInv`). | Observation: `src/acer_runtime/runtime.py` (`request_escalation` is explicit syscall) | REQUIRED |
| P-KSM-009 | safety | Refusal reasons keep deterministic errno mapping (`RefusalErrnoConsistencyInv`). | Observation: `src/acer_runtime/types.py`, `src/acer_runtime/runtime.py` | REQUIRED |
| P-KSM-010 | liveness | Every submitted act eventually reaches `emitted` or terminal refusal. | Missing explicit progress obligations in accepted FR/AT | BLOCKED |
| P-KSM-011 | liveness | Repeated admissible observations eventually enable escalation when grant prerequisites are met. | Missing explicit fairness/progress obligations in accepted FR/AT | OPEN |

## Model Status

`PARTIAL`

Reason:
- Required safety/invariant properties are modeled.
- Liveness properties remain `OPEN/BLOCKED` due missing explicit progress sources.
