---- MODULE xrouter ----
EXTENDS TLC, Naturals

\* Core non-billing formal model for xrouter request flow with streaming:
\* ingest -> tokenize -> generate(stream) -> done

CONSTANT MaxOutputTokens

States ==
  {"idle", "ingest", "tokenize", "generate", "done", "failed"}

Actions ==
  {"none", "start",
   "ingest_ok", "ingest_fail",
   "tokenize_ok", "tokenize_fail",
   "generate_chunk", "generate_done", "generate_fail",
   "client_disconnect",
   "reset"}

VARIABLES
  kstate,
  clientConnected,
  outputTokens,
  responseCompleted,
  lastAction

vars ==
  <<kstate,
    clientConnected,
    outputTokens,
    responseCompleted,
    lastAction>>

Init ==
  /\ kstate = "idle"
  /\ clientConnected = FALSE
  /\ outputTokens = 0
  /\ responseCompleted = FALSE
  /\ lastAction = "none"

Start ==
  /\ kstate = "idle"
  /\ kstate' = "ingest"
  /\ clientConnected' = TRUE
  /\ outputTokens' = 0
  /\ responseCompleted' = FALSE
  /\ lastAction' = "start"

IngestOK ==
  /\ kstate = "ingest"
  /\ kstate' = "tokenize"
  /\ UNCHANGED <<clientConnected, outputTokens, responseCompleted>>
  /\ lastAction' = "ingest_ok"

IngestFail ==
  /\ kstate = "ingest"
  /\ kstate' = "failed"
  /\ responseCompleted' = FALSE
  /\ UNCHANGED <<clientConnected, outputTokens>>
  /\ lastAction' = "ingest_fail"

TokenizeOK ==
  /\ kstate = "tokenize"
  /\ kstate' = "generate"
  /\ UNCHANGED <<clientConnected, outputTokens, responseCompleted>>
  /\ lastAction' = "tokenize_ok"

TokenizeFail ==
  /\ kstate = "tokenize"
  /\ kstate' = "failed"
  /\ responseCompleted' = FALSE
  /\ UNCHANGED <<clientConnected, outputTokens>>
  /\ lastAction' = "tokenize_fail"

GenerateChunk ==
  /\ kstate = "generate"
  /\ outputTokens < MaxOutputTokens
  /\ kstate' = "generate"
  /\ outputTokens' = outputTokens + 1
  /\ UNCHANGED <<clientConnected, responseCompleted>>
  /\ lastAction' = "generate_chunk"

GenerateDone ==
  /\ kstate = "generate"
  /\ kstate' = "done"
  /\ responseCompleted' = TRUE
  /\ UNCHANGED <<clientConnected, outputTokens>>
  /\ lastAction' = "generate_done"

GenerateFail ==
  /\ kstate = "generate"
  /\ kstate' = "failed"
  /\ responseCompleted' = FALSE
  /\ UNCHANGED <<clientConnected, outputTokens>>
  /\ lastAction' = "generate_fail"

ClientDisconnect ==
  /\ kstate \in {"ingest", "tokenize", "generate"}
  /\ clientConnected
  /\ clientConnected' = FALSE
  /\ IF kstate \in {"ingest", "tokenize"}
       THEN /\ kstate' = "failed"
            /\ responseCompleted' = FALSE
       ELSE /\ kstate' = "generate"
            /\ responseCompleted' = responseCompleted
  /\ UNCHANGED <<outputTokens>>
  /\ lastAction' = "client_disconnect"

Reset ==
  /\ kstate \in {"done", "failed"}
  /\ kstate' = "idle"
  /\ clientConnected' = FALSE
  /\ outputTokens' = 0
  /\ responseCompleted' = FALSE
  /\ lastAction' = "reset"

Next ==
  \/ Start
  \/ IngestOK
  \/ IngestFail
  \/ TokenizeOK
  \/ TokenizeFail
  \/ GenerateChunk
  \/ GenerateDone
  \/ GenerateFail
  \/ ClientDisconnect
  \/ Reset

Spec ==
  /\ Init
  /\ [][Next]_vars
  /\ WF_vars(GenerateDone \/ GenerateFail)

TypeInv ==
  /\ kstate \in States
  /\ clientConnected \in BOOLEAN
  /\ outputTokens \in 0..MaxOutputTokens
  /\ responseCompleted \in BOOLEAN
  /\ lastAction \in Actions

FlowInv ==
  /\ responseCompleted => kstate = "done"
  /\ kstate = "idle" => outputTokens = 0

StreamingInv ==
  /\ outputTokens > 0 => kstate \in {"generate", "done", "failed"}

DisconnectSafetyInv ==
  /\ kstate \in {"ingest", "tokenize"} => clientConnected
  /\ ~clientConnected => kstate \in {"idle", "generate", "done", "failed"}

GenerateProgressLiveness ==
  [](kstate = "generate" ~> (kstate = "done" \/ kstate = "failed"))

=============================================================================
