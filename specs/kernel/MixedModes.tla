----------------------------- MODULE MixedModes -----------------------------
(* Scenario (#24: X and DRAW on one key in one footprint): key 1 has a
   balance of 1. t1 sets the balance to 2 and then draws one; t2 draws one
   and then closes the row (balance 0). Each holds X and DRAW on key 1,
   the X locks conflict, so the two serialize. t1's budget (1) exceeds the
   committed balance it is admitted against when t2 commits first (0), so
   its reservation is capped at 0; as the key's only draw holder it draws
   from the balance its own update raised, the one case a draw exceeds
   the reservation. *)

EXTENDS Kernel

MixedModesProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Update(1, 0, 2), Buy(1, 1)>>
      [] t = "t2" -> <<Buy(1, 1), Update(1, 5, 0)>>]

MixedModesInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 1, parent |-> 0]]

(* Witness: t1 drew one against a reservation of 0 under its X lock. *)
MixedModesRaisedUnreached ==
  ~(Active("t1") /\ reserved["t1"][1] = 0 /\ draws["t1"][1].bal = 1)

============================================================================
