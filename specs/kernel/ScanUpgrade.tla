---------------------------- MODULE ScanUpgrade ----------------------------
(* Review finding 6: t1 and t2 each scan keys 1..2 and then update a
   different key the scan covers. Registered per operation, the two range
   locks coexist and each update then wants an X lock on a key the
   other's range covers: the lock-upgrade deadlock, which the
   PerOperationAcquisition mutant reproduces. With the complete footprint
   acquired at dispatch each transaction takes its range lock and its X
   lock together; the X lock conflicts with the other's range, so the
   programs serialize. The restart rule for a class-4 call whose covering
   resource must widen mid-execution (§4) is outside this slice. *)

EXTENDS Kernel

ScanUpgradeProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Scan(1, 2), Update(1, 0, 0)>>
      [] t = "t2" -> <<Scan(1, 2), Update(2, 0, 0)>>]

ScanUpgradeInitDb ==
  [k \in 1..2 |-> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

============================================================================
