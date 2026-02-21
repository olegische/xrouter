---- MODULE kernel_state_machine ----
EXTENDS Naturals, Sequences, TLC

\* PARTIAL MODEL:
\* This model focuses on safety semantics of the ACER kernel state machine.
\* Liveness properties are intentionally omitted pending explicit FR/AT progress obligations.

CONSTANTS MaxObs, MaxGrantUses

KernelStates ==
  {"idle", "act_submitted", "observing", "escalation_pending", "emission_pending", "emitted"}

Syscalls ==
  {"submit_act", "commit_observation", "request_escalation", "emit_act", "execute_turn", "none"}

Errnos ==
  {"ok", "eperm", "eacces", "eagain", "enoent", "einval", "enosys"}

Refusals ==
  {"none",
   "input_not_licensed",
   "observation_not_admissible",
   "missing_escalation_grant",
   "invalid_escalation_grant",
   "illegal_modality_escalation",
   "insufficient_grounds",
   "claim_ground_coverage_missing",
   "output_not_licensed",
   "runtime_not_implemented",
   "syscall_not_allowed_in_state",
   "emit_modality_mismatch"}

Modalities ==
  {"descriptive", "evaluative", "proposal", "assertive", "directive", "decision", "refusal"}

AssertivePlus == {"assertive", "directive", "decision"}

ObsSourceKinds == {"tool", "human", "env", "model", "external_system"}

AllowedSyscallsByState(state) ==
  CASE state = "idle"               -> {"submit_act", "execute_turn"}
    [] state = "act_submitted"      -> {"commit_observation", "request_escalation", "emit_act", "execute_turn"}
    [] state = "observing"          -> {"commit_observation", "request_escalation", "emit_act", "execute_turn"}
    [] state = "escalation_pending" -> {"request_escalation"}
    [] state = "emission_pending"   -> {"emit_act"}
    [] state = "emitted"            -> {"submit_act", "execute_turn"}
    [] OTHER                        -> {}

MinGroundsByModality(modality) ==
  CASE modality = "descriptive" -> 0
    [] modality = "evaluative"  -> 1
    [] modality = "proposal"    -> 1
    [] modality = "assertive"   -> 1
    [] modality = "directive"   -> 2
    [] modality = "decision"    -> 2
    [] modality = "refusal"     -> 0
    [] OTHER                    -> 0

CanTransition(from, to) ==
  \/ /\ from = "descriptive" /\ to \in {"descriptive", "evaluative"}
  \/ /\ from = "evaluative"  /\ to \in {"evaluative", "proposal"}
  \/ /\ from = "proposal"    /\ to \in {"proposal", "assertive"}
  \/ /\ from = "assertive"   /\ to = "assertive"
  \/ /\ from = "directive"   /\ to = "directive"
  \/ /\ from = "decision"    /\ to = "decision"
  \/ /\ from = "refusal"     /\ to = "refusal"

VARIABLES
  kstate,
  prevState,
  activeAct,
  activeModality,
  escalationFromModality,
  obsCount,
  obsSources,
  grantPresent,
  grantFrom,
  grantTo,
  grantRequiredSources,
  grantUses,
  grantMaxUses,
  requestedModality,
  candidateModality,
  claimCoverageOK,
  lastSyscall,
  lastErrno,
  lastRefusal

vars ==
  << kstate,
     prevState,
     activeAct,
     activeModality,
     escalationFromModality,
     obsCount,
     obsSources,
     grantPresent,
     grantFrom,
     grantTo,
     grantRequiredSources,
     grantUses,
     grantMaxUses,
     requestedModality,
     candidateModality,
     claimCoverageOK,
     lastSyscall,
     lastErrno,
     lastRefusal >>

Init ==
  /\ kstate = "idle"
  /\ prevState = "idle"
  /\ activeAct = FALSE
  /\ activeModality = "descriptive"
  /\ escalationFromModality = "descriptive"
  /\ obsCount = 0
  /\ obsSources = {}
  /\ grantPresent = FALSE
  /\ grantFrom = "descriptive"
  /\ grantTo = "descriptive"
  /\ grantRequiredSources = {}
  /\ grantUses = 0
  /\ grantMaxUses = 0
  /\ requestedModality = "descriptive"
  /\ candidateModality = "descriptive"
  /\ claimCoverageOK = TRUE
  /\ lastSyscall = "none"
  /\ lastErrno = "ok"
  /\ lastRefusal = "none"

