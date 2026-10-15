---------------------------- MODULE Starvation ----------------------------
(* Scenario (#38: repeated arrivals): three transactions each update key 1
   and, once finished, request the same program again (`RepeatSpec`). The
   X locks conflict, so the three serialize forever. With dispatch-order
   admission an older queued request is never overtaken; without it, two
   transactions can pass the key between them while the third waits in
   every state it could have been admitted (the `NoBypass` mutant). *)

EXTENDS Kernel

StarvationProg == [t \in {"t1", "t2", "t3"} |-> <<Update(1, 0, 0)>>]

StarvationInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness: t3 is queued behind t1, which holds the key, and t2 queued
   earlier is ahead of it. *)
StarvationQueuedBehindUnreached ==
  ~(Active("t1") /\ Queued("t2") /\ Queued("t3") /\ seq["t2"] < seq["t3"])

============================================================================
