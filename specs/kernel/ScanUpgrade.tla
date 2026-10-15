---------------------------- MODULE ScanUpgrade ----------------------------
(* Refused scenario (review finding 6): t1 and t2 each scan keys 1..2 and
   then update a different key the scan covers. The two range locks are
   compatible, so both scans run; each update then wants an X lock on a
   key the other's range covers, and the two wait on each other. The
   programs are in key order, but each needs a stronger mode (X) on a key
   it already holds in a weaker one (RS): TLC refuses the scenario at
   `ASSUME NoUpgrade`; the log is the proof. Whether the protocol detects,
   restarts or forbids such an upgrade is decided in #24. *)

EXTENDS Kernel

ScanUpgradeProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Scan(1, 2), Update(1, 0, 0)>>
      [] t = "t2" -> <<Scan(1, 2), Update(2, 0, 0)>>]

ScanUpgradeInitDb ==
  [k \in 1..2 |-> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

============================================================================
