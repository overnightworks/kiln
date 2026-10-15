----------------------------- MODULE WidenCycle -----------------------------
(* Scenario (#46: widen-and-restart where two calls would deadlock if they
   widened in place). Keys 1 and 2 start with no pointer.
     t1 points key 2 at key 1, then follows key 2;
     t2 points key 1 at key 2, then follows key 1.
   Each call's guessed footprint covers only its own update, and each
   discovers its second resource from a pointer it has just written. In
   place, each would hold the key the other still needs: a wait cycle.
   Restarting, each discards its pointer and re-requests the wider
   footprint through admission, which serializes them. *)

EXTENDS Kernel

WidenCycleProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Update(2, 1, 0), Follow(2)>>
      [] t = "t2" -> <<Update(1, 2, 0), Follow(1)>>]

WidenCycleInitDb ==
  [k \in 1..2 |-> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness (negated): both calls widened - each saw a resource the other
   held - and both ended. *)
WidenCycleBothUnreached ==
  ~(widened["t1"] # {} /\ widened["t2"] # {} /\ Finished)

============================================================================
