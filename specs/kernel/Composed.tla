----------------------------- MODULE Composed -----------------------------
(* Scenario: escrow, claim, pin, delete and a table scan whose first
   operations do not serialize the run. Key 1 is an event row with a
   quota of 2; key 2 is absent; key 3 is a present row with no parent.
     t1 draws one ticket from the event's escrow, then inserts key 2 as a
        child of key 3 (claim on 2, pin on 3);
     t2 draws one ticket as well, then deletes key 3 (exclusive against
        3's reverse references);
     t3 scans the whole table (a table resource).
   The two draws fit the allowance and coexist; with both live, t3's scan
   waits on both, and whichever of t1 and t2 reaches key 3 second waits on
   the other while still holding its draw. The order decides the outcome:
   insert first rejects the delete with "referenced", delete first rejects
   the insert with "noparent", and a rejected transaction's draw leaves
   no effect on the quota. *)

EXTENDS Kernel

ComposedProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Buy(1, 1), Insert(2, 3)>>
      [] t = "t2" -> <<Buy(1, 1), Delete(3)>>
      [] t = "t3" -> <<Scan(1, 3)>>]

ComposedInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]
      [] k = 2 -> AbsentRow
      [] k = 3 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* Both draws are live at once and the table scan waits on both. *)
ComposedDrawsUnreached ==
  ~(Res("DRAW", {1}) \in held["t1"] /\ Res("DRAW", {1}) \in held["t2"]
    /\ WaitsOn("t3") = {"t1", "t2"})

(* t1 holds its pin on 3 and t2, still holding its draw, waits on it. *)
ComposedPinDeleteUnreached ==
  ~(Res("PIN", {3}) \in held["t1"] /\ Res("DRAW", {1}) \in held["t2"]
    /\ WaitsOn("t2") = {"t1"})

(* The insert won: t2 was told "referenced" and its draw left no effect. *)
ComposedReferencedUnreached ==
  ~(resp["t2"] = <<"ok", "referenced">> /\ status["t1"] = "committed"
    /\ db[1].bal = 1 /\ db[2].parent = 3)

============================================================================
