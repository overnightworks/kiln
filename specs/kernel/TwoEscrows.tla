----------------------------- MODULE TwoEscrows -----------------------------
(* Scenario (#24, rule 1: admission is all-or-nothing across escrows):
   keys 1 and 2 each hold a balance of 1. t1 draws one from each; t2 draws
   one from key 2 and t3 one from key 1. While t2 or t3 holds its
   reservation, t1 cannot fund its whole footprint and waits holding no
   reservation on either key, so the other key stays available to the
   transaction that needs only it. Once both have committed t1 is capped
   at 0 on both keys and its first draw fails on committed state. *)

EXTENDS Kernel

TwoEscrowsProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Buy(1, 1), Buy(2, 1)>>
      [] t = "t2" -> <<Buy(2, 1)>>
      [] t = "t3" -> <<Buy(1, 1)>>]

TwoEscrowsInitDb ==
  [k \in 1..2 |-> [present |-> TRUE, val |-> 0, bal |-> 1, parent |-> 0]]

(* Witness: t2 and t3 each hold one key's reservation; t1 waits on both
   and holds nothing. *)
TwoEscrowsAllOrNothingUnreached ==
  ~(reserved["t2"][2] = 1 /\ reserved["t3"][1] = 1
    /\ Queued("t1") /\ WaitsOn("t1") = {"t2", "t3"})

============================================================================
