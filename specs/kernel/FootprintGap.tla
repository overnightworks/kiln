---------------------------- MODULE FootprintGap ----------------------------
(* Review finding 2: rows 1 and 2 present, row 3 absent. Registered per
   operation, t1: Insert(3,1); Update(2,0,0) holds CLAIM 3 + PIN 1 and
   then wants X 2 while t2: Update(2,1,0); Delete(3) holds X 2 and then
   wants DEL 3 (+ PIN 1, the parent t1 gives row 3): a deadlock an
   ordering over each operation's lowest key admitted, which the
   PerOperationAcquisition mutant reproduces. With the complete footprint
   acquired at dispatch the X locks on 2 conflict and the programs
   serialize: insert first, the delete is rejected with "referenced";
   update first, the insert commits over the updated row. *)

EXTENDS Kernel

FootprintGapProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Insert(3, 1), Update(2, 0, 0)>>
      [] t = "t2" -> <<Update(2, 1, 0), Delete(3)>>]

FootprintGapInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] k = 2 -> [present |-> TRUE, val |-> 1, bal |-> 0, parent |-> 0]
      [] k = 3 -> AbsentRow]

============================================================================
