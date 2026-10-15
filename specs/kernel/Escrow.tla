---------------------------- MODULE Escrow ----------------------------
(* Kernel model slice 1, part B.3 (issue #16): the escrow of
   docs/concept.md §3.2/§5 - a numeric bound ("balance must stay >= 0")
   split into per-caller allowances, admitted pessimistically so a
   concurrent draw sees the reduced remaining allowance before either
   commits. See specs/README.md for the scenario and its bounds. *)

EXTENDS Naturals, FiniteSets, TLC

CONSTANTS e1, e2, e3

Txns == {e1, e2, e3}
Ceiling == 3

(* e1 and e2 together (2+2=4) exceed the ceiling, as do all three (5); e1
   with e3 (2+1=3) or e2 with e3 (2+1=3) exactly exhaust it - small enough
   to force real contention within the 2-3 transaction bound. *)
Amount == (e1 :> 2) @@ (e2 :> 2) @@ (e3 :> 1)

VARIABLES status, drawHeld

Vars == <<status, drawHeld>>

TxnStates == {"active", "committed", "aborted"}

Init ==
  /\ status = [t \in Txns |-> "active"]
  /\ drawHeld = [t \in Txns |-> FALSE]

RECURSIVE SumAmounts(_)
SumAmounts(S) ==
  IF S = {} THEN 0
  ELSE LET t == CHOOSE x \in S : TRUE IN Amount[t] + SumAmounts(S \ {t})

CommittedSum == SumAmounts({t \in Txns : status[t] = "committed"})
ActiveHeldSum == SumAmounts({t \in Txns : status[t] = "active" /\ drawHeld[t]})

(* Pessimistic admission: a draw is granted tentatively only while the
   ceiling has room for every already-admitted draw plus this one - this
   is what makes the remaining allowance a concurrent draw sees already
   reflect this one, before either commits. *)
AcquireDraw(t) ==
  /\ status[t] = "active"
  /\ ~drawHeld[t]
  /\ CommittedSum + ActiveHeldSum + Amount[t] <= Ceiling
  /\ drawHeld' = [drawHeld EXCEPT ![t] = TRUE]
  /\ UNCHANGED status

(* Committing consumes the allowance for good - unlike a plain lock, a
   committed draw is never released back to the pool in this model, the
   same durability pattern as Claims.committedValues and
   Pins.parentDeleted. *)
CommitDraw(t) ==
  /\ status[t] = "active"
  /\ drawHeld[t]
  /\ status' = [status EXCEPT ![t] = "committed"]
  /\ UNCHANGED drawHeld

AbortDraw(t) ==
  /\ status[t] = "active"
  /\ status' = [status EXCEPT ![t] = "aborted"]
  /\ UNCHANGED drawHeld

AllDone == \A t \in Txns : status[t] \in {"committed", "aborted"}

Terminating == AllDone /\ UNCHANGED Vars

Next == \E t \in Txns : AcquireDraw(t) \/ CommitDraw(t) \/ AbortDraw(t) \/ Terminating

Spec == Init /\ [][Next]_Vars

TypeOK ==
  /\ status \in [Txns -> TxnStates]
  /\ drawHeld \in [Txns -> BOOLEAN]

(* The check constraint itself: the ceiling never lets the sum of
   committed allowances exceed it. *)
BalanceNonNegative == CommittedSum <= Ceiling

(* The stronger pessimistic-admission invariant: even counting tentative,
   not-yet-committed draws, the ceiling is never oversubscribed. *)
NoOveradmission == CommittedSum + ActiveHeldSum <= Ceiling

============================================================================
