---------------------------- MODULE OutOfOrder ----------------------------
(* t2's program touches key 2 before key 1 while t1's touches 1 before 2.
   Registered per operation, t1's tentative draw exhausts the allowance
   t2's draw needs (quota 1) while t1 waits on t2's claim: a wait cycle
   through a non-exclusive escrow holder (§9.5), which the
   PerOperationAcquisition mutant reproduces. With the complete footprint
   acquired at dispatch the order of a program's operations is
   immaterial: the two claims on 2 conflict, so the programs serialize
   and the second draw is rejected with "insufficient". *)

EXTENDS Kernel

OutOfOrderProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Buy(1, 1), Insert(2, 0)>>
      [] t = "t2" -> <<Insert(2, 0), Buy(1, 1)>>]

OutOfOrderInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 1, parent |-> 0]
      [] k = 2 -> AbsentRow]

============================================================================
