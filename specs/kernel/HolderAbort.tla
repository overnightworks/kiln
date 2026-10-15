----------------------------- MODULE HolderAbort -----------------------------
(* Scenario (#24, rule 2: no premature failure while a holder may still
   abort): key 1 has a balance of 2 and key 2 is present. t1 draws one
   ticket and then inserts key 2, which is rejected with `exists` and
   releases its reservation; t2 needs both tickets (budget 2). While t1's
   reservation is live t2 waits rather than reserving the remaining one
   and failing on it; after t1's rejection t2 reserves 2 and both its
   draws succeed, as the serial order "t1 (rejected), then t2" says. *)

EXTENDS Kernel

HolderAbortProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Buy(1, 1), Insert(2, 0)>>
      [] t = "t2" -> <<Buy(1, 1), Buy(1, 1)>>]

HolderAbortInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]
      [] k = 2 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness: t1 was rejected after its draw, and t2, which waited on it,
   now holds the whole balance. *)
HolderAbortReleasedUnreached ==
  ~(resp["t1"] = <<"ok", "exists">> /\ status["t1"] = "rejected"
    /\ Active("t2") /\ reserved["t2"][1] = 2)

============================================================================
