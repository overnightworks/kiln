---------------------------- MODULE RangePoint ----------------------------
(* Scenario: a range lock and a point lock held together against an
   update and a draw. Keys 1..3 are present; key 3 has a balance of 1.
     t1 scans the whole table (a table resource);
     t2 reads key 2 (S point lock), then draws from key 3's escrow;
     t3 updates key 2 (X point lock).
   The table scan and the point read coexist; the update waits on both
   and t2's draw waits on the scan covering key 3. Scan and read see
   key 2's value before or after the update, in one consistent order. *)

EXTENDS Kernel

RangePointProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Scan(1, 3)>>
      [] t = "t2" -> <<Read(2), Buy(3, 1)>>
      [] t = "t3" -> <<Update(2, 1, 0)>>]

RangePointInitDb ==
  [k \in 1..3 |->
    CASE k = 3 -> [present |-> TRUE, val |-> 0, bal |-> 1, parent |-> 0]
      [] OTHER -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness (negated; TLC's counterexample is the reachability proof): the
   scan and the read are both live, the update waits on both, and the draw
   waits on the scan. *)
RangePointUnreached ==
  ~(Res("RS", 1..3) \in held["t1"] /\ Res("S", {2}) \in held["t2"]
    /\ WaitsOn("t3") = {"t1", "t2"} /\ WaitsOn("t2") = {"t1"})

============================================================================
