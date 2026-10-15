----------------------------- MODULE CrashFree -----------------------------
(* Scenario (#51): the Composed programs (escrow, claim, pin and delete
   contending through a table scan) on the durable engine with no failure.
   Commit takes its three steps (append, flush, acknowledge), and every
   Kernel invariant and `RequestedLeadsToTerminal` still hold. *)

EXTENDS Durable, Composed

CrashFreeFailures == 0

============================================================================
