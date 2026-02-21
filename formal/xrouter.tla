---- MODULE xrouter ----
EXTENDS TLC, Naturals

\* Minimal post-paid formal model for xrouter request flow with streaming:
\* ingest -> tokenize -> hold? -> generate(stream) -> finalize? -> done

CONSTANTS MaxBillableTokens, MaxLedger

States ==
  {"idle", "ingest", "tokenize", "hold", "generate", "finalize", "done", "failed"}

Actions ==
  {"none", "start",
   "ingest_ok", "ingest_fail",
   "tokenize_ok", "tokenize_fail",
   "hold_ok", "hold_fail",
   "generate_chunk", "generate_done", "generate_fail",
   "finalize_ok", "finalize_fail",
   "recovery_resolved_external",
   "client_disconnect",
   "reset"}

VARIABLES
  kstate,
  billingEnabled,
  holdAcquired,
  holdReleased,
  chargeCommitted,
  chargeRecoveryRequired,
  recoveredExternally,
  clientConnected,
  billableTokens,
  externalLedger,
  responseCompleted,
  lastAction

vars ==
  <<kstate,
    billingEnabled,
    holdAcquired,
    holdReleased,
    chargeCommitted,
    chargeRecoveryRequired,
    recoveredExternally,
    clientConnected,
    billableTokens,
    externalLedger,
    responseCompleted,
    lastAction>>

Init ==
  /\ kstate = "idle"
  /\ billingEnabled = FALSE
  /\ holdAcquired = FALSE
  /\ holdReleased = FALSE
  /\ chargeCommitted = FALSE
  /\ chargeRecoveryRequired = FALSE
  /\ recoveredExternally = FALSE
  /\ clientConnected = FALSE
  /\ billableTokens = 0
  /\ externalLedger = 0
  /\ responseCompleted = FALSE
  /\ lastAction = "none"

Start ==
  /\ kstate = "idle"
  /\ kstate' = "ingest"
  /\ billingEnabled' \in BOOLEAN
  /\ holdAcquired' = FALSE
  /\ holdReleased' = FALSE
  /\ chargeCommitted' = FALSE
  /\ chargeRecoveryRequired' = FALSE
  /\ recoveredExternally' = FALSE
  /\ clientConnected' = TRUE
  /\ billableTokens' = 0
  /\ externalLedger' = externalLedger
  /\ responseCompleted' = FALSE
  /\ lastAction' = "start"

IngestOK ==
  /\ kstate = "ingest"
  /\ kstate' = "tokenize"
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "ingest_ok"

IngestFail ==
  /\ kstate = "ingest"
  /\ kstate' = "failed"
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "ingest_fail"

TokenizeOK ==
  /\ kstate = "tokenize"
  /\ kstate' = IF billingEnabled THEN "hold" ELSE "generate"
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "tokenize_ok"

TokenizeFail ==
  /\ kstate = "tokenize"
  /\ kstate' = "failed"
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "tokenize_fail"

HoldOK ==
  /\ kstate = "hold"
  /\ billingEnabled
  /\ kstate' = "generate"
  /\ holdAcquired' = TRUE
  /\ holdReleased' = FALSE
  /\ UNCHANGED <<billingEnabled, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "hold_ok"

HoldFail ==
  /\ kstate = "hold"
  /\ billingEnabled
  /\ kstate' = "failed"
  /\ holdAcquired' = FALSE
  /\ holdReleased' = FALSE
  /\ UNCHANGED <<billingEnabled, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "hold_fail"

GenerateChunk ==
  /\ kstate = "generate"
  /\ (~billingEnabled) \/ holdAcquired
  /\ billableTokens < MaxBillableTokens
  /\ kstate' = "generate"
  /\ billableTokens' = billableTokens + 1
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, externalLedger, responseCompleted>>
  /\ lastAction' = "generate_chunk"

