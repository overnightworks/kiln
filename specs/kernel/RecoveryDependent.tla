--------------------------- MODULE RecoveryDependent ---------------------------
(* Scenario (#51): a response that depends on another transaction, across
   failures. Key 1 is a present row.
     t1 updates it to 5 and is told the previous row;
     t2 updates it to 6 and is told the previous row;
     t3 reads it.
   The three conflict on key 1 and serialize behind the first admitted.
   Whatever a failure keeps, t2's and t3's responses must be exactly what
   a serial run of the committed transactions gives: a predecessor whose
   record is durable but whose caller was not acknowledged is seen by its
   successors after recovery, a predecessor whose record was lost is not.
   Up to two failures, crashes or fsync failures. *)

EXTENDS Durable

RecoveryDependentFailures == 2

RecoveryDependentProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Update(1, 5, 0)>>
      [] t = "t2" -> <<Update(1, 6, 0)>>
      [] t = "t3" -> <<Read(1)>>]

RecoveryDependentInitDb ==
  [k \in 1..1 |-> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witnesses. *)

(* t1's record was durable and its caller unacknowledged when the engine
   crashed; after recovery t2 committed and was told t1's row: its
   response depends on a commit nobody confirmed. *)
RecoveryDependentSeesUnackedUnreached ==
  ~(failuresUsed >= 1 /\ answer["t1"] = "unknown" /\ status["t1"] = "committed"
    /\ status["t2"] = "committed" /\ resp["t2"][1].val = 5)

(* t1's record was lost with the crash; after recovery t2 committed and
   was told the initial row: it never saw t1's discarded effects. *)
RecoveryDependentIgnoresLostUnreached ==
  ~(failuresUsed >= 1 /\ status["t1"] = "aborted"
    /\ status["t2"] = "committed" /\ resp["t2"][1].val = 0
    /\ db[1].val = 6)

============================================================================
