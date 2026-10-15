---------------------------- MODULE OutOfOrder ----------------------------
(* Scenario for the deadlock mutant: t2 registers its resources against
   canonical order (claim on 2, then a draw on 1). With key 1's quota at
   1, t1's tentative draw exhausts the allowance t2's draw needs while t1
   waits on t2's claim: a wait cycle through a non-exclusive escrow
   holder (§9.5). The unmodified Kernel rejects this program through its
   CanonicalOrder assumption; only the NoCanonicalOrder mutant runs it. *)

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
