---------------------------- MODULE BranchBudget ----------------------------
(* Scenario (#24, rule 1: a branch counts in full): key 1 has a balance of
   2 and nothing sold. t1 runs BuyUnder(1,1,1) twice - each draw is taken
   only while fewer than one ticket is sold, so the second branch is never
   taken - and its budget is nevertheless 2. t2 draws one ticket. While t1
   is live its untaken branch keeps the whole balance reserved, so t2
   waits on it although t1 draws only one; the unused reservation is
   released at t1's commit and t2 then reserves and draws the remaining
   one. *)

EXTENDS Kernel

BranchBudgetProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<BuyUnder(1, 1, 1), BuyUnder(1, 1, 1)>>
      [] t = "t2" -> <<Buy(1, 1)>>]

BranchBudgetInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]]

(* Witness: t1 has skipped its second branch, still holds the reservation
   of 2 against a draw of 1, and t2 waits on it for the one it would need. *)
BranchBudgetOverReservedUnreached ==
  ~(resp["t1"] = <<"ok", "skipped">> /\ Active("t1")
    /\ reserved["t1"][1] = 2 /\ draws["t1"][1].bal = 1
    /\ WaitsOn("t2") = {"t1"})

(* Both committed: the skipped branch left its ticket for t2. *)
BranchBudgetReleasedUnreached ==
  ~(status["t1"] = "committed" /\ status["t2"] = "committed"
    /\ resp["t2"] = <<"ok">> /\ db[1].bal = 0)

============================================================================
