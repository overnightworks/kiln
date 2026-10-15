-------------------------- MODULE ConcurrentBranch --------------------------
(* Scenario (#24: reservations cover blind draws only): key 1 has a balance
   of 2 and nothing sold. t1 and t2 each run BuyUnder(1,1,1) once: a draw
   taken only while no ticket is sold. Their reservations of 1 and 1 are
   both funded, so a protocol that protects only the draw would run them
   side by side, both would observe "nothing sold" and both draw. Serially
   the second one sees the first one's ticket and skips; the branch reads
   the row, so it is ordered. *)

EXTENDS Kernel

ConcurrentBranchProg ==
  [t \in {"t1", "t2"} |-> <<BuyUnder(1, 1, 1)>>]

ConcurrentBranchInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]]

(* Witness: one transaction drew and the other skipped its branch. *)
ConcurrentBranchSkippedUnreached ==
  ~(status["t1"] = "committed" /\ status["t2"] = "committed"
    /\ {resp["t1"], resp["t2"]} = {<<"ok">>, <<"skipped">>} /\ db[1].bal = 1)

============================================================================
