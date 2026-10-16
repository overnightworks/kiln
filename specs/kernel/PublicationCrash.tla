--------------------------- MODULE PublicationCrash ---------------------------
(* Scenario (#59): two commits, each touching two derived structures, with
   up to two failures while the engine operates, none during recovery.
   Key 1 is a present parent row.
     t1 inserts key 2 as a child of key 1;
     t2 inserts key 3 as a child of key 1.
   Each commit changes its base row and key 1's reverse-reference set, so
   an installation can stop between the two. A snapshot reader reads both
   structures at the version it captured, during operation and during
   recovery; a failure may land between a durable record and a complete
   installation, and the next recovery rebuilds the pointer below that
   commit, installs it, and only then advances. *)

EXTENDS Publication

PublicationCrashFailures == 2

PublicationProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Insert(2, 1)>>
      [] t = "t2" -> <<Insert(3, 1)>>]

PublicationInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] OTHER -> AbsentRow]

============================================================================