SubmitActLicensed ==
  /\ kstate \in {"idle", "emitted"}
  /\ activeAct' = TRUE
  /\ kstate' = "act_submitted"
  /\ prevState' = prevState
  /\ activeModality' \in Modalities
  /\ escalationFromModality' = activeModality'
  /\ obsCount' = 0
  /\ obsSources' = {}
  /\ grantPresent' = FALSE
  /\ grantFrom' = activeModality'
  /\ grantTo' = activeModality'
  /\ grantRequiredSources' = {}
  /\ grantUses' = 0
  /\ grantMaxUses' = 0
  /\ requestedModality' = activeModality'
  /\ candidateModality' = activeModality'
  /\ claimCoverageOK' = TRUE
  /\ lastSyscall' = "submit_act"
  /\ lastErrno' = "ok"
  /\ lastRefusal' = "none"

SubmitActRejected ==
  /\ kstate \in {"idle", "emitted"}
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "submit_act"
  /\ lastErrno' = "eperm"
  /\ lastRefusal' = "input_not_licensed"

CommitObservationSuccess ==
  /\ kstate \in {"act_submitted", "observing"}
  /\ activeAct
  /\ obsCount < MaxObs
  /\ \E src \in (ObsSourceKinds \ {"model"}):
       /\ kstate' = "observing"
       /\ obsCount' = obsCount + 1
       /\ obsSources' = obsSources \cup {src}
  /\ UNCHANGED <<prevState, activeAct, activeModality, escalationFromModality,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "commit_observation"
  /\ lastErrno' = "ok"
  /\ lastRefusal' = "none"

CommitObservationModelRejected ==
  /\ kstate \in {"act_submitted", "observing"}
  /\ activeAct
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "commit_observation"
  /\ lastErrno' = "eacces"
  /\ lastRefusal' = "observation_not_admissible"

RequestEscalationStart ==
  /\ kstate \in {"act_submitted", "observing"}
  /\ activeAct
  /\ requestedModality' \in Modalities
  /\ grantPresent' = TRUE
  /\ grantFrom' \in Modalities
  /\ grantTo' \in Modalities
  /\ \E reqSources \in SUBSET (ObsSourceKinds \ {"model"}):
       grantRequiredSources' = reqSources
  /\ grantMaxUses' \in 1..MaxGrantUses
  /\ grantUses' \in 0..grantMaxUses'
  /\ prevState' = kstate
  /\ escalationFromModality' = activeModality
  /\ kstate' = "escalation_pending"
  /\ UNCHANGED <<activeAct, activeModality, obsCount, obsSources, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "request_escalation"
  /\ lastErrno' = "ok"
  /\ lastRefusal' = "none"

RequestEscalationDeniedMissingGrant ==
  /\ kstate \in {"act_submitted", "observing"}
  /\ activeAct
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "request_escalation"
  /\ lastErrno' = "eacces"
  /\ lastRefusal' = "missing_escalation_grant"

