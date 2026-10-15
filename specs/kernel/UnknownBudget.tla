---------------------------- MODULE UnknownBudget ----------------------------
(* Scenario (#24, rule 1: an unknown budget counts as unbounded): key 1
   has a balance of 2; key 2 is a row whose val (1) decides how much t1
   draws from key 1 (BuyVal). t1's budget on key 1 is unknown, so its
   reservation is the whole committed balance and it is admitted only
   once no other reservation is live on the key; t2 draws one; t3 sets
   key 2's val to 3, which makes t1's draw unfundable when it runs after
   t3. Admitted first, t1 holds the balance of 2 while drawing 1 and
   leaves one for t2. *)

EXTENDS Kernel

UnknownBudgetProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<BuyVal(1, 2)>>
      [] t = "t2" -> <<Buy(1, 1)>>
      [] t = "t3" -> <<Update(2, 3, 0)>>]

UnknownBudgetInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]
      [] k = 2 -> [present |-> TRUE, val |-> 1, bal |-> 0, parent |-> 0]]

(* Witness: t1 holds the whole balance under an unknown budget while t2
   waits on it. *)
UnknownBudgetUnboundedUnreached ==
  ~(Active("t1") /\ reserved["t1"][1] = 2 /\ WaitsOn("t2") = {"t1"})

(* Witness: after t3 raised the amount, t1's draw of 3 failed on the
   committed balance of 2. *)
UnknownBudgetInsufficientUnreached ==
  ~(status["t3"] = "committed" /\ resp["t1"] = <<"insufficient">>
    /\ db[1].bal = 2)

============================================================================
