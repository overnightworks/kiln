------------------------------ MODULE Phantom ------------------------------
(* Scenario (round-1 phantom, #6): t1 scans keys 2..3 twice under one
   range lock; t2 inserts key 2 inside that range through its unique
   claim. §4: the claim is checked against the covering range, so t2 waits
   on t1 and the second scan repeats the first. *)

EXTENDS Kernel

PhantomProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<Scan(2, 3), Scan(2, 3)>>
      [] t = "t2" -> <<Insert(2, 0)>>]

PhantomInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]
      [] k = 2 -> AbsentRow
      [] k = 3 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness (negated; TLC's counterexample is the reachability proof): the
   claim on 2 waits behind the range lock covering it. *)
PhantomUnreached ==
  ~(Res("RS", 2..3) \in held["t1"] /\ <<"t2", "t1">> \in WaitFor)

============================================================================
