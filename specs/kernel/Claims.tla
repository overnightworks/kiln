---------------------------- MODULE Claims ----------------------------
(* Kernel model slice 1, part B.1 (issue #16): the unique claim of
   docs/concept.md §3.2/§5 - an exclusive reservation on one value, with a
   tentative/committed distinction and a loser that waits on the holder's
   outcome. See specs/README.md for the scenario and its bounds. *)

EXTENDS Naturals, FiniteSets

CONSTANTS t1, t2, t3

Txns == {t1, t2, t3}
ClaimValues == {"x", "y"}

(* t1 and t2 both claim "x": direct contention, exercising both outcomes
   named in concept.md §5 - the holder commits (the waiter's claim fails)
   and the holder aborts (the waiter proceeds as if never claimed). t3
   claims the independent value "y". *)
ClaimOf == [t \in Txns |-> IF t = t3 THEN "y" ELSE "x"]

VARIABLES status, acquired, committedValues

Vars == <<status, acquired, committedValues>>

TxnStates == {"active", "committed", "aborted"}

Init ==
  /\ status = [t \in Txns |-> "active"]
  /\ acquired = [t \in Txns |-> FALSE]
  /\ committedValues = {}

TentativelyHeld(v) ==
  \E u \in Txns : status[u] = "active" /\ acquired[u] /\ ClaimOf[u] = v

(* A claim is held tentatively only while no other active transaction
   already holds it and no transaction has ever committed it - once a
   value is in committedValues it is taken forever, unlike a plain lock. *)
AttemptClaim(t) ==
  /\ status[t] = "active"
  /\ ~acquired[t]
  /\ LET v == ClaimOf[t] IN
       /\ v \notin committedValues
       /\ ~TentativelyHeld(v)
  /\ acquired' = [acquired EXCEPT ![t] = TRUE]
  /\ UNCHANGED <<status, committedValues>>

CommitClaim(t) ==
  /\ status[t] = "active"
  /\ acquired[t]
  /\ status' = [status EXCEPT ![t] = "committed"]
  /\ committedValues' = committedValues \cup {ClaimOf[t]}
  /\ UNCHANGED acquired

(* A known abort, for any reason (concept.md §5), releases a tentative
   claim as if it had never been taken; a transaction whose value is
   already in committedValues can only ever reach this action, modelling
   "the waiter's claim fails with the uniqueness error" as that
   transaction's own abort. *)
AbortClaim(t) ==
  /\ status[t] = "active"
  /\ status' = [status EXCEPT ![t] = "aborted"]
  /\ UNCHANGED <<acquired, committedValues>>

AllDone == \A t \in Txns : status[t] \in {"committed", "aborted"}

Terminating == AllDone /\ UNCHANGED Vars

Next == \E t \in Txns : AttemptClaim(t) \/ CommitClaim(t) \/ AbortClaim(t) \/ Terminating

Spec == Init /\ [][Next]_Vars

TypeOK ==
  /\ status \in [Txns -> TxnStates]
  /\ acquired \in [Txns -> BOOLEAN]
  /\ committedValues \subseteq ClaimValues

(* Uniqueness: a claim admits exactly one committed winner per value. *)
Uniqueness ==
  \A v \in ClaimValues :
    Cardinality({t \in Txns : status[t] = "committed" /\ ClaimOf[t] = v}) <= 1

(* Mutual exclusion of tentative holders on the same value. *)
MutualExclusion ==
  \A t, u \in Txns :
    (t # u /\ ClaimOf[t] = ClaimOf[u]) =>
      ~(status[t] = "active" /\ acquired[t] /\ status[u] = "active" /\ acquired[u])

============================================================================
