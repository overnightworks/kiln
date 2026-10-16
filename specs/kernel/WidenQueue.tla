---------------------------- MODULE WidenQueue ----------------------------
(* Scenario (#46: widen-and-restart while another call is queued behind
   it). Key 1 points at key 2 through its val; keys 2 and 3 are plain rows.
     t1 follows the pointer in key 1 and reads the row it names;
     t2 repoints key 1 at key 3;
     t3 updates key 2, which t1's guessed footprint holds shared.
   t3 queues behind t1 while it runs. When t1 finds it needs key 3 and
   restarts, it releases everything and re-enters the queue at the tail:
   t3, which had waited on it, is admitted ahead of the restarted call
   and is never overtaken by it. *)

EXTENDS Kernel

WidenQueueProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Follow(1)>>
      [] t = "t2" -> <<Update(1, 3, 0)>>
      [] t = "t3" -> <<Update(2, 5, 0)>>]

WidenQueueInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 2, bal |-> 0, parent |-> 0]
      [] OTHER -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness (negated): the restarted call is queued behind t3, the call
   that was waiting on it when it restarted. *)
WidenQueueBehindUnreached ==
  ~(widened["t1"] # {} /\ Queued("t1") /\ Queued("t3") /\ seq["t3"] < seq["t1"])

============================================================================