EscalationDeniedInvalidGrant ==
  /\ kstate = "escalation_pending"
  /\ grantPresent
  /\ (grantFrom # activeModality \/ grantTo # requestedModality \/ grantUses >= grantMaxUses)
  /\ kstate' = prevState
  /\ UNCHANGED <<prevState, activeAct, activeModality, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "request_escalation"
  /\ lastErrno' = "eacces"
  /\ lastRefusal' = "invalid_escalation_grant"

EscalationDeniedPolicy ==
  /\ kstate = "escalation_pending"
  /\ grantPresent
  /\ grantFrom = activeModality
  /\ grantTo = requestedModality
  /\ grantUses < grantMaxUses
  /\ ~CanTransition(activeModality, requestedModality)
  /\ kstate' = prevState
  /\ UNCHANGED <<prevState, activeAct, activeModality, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "request_escalation"
  /\ lastErrno' = "eperm"
  /\ lastRefusal' = "illegal_modality_escalation"

EscalationDeniedGrounds ==
  /\ kstate = "escalation_pending"
  /\ grantPresent
  /\ grantFrom = activeModality
  /\ grantTo = requestedModality
  /\ grantUses < grantMaxUses
  /\ CanTransition(activeModality, requestedModality)
  /\ (obsCount < MinGroundsByModality(requestedModality)
      \/ ~(grantRequiredSources \subseteq obsSources))
  /\ kstate' = prevState
  /\ UNCHANGED <<prevState, activeAct, activeModality, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantUses, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "request_escalation"
  /\ lastErrno' = "eagain"
  /\ lastRefusal' = "insufficient_grounds"

EscalationSuccess ==
  /\ kstate = "escalation_pending"
  /\ grantPresent
  /\ grantFrom = activeModality
  /\ grantTo = requestedModality
  /\ grantUses < grantMaxUses
  /\ CanTransition(activeModality, requestedModality)
  /\ obsCount >= MinGroundsByModality(requestedModality)
  /\ grantRequiredSources \subseteq obsSources
  /\ kstate' = "emission_pending"
  /\ activeModality' = requestedModality
  /\ grantUses' = grantUses + 1
  /\ UNCHANGED <<prevState, activeAct, escalationFromModality, obsCount, obsSources,
                 grantPresent, grantFrom, grantTo, grantRequiredSources, grantMaxUses,
                 requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "request_escalation"
  /\ lastErrno' = "ok"
  /\ lastRefusal' = "none"

PrepareCandidate ==
  /\ kstate = "emission_pending"
  /\ candidateModality' \in {activeModality, "refusal"}
  /\ claimCoverageOK' \in BOOLEAN
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality,
                 obsCount, obsSources, grantPresent, grantFrom, grantTo, grantRequiredSources,
                 grantUses, grantMaxUses, requestedModality>>
  /\ lastSyscall' = "none"
  /\ lastErrno' = "ok"
  /\ lastRefusal' = "none"

EmitDeniedModalityMismatch ==
  /\ kstate = "emission_pending"
  /\ candidateModality # activeModality
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality,
                 obsCount, obsSources, grantPresent, grantFrom, grantTo, grantRequiredSources,
                 grantUses, grantMaxUses, requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "emit_act"
  /\ lastErrno' = "einval"
  /\ lastRefusal' = "emit_modality_mismatch"

EmitDeniedGrounds ==
  /\ kstate = "emission_pending"
  /\ candidateModality = activeModality
  /\ obsCount < MinGroundsByModality(activeModality)
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality,
                 obsCount, obsSources, grantPresent, grantFrom, grantTo, grantRequiredSources,
                 grantUses, grantMaxUses, requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "emit_act"
  /\ lastErrno' = "eagain"
  /\ lastRefusal' = "insufficient_grounds"

EmitDeniedClaimCoverage ==
  /\ kstate = "emission_pending"
  /\ candidateModality = activeModality
  /\ obsCount >= MinGroundsByModality(activeModality)
  /\ activeModality \in AssertivePlus
  /\ ~claimCoverageOK
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality,
                 obsCount, obsSources, grantPresent, grantFrom, grantTo, grantRequiredSources,
                 grantUses, grantMaxUses, requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "emit_act"
  /\ lastErrno' = "eagain"
  /\ lastRefusal' = "claim_ground_coverage_missing"

EmitDeniedOutputLicense ==
  /\ kstate = "emission_pending"
  /\ candidateModality = activeModality
  /\ obsCount >= MinGroundsByModality(activeModality)
  /\ (activeModality \notin AssertivePlus \/ claimCoverageOK)
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality,
                 obsCount, obsSources, grantPresent, grantFrom, grantTo, grantRequiredSources,
                 grantUses, grantMaxUses, requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "emit_act"
  /\ lastErrno' = "eperm"
  /\ lastRefusal' = "output_not_licensed"

EmitSuccess ==
  /\ kstate = "emission_pending"
  /\ candidateModality = activeModality
  /\ obsCount >= MinGroundsByModality(activeModality)
  /\ (activeModality \notin AssertivePlus \/ claimCoverageOK)
  /\ activeAct' = FALSE
  /\ kstate' = "emitted"
  /\ prevState' = prevState
  /\ activeModality' = activeModality
  /\ escalationFromModality' = escalationFromModality
  /\ obsCount' = 0
  /\ obsSources' = {}
  /\ grantPresent' = FALSE
  /\ grantFrom' = grantFrom
  /\ grantTo' = grantTo
  /\ grantRequiredSources' = {}
  /\ grantUses' = grantUses
  /\ grantMaxUses' = grantMaxUses
  /\ requestedModality' = requestedModality
  /\ candidateModality' = candidateModality
  /\ claimCoverageOK' = claimCoverageOK
  /\ lastSyscall' = "emit_act"
  /\ lastErrno' = "ok"
  /\ lastRefusal' = "none"

ExecuteTurnNotImplemented ==
  /\ kstate \in {"idle", "act_submitted", "observing"}
  /\ UNCHANGED <<kstate, prevState, activeAct, activeModality, escalationFromModality,
                 obsCount, obsSources, grantPresent, grantFrom, grantTo, grantRequiredSources,
                 grantUses, grantMaxUses, requestedModality, candidateModality, claimCoverageOK>>
  /\ lastSyscall' = "execute_turn"
  /\ lastErrno' = "enosys"
  /\ lastRefusal' = "runtime_not_implemented"

Next ==
  \/ SubmitActLicensed
  \/ SubmitActRejected
  \/ CommitObservationSuccess
  \/ CommitObservationModelRejected
  \/ RequestEscalationStart
  \/ RequestEscalationDeniedMissingGrant
  \/ EscalationDeniedInvalidGrant
  \/ EscalationDeniedPolicy
  \/ EscalationDeniedGrounds
  \/ EscalationSuccess
  \/ PrepareCandidate
  \/ EmitDeniedModalityMismatch
  \/ EmitDeniedGrounds
  \/ EmitDeniedClaimCoverage
  \/ EmitDeniedOutputLicense
  \/ EmitSuccess
  \/ ExecuteTurnNotImplemented

Spec == Init /\ [][Next]_vars

TypeInv ==
  /\ kstate \in KernelStates
  /\ prevState \in KernelStates
  /\ activeAct \in BOOLEAN
  /\ activeModality \in Modalities
  /\ escalationFromModality \in Modalities
  /\ obsCount \in 0..MaxObs
  /\ obsSources \subseteq ObsSourceKinds
  /\ grantPresent \in BOOLEAN
  /\ grantFrom \in Modalities
  /\ grantTo \in Modalities
  /\ grantRequiredSources \subseteq (ObsSourceKinds \ {"model"})
  /\ grantUses \in 0..MaxGrantUses
  /\ grantMaxUses \in 0..MaxGrantUses
  /\ requestedModality \in Modalities
  /\ candidateModality \in Modalities
  /\ claimCoverageOK \in BOOLEAN
  /\ lastSyscall \in Syscalls
  /\ lastErrno \in Errnos
  /\ lastRefusal \in Refusals

StateConsistencyInv ==
  /\ (kstate \in {"idle", "emitted"}) => ~activeAct
  /\ (kstate \in {"act_submitted", "observing", "escalation_pending", "emission_pending"}) => activeAct

NoModelGroundingInv ==
  "model" \notin obsSources

EscalationTransitionInv ==
  (kstate = "emission_pending") => CanTransition(escalationFromModality, activeModality)

GrantUseBoundInv ==
  grantUses <= grantMaxUses

PrevStateRollbackDomainInv ==
  (kstate = "escalation_pending") => prevState \in {"act_submitted", "observing"}

NoImplicitEscalationInv ==
  (activeAct /\ (activeModality # escalationFromModality)) => kstate = "emission_pending"

RefusalErrnoConsistencyInv ==
  /\ (lastRefusal = "missing_escalation_grant") => (lastErrno = "eacces")
  /\ (lastRefusal = "invalid_escalation_grant") => (lastErrno = "eacces")
  /\ (lastRefusal = "illegal_modality_escalation") => (lastErrno = "eperm")
  /\ (lastRefusal = "insufficient_grounds") => (lastErrno = "eagain")
  /\ (lastRefusal = "claim_ground_coverage_missing") => (lastErrno = "eagain")
  /\ (lastRefusal = "emit_modality_mismatch") => (lastErrno = "einval")
  /\ (lastRefusal = "output_not_licensed") => (lastErrno = "eperm")
  /\ (lastRefusal = "runtime_not_implemented") => (lastErrno = "enosys")

=============================================================================
