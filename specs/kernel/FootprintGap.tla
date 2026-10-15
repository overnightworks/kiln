---------------------------- MODULE FootprintGap ----------------------------
(* Refused scenario (review finding 2): rows 1 and 2 present, row 3 absent.
   t1: Insert(3,1); Update(2,0,0) holds CLAIM 3 + PIN 1 and then wants X 2;
   t2: Update(2,1,0); Delete(3) holds X 2 and then wants DEL 3 (+ PIN 1,
   the parent t1 gives row 3). Ordering only each operation's lowest key
   accepted both programs (1 then 2; 2 then 3) and they deadlocked. Over
   the complete footprint neither is canonical: t1 covers key 2 after key
   3, t2 covers key 1 after key 2. TLC refuses the scenario at
   `ASSUME CanonicalOrder`; the log is the proof. *)

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
