------------------------------- MODULE Pins -------------------------------
(* Scenario: concurrent pins on one parent against its delete, and claims
   against a table scan. Key 1 is a present row with no children; keys 2
   and 3 are absent.
     t1 deletes key 1 (exclusive against 1's reverse references), then
        scans the whole table;
     t2 inserts key 2 as a child of 1 (claim on 2, pin on 1);
     t3 inserts key 3 as a child of 1 (claim on 3, pin on 1).
   The two pins are compatible and coexist; the delete then waits on both
   and is rejected with "referenced" once they commit. Delete first makes
   both inserts wait (pin against the delete, then claim against the table
   scan) and rejects them with "noparent". *)

EXTENDS Kernel

PinsProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Delete(1), Scan(1, 3)>>
      [] t = "t2" -> <<Insert(2, 1)>>
      [] t = "t3" -> <<Insert(3, 1)>>]

PinsInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] k = 2 -> AbsentRow
      [] k = 3 -> AbsentRow]

(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* Both pins on 1 are live at once and the delete waits on both. *)
PinsConcurrentUnreached ==
  ~(Res("PIN", {1}) \in held["t2"] /\ Res("PIN", {1}) \in held["t3"]
    /\ WaitsOn("t1") = {"t2", "t3"})

(* t1 holds the table scan after its delete; both claims wait on it. *)
PinsTableUnreached ==
  ~(Res("RS", 1..3) \in held["t1"]
    /\ WaitsOn("t2") = {"t1"} /\ WaitsOn("t3") = {"t1"})

(* The pins won: the delete was rejected and both children are committed. *)
PinsReferencedUnreached ==
  ~(resp["t1"] = <<"referenced">> /\ db[2].parent = 1 /\ db[3].parent = 1)

============================================================================
