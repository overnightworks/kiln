--------------------------- MODULE ParentDelete ---------------------------
(* Scenario (§5 foreign keys): t1 deletes parent key 1 (no committed
   children) while t2 inserts key 2 as a child of 1. The delete's
   exclusive reservation against 1's reverse-reference set and the
   child's pin on 1 are incompatible, so one orders the other: delete
   first rejects the insert with "noparent", insert first rejects the
   delete with "referenced". *)

EXTENDS Kernel

ParentDeleteProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Delete(1)>>
      [] t = "t2" -> <<Insert(2, 1)>>]

ParentDeleteInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] k = 2 -> AbsentRow]

(* Witness: the child insert holds its pin and the parent delete waits. *)
ParentDeleteUnreached ==
  ~(Res("PIN", {1}) \in held["t2"] /\ <<"t1", "t2">> \in WaitFor)

============================================================================
