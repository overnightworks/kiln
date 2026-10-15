------------------------------ MODULE Widen ------------------------------
(* Scenario (#46: widen-and-restart, a widened resource that conflicts).
   Key 1 points at key 2 through its val; keys 2 and 3 are plain rows.
     t1 follows the pointer in key 1 and reads the row it names;
     t2 repoints key 1 at key 3;
     t3 updates key 3.
   t1's footprint is guessed at dispatch from InitDb: S on 1 and 2. If t2
   commits first, t1 finds that it needs S on 3 as well. The call cannot
   take it on top of what it holds, so it restarts with the wider
   footprint; if t3 holds key 3 meanwhile, the restarted call waits for
   it. Whatever the interleaving, t1's response is the one of a serial
   order. *)

EXTENDS Kernel

WidenProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Follow(1)>>
      [] t = "t2" -> <<Update(1, 3, 0)>>
      [] t = "t3" -> <<Update(3, 9, 0)>>]

WidenInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 2, bal |-> 0, parent |-> 0]
      [] OTHER -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* A call widened and restarted: the repointing committed first and the
   follow now holds the wider footprint. *)
WidenRestartedUnreached ==
  ~(widened["t1"] # {} /\ Active("t1") /\ Res("S", {3}) \in held["t1"])

(* The restarted call waits for an active holder of the widened resource. *)
WidenConflictUnreached ==
  ~(widened["t1"] # {} /\ Queued("t1") /\ Active("t3") /\ Res("X", {3}) \in held["t3"]
    /\ "t3" \in WaitsOn("t1"))

============================================================================
