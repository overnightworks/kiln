---------------------------- MODULE ClaimAbort ----------------------------
(* Scenario (#6 critique: a tentative claim whose holder aborts): t1
   claims key 2 and then draws from key 3's empty escrow, which rejects it
   and releases the claim; t2 and t3 claim key 2 as well. §5: both wait
   on t1's outcome; after the rejection they proceed as if 2 had never
   been claimed, one wins, and the other is rejected with "exists" only
   once the winner has committed. *)

EXTENDS Kernel

ClaimAbortProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Insert(2, 0), Buy(3, 1)>>
      [] t = "t2" -> <<Insert(2, 0)>>
      [] t = "t3" -> <<Insert(2, 0)>>]

ClaimAbortInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> AbsentRow
      [] k = 2 -> AbsentRow
      [] k = 3 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness, in two states of one run: t2 waits on t1's tentative claim,
   and later t1 is rejected while t2 has committed the row and t3 has
   been rejected with "exists". *)
ClaimAbortWaitUnreached ==
  ~(Res("CLAIM", {2}) \in held["t1"] /\ <<"t2", "t1">> \in WaitFor)

ClaimAbortUnreached ==
  ~(resp["t1"] = <<"ok", "insufficient">> /\ status["t2"] = "committed"
    /\ db[2].present /\ resp["t3"] = <<"exists">>)

============================================================================
