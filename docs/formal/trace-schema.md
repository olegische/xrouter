# Trace Schema (Kernel State Machine)

| Event | Actor | Inputs | Preconditions | Observable Outcome | TLA+ Action |
|---|---|---|---|---|---|
| Submit act accepted | caller/runtime | incoming act | `kstate in {idle, emitted}` and input licensed | `kstate -> act_submitted`, active act initialized | `SubmitActLicensed` |
| Submit act rejected | caller/runtime | incoming act | `kstate in {idle, emitted}` and input not licensed | refusal `input_not_licensed` | `SubmitActRejected` |
| Commit observation | tool/env/human/external | observation event | `kstate in {act_submitted, observing}` and source != model | observation accepted, `kstate -> observing` | `CommitObservationSuccess` |
| Commit observation denied | model artifact | observation event | source is model | refusal `observation_not_admissible` | `CommitObservationModelRejected` |
| Escalation denied (missing grant) | caller/runtime | target modality without grant object | `kstate in {act_submitted, observing}` | refusal `missing_escalation_grant`, no state transition | `RequestEscalationDeniedMissingGrant` |
| Escalation request started | caller/runtime | target modality + grant payload | `kstate in {act_submitted, observing}` and active act | `kstate -> escalation_pending` | `RequestEscalationStart` |
| Escalation denied (invalid grant) | kernel | request + grant | `kstate = escalation_pending` and grant mismatch/overuse | refusal `invalid_escalation_grant`, state rollback | `EscalationDeniedInvalidGrant` |
| Escalation denied (policy) | kernel | request + grant | transition not allowed by modality table | refusal `illegal_modality_escalation`, state rollback | `EscalationDeniedPolicy` |
| Escalation denied (grounds) | kernel | request + grant + observations | min grounds or required source classes not satisfied | refusal `insufficient_grounds`, state rollback | `EscalationDeniedGrounds` |
| Escalation succeeds | kernel | request + grant + observations | all escalation checks pass | active modality updated, `kstate -> emission_pending` | `EscalationSuccess` |
| Prepare candidate | engine/caller | candidate modality + coverage flag | `kstate = emission_pending` | candidate envelope prepared for emission decision | `PrepareCandidate` |
| Emit denied (modality mismatch) | kernel | candidate act | candidate modality != active modality | refusal `emit_modality_mismatch` | `EmitDeniedModalityMismatch` |
| Emit denied (insufficient grounds) | kernel | candidate act | modality min grounds not met | refusal `insufficient_grounds` | `EmitDeniedGrounds` |
| Emit denied (claim coverage) | kernel | candidate act | assertive+ without claim/citation coverage | refusal `claim_ground_coverage_missing` | `EmitDeniedClaimCoverage` |
| Emit denied (output not licensed) | normative gate | candidate act + trace | output licensing denied | refusal `output_not_licensed` | `EmitDeniedOutputLicense` |
| Emit succeeds | kernel | candidate act + licensing | all checks pass | output emitted, `kstate -> emitted` | `EmitSuccess` |
| Execute turn facade (not implemented) | caller/runtime | turn request | `kstate in {idle, act_submitted, observing}` | refusal `runtime_not_implemented` (`ENOSYS`) | `ExecuteTurnNotImplemented` |
