--------------------------- MODULE AllowanceCycle ---------------------------
(* Documented failing scenario (review finding 1; concept defect, #24):
   key 1 has an escrow balance of 2, and t1 and t2 each run Buy(1,1)
   twice. Both programs are canonical and upgrade-free (one key, one
   mode), so the scenario is admitted. The first draws coexist, as §5
   allows. Each second draw fits its holder's own view (balance 1) but not
   the allowance left after the other's tentative draw (0), so each waits
   on the other: the non-exclusive wait cycle §9.5 fears, inside canonical
   order. TLC reports `NoWaitCycle` violated and then the deadlock; the
   log is the counterexample. This scenario is not part of the passing
   set; the protocol decision (aggregate reservation, arbitration, or
   another mechanism) is #24's. *)

EXTENDS Kernel

AllowanceCycleProg ==
  [t \in {"t1", "t2"} |-> <<Buy(1, 1), Buy(1, 1)>>]

AllowanceCycleInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]]

============================================================================