GenerateDone ==
  /\ kstate = "generate"
  /\ (~billingEnabled) \/ holdAcquired
  /\ kstate' = IF billingEnabled THEN "finalize" ELSE "done"
  /\ responseCompleted' = IF billingEnabled THEN FALSE ELSE TRUE
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted, chargeRecoveryRequired, recoveredExternally,
                 clientConnected, billableTokens, externalLedger>>
  /\ lastAction' = "generate_done"

GenerateFail ==
  /\ kstate = "generate"
  /\ (~billingEnabled) \/ holdAcquired
  /\ kstate' = IF billingEnabled /\ billableTokens > 0 THEN "finalize" ELSE "failed"
  /\ holdAcquired' = IF billingEnabled /\ billableTokens > 0 THEN holdAcquired ELSE FALSE
  /\ holdReleased' = IF billingEnabled /\ billableTokens > 0
                      THEN holdReleased
                      ELSE holdReleased \/ holdAcquired
  /\ chargeCommitted' = chargeCommitted
  /\ chargeRecoveryRequired' = IF billingEnabled /\ billableTokens > 0
                               THEN chargeRecoveryRequired
                               ELSE FALSE
  /\ recoveredExternally' = FALSE
  /\ responseCompleted' = FALSE
  /\ UNCHANGED <<billingEnabled, clientConnected, billableTokens, externalLedger>>
  /\ lastAction' = "generate_fail"

FinalizeOK ==
  /\ kstate = "finalize"
  /\ billingEnabled
  /\ holdAcquired
  /\ ~chargeCommitted
  /\ IF billableTokens > 0
        THEN externalLedger + billableTokens <= MaxLedger
        ELSE TRUE
  /\ kstate' = "done"
  /\ holdAcquired' = FALSE
  /\ holdReleased' = TRUE
  /\ chargeCommitted' = IF billableTokens > 0 THEN TRUE ELSE chargeCommitted
  /\ chargeRecoveryRequired' = FALSE
  /\ recoveredExternally' = FALSE
  /\ externalLedger' = IF billableTokens > 0 THEN externalLedger + billableTokens ELSE externalLedger
  /\ responseCompleted' = TRUE
  /\ UNCHANGED <<billingEnabled, clientConnected, billableTokens>>
  /\ lastAction' = "finalize_ok"

FinalizeFail ==
  /\ kstate = "finalize"
  /\ billingEnabled
  /\ holdAcquired
  /\ ~chargeCommitted
  /\ kstate' = "failed"
  /\ holdAcquired' = FALSE
  /\ holdReleased' = TRUE
  /\ chargeCommitted' = FALSE
  /\ chargeRecoveryRequired' = (billableTokens > 0)
  /\ recoveredExternally' = FALSE
  /\ externalLedger' = externalLedger
  /\ responseCompleted' = FALSE
  /\ UNCHANGED <<billingEnabled, clientConnected, billableTokens>>
  /\ lastAction' = "finalize_fail"

RecoveryResolved ==
  /\ kstate = "failed"
  /\ chargeRecoveryRequired
  /\ kstate' = "failed"
  /\ chargeRecoveryRequired' = FALSE
  /\ recoveredExternally' = TRUE
  /\ UNCHANGED <<billingEnabled, holdAcquired, holdReleased, chargeCommitted,
                 clientConnected, billableTokens, externalLedger, responseCompleted>>
  /\ lastAction' = "recovery_resolved_external"

ClientDisconnect ==
  /\ kstate \in {"ingest", "tokenize", "hold", "generate", "finalize"}
  /\ clientConnected
  /\ clientConnected' = FALSE
  /\ responseCompleted' = FALSE
  /\ IF kstate \in {"ingest", "tokenize", "hold"}
       THEN /\ kstate' = "failed"
            /\ holdAcquired' = FALSE
            /\ holdReleased' = holdReleased
            /\ chargeCommitted' = chargeCommitted
            /\ chargeRecoveryRequired' = chargeRecoveryRequired
            /\ recoveredExternally' = recoveredExternally
            /\ externalLedger' = externalLedger
       ELSE /\ kstate' = kstate
            /\ holdAcquired' = holdAcquired
            /\ holdReleased' = holdReleased
            /\ chargeCommitted' = chargeCommitted
            /\ chargeRecoveryRequired' = chargeRecoveryRequired
            /\ recoveredExternally' = recoveredExternally
            /\ externalLedger' = externalLedger
  /\ UNCHANGED <<billingEnabled, billableTokens>>
  /\ lastAction' = "client_disconnect"

