--------------------------- MODULE PublicationFree ---------------------------
(* Scenario (#59): the PublicationCrash programs with no failure. Installation
   still stops between a commit's two structures while a snapshot reader reads
   at the pointer, and the pointer passes each commit only when it is whole. *)

EXTENDS Publication

PublicationFreeFailures == 0

PublicationProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Insert(2, 1)>>
      [] t = "t2" -> <<Insert(3, 1)>>]

PublicationInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] OTHER -> AbsentRow]

============================================================================
