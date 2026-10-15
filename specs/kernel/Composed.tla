----------------------------- MODULE Composed -----------------------------
(* Scenario: every mechanism in one state space. Key 1 is an event row
   (sold = 10, quota = 1) and the parent of key 3; key 2 is absent.
     t1 closes the event under an X lock (observes sold and quota, sets
        both to 0), then scans keys 2..3 under a range lock;
     t2 draws one ticket from the event's escrow, then inserts key 2 as a
        child of key 1 (claim on 2, pin on 1);
     t3 deletes child 3 (exclusive against 3's reverse references, pin on
        its parent 1), then scans key 3.
   X meets DRAW on key 1, the two pins on key 1 are compatible, the claim
   on 2 and the delete of 3 both meet t1's range, and the serial orders
   differ in every response. *)

EXTENDS Kernel

ComposedProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Update(1, 0, 0), Scan(2, 3)>>
      [] t = "t2" -> <<Buy(1, 1), Insert(2, 1)>>
      [] t = "t3" -> <<Delete(3), Scan(3, 3)>>]

ComposedInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 10, bal |-> 1, parent |-> 0]
      [] k = 2 -> AbsentRow
      [] k = 3 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 1]]

============================================================================
