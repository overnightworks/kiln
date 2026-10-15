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

(* t1's flush completed and its caller was not told when the engine
   crashed. t2 had run on t1's committed row, and its record reached
   storage in the crash: recovery serves both as committed, answers both
   `unknown`, and t2's response depends on a commit nobody confirmed. *)
RecoveryDependentSeesUnackedUnreached ==
  ~(failuresUsed >= 1 /\ answer["t1"] = "unknown" /\ status["t1"] = "committed"
    /\ status["t2"] = "committed" /\ resp["t2"][1].val = 5)

(* t1 had run its update (told the initial row, a tentative effect
   complete) and its caller was not told when the crash discarded it; t2,
   not yet requested, then ran on the recovered state, was told the
   initial row and committed. t1's response being the initial row shows
   it ran before t2's commit, so t2 started after the discard. *)
RecoveryDependentIgnoresLostUnreached ==
  ~(failuresUsed >= 1 /\ status["t1"] = "aborted" /\ answer["t1"] = "unknown"
    /\ Len(resp["t1"]) = 1 /\ resp["t1"][1].val = 0
    /\ status["t2"] = "committed" /\ resp["t2"][1].val = 0
    /\ db[1].val = 6)

============================================================================