Reset ==
  /\ kstate \in {"done", "failed"}
  /\ ~chargeRecoveryRequired
  /\ kstate' = "idle"
  /\ billingEnabled' = FALSE
  /\ holdAcquired' = FALSE
  /\ holdReleased' = FALSE
  /\ chargeCommitted' = FALSE
  /\ chargeRecoveryRequired' = FALSE
  /\ recoveredExternally' = FALSE
  /\ clientConnected' = FALSE
  /\ billableTokens' = 0
  /\ externalLedger' = externalLedger
  /\ responseCompleted' = FALSE
  /\ lastAction' = "reset"

Next ==
  \/ Start
  \/ IngestOK
  \/ IngestFail
  \/ TokenizeOK
  \/ TokenizeFail
  \/ HoldOK
  \/ HoldFail
  \/ GenerateChunk
  \/ GenerateDone
  \/ GenerateFail
  \/ FinalizeOK
  \/ FinalizeFail
  \/ RecoveryResolved
  \/ ClientDisconnect
  \/ Reset

Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ WF_vars(GenerateDone \/ GenerateFail)
  /\ WF_vars(FinalizeOK \/ FinalizeFail)
  /\ WF_vars(RecoveryResolved)

TypeInv ==
  /\ kstate \in States
  /\ billingEnabled \in BOOLEAN
  /\ holdAcquired \in BOOLEAN
  /\ holdReleased \in BOOLEAN
  /\ chargeCommitted \in BOOLEAN
  /\ chargeRecoveryRequired \in BOOLEAN
  /\ recoveredExternally \in BOOLEAN
  /\ clientConnected \in BOOLEAN
  /\ billableTokens \in 0..MaxBillableTokens
  /\ externalLedger \in 0..MaxLedger
  /\ responseCompleted \in BOOLEAN
  /\ lastAction \in Actions

BillingGateInv ==
  /\ kstate = "hold" => billingEnabled
  /\ kstate = "finalize" => /\ billingEnabled /\ holdAcquired
  /\ billingEnabled /\ kstate = "generate" => holdAcquired
  /\ holdAcquired => billingEnabled
  /\ chargeCommitted => /\ billingEnabled /\ holdReleased /\ billableTokens > 0
  /\ kstate = "done" /\ billingEnabled /\ billableTokens > 0 => chargeCommitted
  /\ chargeCommitted => externalLedger >= billableTokens

HoldLifecycleInv ==
  /\ holdReleased => ~holdAcquired
  /\ kstate \in {"done", "failed"} => ~holdAcquired

NoFreeTokensInv ==
  /\ billingEnabled /\ billableTokens > 0 /\ kstate = "failed"
     => chargeCommitted \/ chargeRecoveryRequired \/ recoveredExternally \/ kstate = "finalize"
  /\ chargeRecoveryRequired => /\ billingEnabled /\ billableTokens > 0 /\ ~chargeCommitted
  /\ recoveredExternally => /\ billingEnabled /\ billableTokens > 0 /\ ~chargeCommitted /\ ~chargeRecoveryRequired
  /\ kstate = "idle" => ~chargeRecoveryRequired

DebtProgressLiveness ==
  []((billingEnabled /\ billableTokens > 0 /\ kstate = "generate")
    ~> (chargeCommitted \/ chargeRecoveryRequired \/ recoveredExternally))

StreamingInv ==
  /\ billableTokens > 0 => kstate \in {"generate", "finalize", "done", "failed"}
  /\ kstate = "done" => responseCompleted
  /\ responseCompleted => kstate = "done"

DisconnectSafetyInv ==
  /\ kstate \in {"ingest", "tokenize", "hold"} => clientConnected
  /\ ~clientConnected => kstate \in {"idle", "generate", "finalize", "done", "failed"}

=============================================================================
