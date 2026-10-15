---------------------------- MODULE EventClose ----------------------------
(* Scenario (#16): key 1 is an event row with sold = 10 and quota = 1.
   t1 takes an X lock on the row, observes sold and quota and sets quota
   to 0; t2 and t3 each draw one ticket from the quota's escrow. The X
   lock and a draw are incompatible on the equal key, so whichever is
   first orders the other; the two draws share the allowance, so the
   second waits on the first's outcome and is rejected once it commits. *)

EXTENDS Kernel

EventCloseProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Update(1, 10, 0)>>
      [] t = "t2" -> <<Buy(1, 1)>>
      [] t = "t3" -> <<Buy(1, 1)>>]

EventCloseInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 10, bal |-> 1, parent |-> 0]
      [] k = 2 -> AbsentRow]

(* Witness: t2 holds its tentative draw while t1's X lock waits on it and
   t3's draw waits on the exhausted allowance. *)
EventCloseUnreached ==
  ~(Res("DRAW", {1}) \in held["t2"]
    /\ <<"t1", "t2">> \in WaitFor /\ <<"t3", "t2">> \in WaitFor)

============================================================================
