------------------------------ MODULE Recovery ------------------------------
(* Scenario (#51): up to two failures, each a crash or an fsync failure,
   anywhere in two escrow draws, a claim and a plain update.
   Key 1 is an event row with a quota of 2; key 2 is absent; key 3 is a
   present row.
     t1 draws one ticket;
     t2 draws one ticket, then inserts key 2;
     t3 updates key 3.
   The two draws coexist, so both commit records can sit in the tail at
   once; a crash or an fsync failure then loses or keeps any prefix of
   them. Whatever the failure, the recovered history is a prefix of the
   log, every acknowledged commit is in it, and an unknown outcome
   resolves to what the log shows. A transaction not yet requested at a
   failure is requested on the recovered engine. *)

EXTENDS Durable

RecoveryFailures == 2

RecoveryProg ==
  [t \in {"t1", "t2", "t3"} |->
    CASE t = "t1" -> <<Buy(1, 1)>>
      [] t = "t2" -> <<Buy(1, 1), Insert(2, 0)>>
      [] t = "t3" -> <<Update(3, 7, 0)>>]

RecoveryInitDb ==
  [k \in 1..3 |->
    CASE k = 1 -> [present |-> TRUE, val |-> 0, bal |-> 2, parent |-> 0]
      [] k = 2 -> AbsentRow
      [] k = 3 -> [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> 0]]

(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* The crash came after the flush made t1's record durable and before the
   caller was told. *)
RecoveryDurableUnackedUnreached ==
  ~(engine = "down" /\ "t1" \in TxnsOf(history) /\ answer["t1"] = "none")

(* After that crash, recovery serves t1's commit although its caller was
   never acknowledged: the caller's answer is unknown. *)
RecoveryUnackedCommittedUnreached ==
  ~(engine = "up" /\ failuresUsed = 1 /\ fsyncFailed = {}
    /\ status["t1"] = "committed" /\ answer["t1"] = "unknown")

(* A flush failed with a transaction waiting on it: the writer is fenced,
   the outcome is unknown and the transaction still holds its escrow
   reservation. *)
RecoveryFencedUnreached ==
  ~(engine = "fenced" /\ "t1" \in unresolved /\ answer["t1"] = "unknown"
    /\ reserved["t1"][1] > 0 /\ Active("t1"))

(* Recovery after an fsync failure resolved the unknown outcome as a
   commit: the record had reached storage despite the failure. *)
RecoveryResolvedCommittedUnreached ==
  ~(engine = "up" /\ "t1" \in fsyncFailed /\ status["t1"] = "committed"
    /\ answer["t1"] = "unknown")

(* Recovery after an fsync failure resolved the unknown outcome as an
   abort: the record had not reached storage. *)
RecoveryResolvedAbortedUnreached ==
  ~(engine = "up" /\ "t1" \in fsyncFailed /\ status["t1"] = "aborted"
    /\ answer["t1"] = "unknown")

============================================================================
