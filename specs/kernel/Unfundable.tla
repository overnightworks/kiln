----------------------------- MODULE Unfundable -----------------------------
(* Scenario (#24, rule 2: a capped reservation fails only on committed
   state): key 1 has a balance of 2. t1's budget is 3 (Buy(1,2) then
   Buy(1,1)), more than the committed balance, so its reservation is
   capped at the balance and it is admitted only once no other reservation
   is live on the key; t2 draws one. Admitted first, t1 draws 2 from its
   reservation and the third ticket fails with `insufficient` on the
   committed state alone; t2 then takes the one t1's rejection left.
   Admitted after t2's commit, t1 is capped at 1 and its first draw fails. *)

EXTENDS Kernel

UnfundableProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Buy(1, 2), Buy(1, 1)>>
      [] t = "t2" -> <<Buy(1, 1)>>]

UnfundableInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]]

(* Witness, the capped case: t1 holds a reservation equal to the committed
   balance and below its budget, and t2 is queued behind it. *)
UnfundableCappedUnreached ==
  ~(Active("t1") /\ Capped("t1", 1) /\ reserved["t1"][1] = db[1].bal
    /\ db[1].bal = 2 /\ WaitsOn("t2") = {"t1"})

(* Witness, the justified failure: t1 drew its reservation and was told
   `insufficient` for the rest; t2 committed the ticket the rejection
   released. *)
UnfundableInsufficientUnreached ==
  ~(resp["t1"] = <<"ok", "insufficient">> /\ status["t1"] = "rejected"
    /\ status["t2"] = "committed" /\ db[1].bal = 1)

============================================================================
