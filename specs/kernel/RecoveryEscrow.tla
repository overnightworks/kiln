---------------------------- MODULE RecoveryEscrow ----------------------------
(* Scenario (#51): what an unknown outcome keeps, reservations included.
   Key 1 is an event row with a balance of 2, key 2 an event row with an
   empty balance.
     t1 runs BuyUnder(1,1,1) twice: it draws one ticket, the second
        branch is skipped, and its reservation is 2 against a draw of 1
        (an unused part);
     t2 runs BuyUnder(2,1,0): the branch is never taken and the balance is
        empty, so its reservation is 0, legitimately.
   A flush that fails with both records in the tail leaves the engine
   fenced and both outcomes unknown; until recovery each transaction must
   hold exactly the reservation it held when the flush failed, the unused
   part of t1 and the zero of t2 included. *)

EXTENDS Durable

RecoveryEscrowFailures == 1

RecoveryEscrowProg ==
  [t \in {"t1", "t2"} |->
    CASE t = "t1" -> <<BuyUnder(1, 1, 1), BuyUnder(1, 1, 1)>>
      [] t = "t2" -> <<BuyUnder(2, 1, 0)>>]

RecoveryEscrowInitDb ==
  [k \in 1..2 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]
      [] k = 2 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witness: the fenced engine with both outcomes unknown, t1 holding its
   unused reservation (2 against a draw of 1) and t2 its zero one. *)
RecoveryEscrowFencedUnreached ==
  ~(engine = "fenced" /\ unresolved = {"t1", "t2"}
    /\ reserved["t1"][1] = 2 /\ draws["t1"][1].bal = 1
    /\ reserved["t2"][2] = 0 /\ resp["t2"] = <<"skipped">>)

============================================================================
