------------------------- MODULE PublicationDependent -------------------------
(* Scenario (#59): protocol transactions that depend on a commit still being
   installed, with up to two failures while the engine operates, none during
   recovery. Key 1 is a present parent.
     t1 inserts key 2 as a child of key 1 (a base row and key 1's
        reverse-reference set to install);
     t2 deletes key 1, which checks the reverse references of key 1;
     t3 reads key 2.
   Once t1's record is durable, t2's reference check and t3's read run
   against it while its structures are being installed - unless t1's
   locks hold them back until the pointer passes t1. Serializable compares
   every response exactly; ReaderConsistent covers a snapshot reader. *)

EXTENDS Publication

PublicationDependentFailures == 2

PublicationProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Insert(2, 1)>>
      [] t = "t2" -> <<Delete(1)>>
      [] t = "t3" -> <<Read(2)>>]

PublicationInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] OTHER -> AbsentRow]

----------------------------------------------------------------------------
(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* t1's record is durable and unpublished, and t2 and t3 both wait on it. *)
DependentWaitsOnDurableUnreached ==
  ~("t1" \in TxnsOf(log) /\ "t1" \notin VisibleTxns
    /\ "t1" \in WaitsOn("t2") /\ "t1" \in WaitsOn("t3"))

(* The reference check ran after t1 was published: t2 was told "referenced". *)
DependentReferencedUnreached ==
  ~(resp["t2"] = <<"referenced">> /\ status["t1"] = "committed")

(* The read ran after t1 was published: t3 was told the child row. *)
DependentReadsPublishedUnreached ==
  ~(status["t3"] = "committed" /\ resp["t3"][1].present)

============================================================================
