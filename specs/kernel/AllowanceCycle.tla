--------------------------- MODULE AllowanceCycle ---------------------------
(* Scenario (#24, the concept defect slice 1 found): key 1 has an escrow
   balance of 2, and t1 and t2 each run Buy(1,1) twice. Both footprints
   are one DRAW on 1 and coexist. Under slice 1's protocol the first draws
   coexisted and each second draw fit its holder's own view (balance 1)
   but not the allowance left after the other's tentative draw (0), so
   each waited on the other: a wait cycle between holders. With aggregate
   escrow budgets each program reserves min(2, 2) = 2 at admission, so the
   second transaction is admitted only once the first has resolved; it is
   then capped at the remaining balance 0 and its first draw fails with
   `insufficient`, justified by the committed state the first commit
   left. *)

EXTENDS Kernel

AllowanceCycleProg ==
  [t \in {"t1", "t2"} |-> <<Buy(1, 1), Buy(1, 1)>>]

AllowanceCycleInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]]

(* Witness, the queued case: t1 holds a reservation of 2 (nothing drawn
   yet) and t2 is queued behind it on the escrow alone. *)
AllowanceCycleQueuedUnreached ==
  ~(reserved["t1"][1] = 2 /\ draws["t1"][1].bal = 0
    /\ Queued("t2") /\ WaitsOn("t2") = {"t1"})

============================================================================
