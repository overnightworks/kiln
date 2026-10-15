----------------------------- MODULE WidenEscrow -----------------------------
(* Scenario (#46: a restart discards a positive escrow reservation and a
   draw). Key 1 has an escrow balance of 1; key 3 holds no pointer yet.
     t1 buys one ticket on key 1, then follows key 3;
     t2 points key 3 at key 2;
     t3 buys one ticket on key 1.
   t1's guessed footprint is DRAW 1 and S 3. If t2 commits first, t1 has
   reserved the one ticket and drawn it when its follow needs S 2, which
   it does not hold. It restarts: the reservation and the draw are
   discarded, so t3, which waited on that reservation, may now be
   admitted and take the ticket; t1 re-enters behind it and its buy fails
   on the committed state. Whatever the interleaving, every response is
   the one of a serial order. *)

EXTENDS Kernel

WidenEscrowProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Buy(1, 1), Follow(3)>>
      [] t = "t2" -> <<Update(3, 2, 0)>>
      [] t = "t3" -> <<Buy(1, 1)>>]

WidenEscrowInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 1, parent |-> 0]
      [] OTHER -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* About to restart: t1 holds the whole balance reserved and drawn, its
   follow lacks S 2, and t3 waits on the reservation. *)
WidenEscrowDrawnUnreached ==
  ~(Running("t1") /\ ~Covered("t1") /\ reserved["t1"][1] = 1 /\ draws["t1"][1].bal = 1
    /\ Queued("t3") /\ "t1" \in WaitsOn("t3"))

(* Restarted: the reservation and the draw are gone and t1 has widened. *)
WidenEscrowDiscardedUnreached ==
  ~(widened["t1"] # {} /\ Queued("t1")
    /\ reserved["t1"][1] = 0 /\ draws["t1"][1].bal = 0)

============================================================================
