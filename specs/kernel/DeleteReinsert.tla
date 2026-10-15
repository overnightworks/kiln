--------------------------- MODULE DeleteReinsert ---------------------------
(* Regression (delta review of #16): rows 1 and 2 present with no parent.
   t1 deletes row 2 and reinserts it under parent 1; t2 deletes row 1 and
   updates row 2. The reinsert makes 1 a possible parent of 2, so t1's
   static footprint pins 1 - a pin its delete never acquired when
   resources were registered per operation. Credited with that pin, t1
   looked ordered (1 before 2) and upgrade-free, and after both deletes
   t1 held DEL 2 wanting PIN 1 while t2 held DEL 1 wanting X 2: an
   exclusive-resource deadlock the ordering assumptions admitted. With the
   complete footprint acquired at dispatch the two programs serialize;
   the PerOperationAcquisition mutant reproduces the deadlock. *)

EXTENDS Kernel

DeleteReinsertProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Delete(2), Insert(2, 1)>>
      [] t = "t2" -> <<Delete(1), Update(2, 0, 0)>>]

DeleteReinsertInitDb ==
  [k \in 1..2 |-> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

============================================================================
