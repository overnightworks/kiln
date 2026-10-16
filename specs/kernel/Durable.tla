------------------------------ MODULE Durable ------------------------------
(* Failure model for the kernel (issue #51, cut from #6): docs/concept.md
   §5's unknown outcomes and §9.2's storage failure model over the
   admission protocol of Kernel.tla.

   Commit is split into the steps a real engine takes:
     Enqueue(t)  the transaction's commit record enters the volatile tail
     Flush       the oldest tail record becomes durable (fsync succeeds);
                 only now are its effects committed and its footprint and
                 reservations released
     Ack(t)      the caller is told the commit happened, after Flush
   and three failure steps, together at most `Failures` of them per run:
     Crash       the engine stops; the tail is lost except for a prefix
                 that reached storage anyway
     FsyncFail   a flush fails: the engine is fenced (it serves nothing
                 more), the transaction's outcome is unknown, and any
                 prefix of the tail may or may not have reached storage
     Recover     restart from the durable log alone

   The abstraction: the engine does nothing between a failure and
   Recover (a fenced engine serves no requests, §5), and a restart loses
   every volatile transaction, so a transaction that is neither durable
   nor finished is discarded ("aborted") by recovery. Torn writes that
   reach acknowledged bytes are outside this module, and so is the
   publication pointer of §9.1: here a commit is published (the served
   state and the locks) at the very step its record becomes durable.
   Publication.tla takes the two apart over the `Publish`, `AppendToLog`
   and `RecoverTo` pieces below. With `Failures = 0` a run is a Kernel run
   whose commit takes three steps. *)

EXTENDS Kernel

CONSTANT Failures   \* the most failures (crashes and fsync failures) one run takes

VARIABLES
  log,           \* Seq(Record): the durable commit log, in commit order
  unflushed,     \* Seq(Record): appended records not yet durable (volatile)
  history,       \* Seq(Record): the committed history the engine serves; db = Replay(history)
  engine,        \* "up", "fenced" (fsync failed) or "down" (crashed)
  answer,        \* [Txns -> {"none", "committed", "unknown", "aborted"}]: the commit call's response
  fsyncFailed,   \* ghost: every transaction whose commit call met an fsync failure
  unresolved,    \* ghost: the transactions whose outcome is unknown until recovery
  reservedAtUnknown, \* ghost: every transaction's escrow reservations when the last fsync failure struck
  failuresUsed

DVars == <<Vars, log, unflushed, history, engine, answer, fsyncFailed, unresolved, reservedAtUnknown, failuresUsed>>

(* A commit record carries the transaction's effects as Commit applies
   them: tentative rows and escrow deltas. Effects of transactions in the
   tail commute (their footprints are compatible), so replaying records in
   log order rebuilds the committed state. *)
Record(t) == [txn |-> t, wr |-> wr[t], draws |-> draws[t]]

ApplyRecord(rec, rows) ==
  [k \in Keys |->
    LET row == IF k \in DOMAIN rec.wr THEN rec.wr[k] ELSE rows[k] IN
    [row EXCEPT !.bal = @ - rec.draws[k].bal, !.val = @ + rec.draws[k].val]]

RECURSIVE Replay(_)
Replay(records) ==
  IF records = <<>> THEN InitDb
  ELSE ApplyRecord(records[Len(records)], Replay(SubSeq(records, 1, Len(records) - 1)))

TxnsOf(records) == {records[i].txn : i \in 1..Len(records)}

IsPrefix(shorter, longer) ==
  Len(shorter) <= Len(longer) /\ SubSeq(longer, 1, Len(shorter)) = shorter

Persisted(prefixLength) == log \o SubSeq(unflushed, 1, prefixLength)

----------------------------------------------------------------------------
DInit ==
  /\ Init
  /\ log = <<>>
  /\ unflushed = <<>>
  /\ history = <<>>
  /\ engine = "up"
  /\ answer = [t \in Txns |-> "none"]
  /\ fsyncFailed = {}
  /\ unresolved = {}
  /\ reservedAtUnknown = [t \in Txns |-> NoReservation]
  /\ failuresUsed = 0

JournalUnchanged ==
  UNCHANGED <<log, unflushed, history, engine, answer, fsyncFailed, unresolved, reservedAtUnknown, failuresUsed>>

Serving == engine = "up"

(* The unchanged protocol steps, served only while the engine is up. *)
Dispatched(t) == Serving /\ Dispatch(t) /\ JournalUnchanged
Acquired(t) == Serving /\ Acquire(t) /\ JournalUnchanged
Executed(t) == Serving /\ DoOp(t) /\ JournalUnchanged
Restarted(t) == Serving /\ WidenAndRestart(t) /\ JournalUnchanged

Enqueue(t) ==
  /\ Serving /\ Active(t) /\ pc[t] = Len(Prog[t])
  /\ t \notin TxnsOf(unflushed) /\ t \notin TxnsOf(log)
  /\ unflushed' = Append(unflushed, Record(t))
  /\ UNCHANGED <<Vars, log, history, engine, answer, fsyncFailed, unresolved, reservedAtUnknown, failuresUsed>>

(* The oldest record of the tail becomes durable. *)
AppendToLog ==
  /\ log' = Append(log, Head(unflushed))
  /\ unflushed' = Tail(unflushed)

(* A durable commit takes effect: its record joins the served history and
   the committed state, and it releases everything the transaction held
   (strict two-phase locking: held from admission to this step). *)
Publish(rec) ==
  LET t == rec.txn IN
  /\ history' = Append(history, rec)
  /\ db' = ApplyRecord(rec, db)
  /\ status' = [status EXCEPT ![t] = "committed"]
  /\ held' = [held EXCEPT ![t] = {}]
  /\ reserved' = [reserved EXCEPT ![t] = NoReservation]
  /\ wr' = [wr EXCEPT ![t] = NoWrites]
  /\ draws' = [draws EXCEPT ![t] = NoDraws]

(* The flush of the oldest record succeeds: the commit is durable and,
   here, takes effect at once. *)
Flush ==
  /\ Serving /\ unflushed # <<>>
  /\ AppendToLog
  /\ Publish(Head(unflushed))
  /\ UNCHANGED <<seq, pc, resp, widened, engine, answer, fsyncFailed, unresolved, reservedAtUnknown, failuresUsed>>

Ack(t) ==
  /\ Serving /\ status[t] = "committed" /\ answer[t] = "none"
  /\ answer' = [answer EXCEPT ![t] = "committed"]
  /\ UNCHANGED <<Vars, log, unflushed, history, engine, fsyncFailed, unresolved, reservedAtUnknown, failuresUsed>>

FailureBudgetLeft == failuresUsed < Failures

(* The engine stops. Records of the tail may have reached storage anyway,
   as a prefix; the rest are gone. Nothing else changes until Recover. *)
Crash ==
  /\ Serving /\ FailureBudgetLeft
  /\ \E persistedFromTail \in 0..Len(unflushed) :
       log' = Persisted(persistedFromTail)
  /\ unflushed' = <<>>
  /\ engine' = "down"
  /\ failuresUsed' = failuresUsed + 1
  /\ UNCHANGED <<Vars, history, answer, fsyncFailed, unresolved, reservedAtUnknown>>

(* A flush fails (§9.2): the writer is fenced and every transaction in the
   flush is answered "unknown" - the bytes may already be durable. They keep
   their footprint and reservations (§5); recovery resolves them. *)
FsyncFail ==
  /\ Serving /\ FailureBudgetLeft /\ unflushed # <<>>
  /\ \E persistedFromTail \in 0..Len(unflushed) :
       log' = Persisted(persistedFromTail)
  /\ answer' = [t \in Txns |-> IF t \in TxnsOf(unflushed) THEN "unknown" ELSE answer[t]]
  /\ fsyncFailed' = fsyncFailed \cup TxnsOf(unflushed)
  /\ unresolved' = TxnsOf(unflushed)
  /\ reservedAtUnknown' = reserved
  /\ unflushed' = <<>>
  /\ engine' = "fenced"
  /\ failuresUsed' = failuresUsed + 1
  /\ UNCHANGED <<Vars, history>>

Undispatched(t) == Pending(t) /\ seq[t] = 0

(* Restart from the log alone, serving the history `published`, a prefix
   of the log. Every transaction with a record in `published` is
   committed, whether or not its caller was told. A durable record beyond
   it stays as it is, its publication pending. Every other dispatched
   transaction is discarded, while a request not yet made is made on the
   recovered engine. A commit whose caller got no acknowledgement
   - an unflushed append, or a flush that completed before the crash - is
   answered "unknown": the caller learns the outcome from the log. *)
RecoverTo(published) ==
  /\ ~Serving
  /\ history' = published
  /\ db' = Replay(published)
  /\ status' = [t \in Txns |->
       IF t \in TxnsOf(published) THEN "committed"
       ELSE IF t \in TxnsOf(log) THEN status[t]
       ELSE IF Live(t) /\ ~Undispatched(t) THEN "aborted" ELSE status[t]]
  /\ answer' = [t \in Txns |->
       IF answer[t] = "none" /\ (status[t] = "committed" \/ (Active(t) /\ pc[t] = Len(Prog[t])))
       THEN "unknown" ELSE answer[t]]
  /\ held' = [t \in Txns |-> {}]
  /\ reserved' = [t \in Txns |-> NoReservation]
  /\ wr' = [t \in Txns |-> NoWrites]
  /\ draws' = [t \in Txns |-> NoDraws]
  /\ seq' = [t \in Txns |-> 0]
  /\ unresolved' = {}
  /\ unflushed' = <<>>
  /\ engine' = "up"
  /\ UNCHANGED <<pc, resp, widened, log, fsyncFailed, reservedAtUnknown, failuresUsed>>

(* What recovery rebuilds: the committed history is the durable log. *)
Recover == RecoverTo(log)

DTerminating == Finished /\ Serving /\ UNCHANGED DVars

DSteps == \E t \in Txns : Dispatched(t) \/ Acquired(t) \/ Executed(t) \/ Restarted(t) \/ Enqueue(t) \/ Ack(t)

DNext ==
  \/ DSteps
  \/ Flush
  \/ Crash
  \/ FsyncFail
  \/ Recover
  \/ DTerminating

(* Weak fairness for the steps the protocol owes, as in Kernel. Failures
   are never owed; liveness is checked with `Failures = 0`. *)
DFairness ==
  /\ \A t \in Txns : WF_DVars(Acquired(t)) /\ WF_DVars(Executed(t))
       /\ WF_DVars(Restarted(t)) /\ WF_DVars(Enqueue(t))
  /\ WF_DVars(Flush)

DSpec == DInit /\ [][DNext]_DVars /\ DFairness

----------------------------------------------------------------------------
DTypeOK ==
  /\ TypeOK
  /\ engine \in {"up", "fenced", "down"}
  /\ answer \in [Txns -> {"none", "committed", "unknown", "aborted"}]
  /\ fsyncFailed \subseteq Txns /\ unresolved \subseteq fsyncFailed
  /\ reservedAtUnknown \in [Txns -> [Keys -> Nat]]
  /\ failuresUsed \in 0..Failures
  /\ Len(log) = Cardinality(TxnsOf(log))
  /\ db = Replay(history)

(* §9.2: a commit acknowledged to its caller is in the recovered history,
   and in the log recovery reads. *)
AcknowledgedSurvive ==
  \A t \in Txns : answer[t] = "committed" => t \in TxnsOf(history) /\ t \in TxnsOf(log)

(* §9.2: the history the engine serves is a prefix of the log's commit
   order, and once the engine is up it is the whole durable log. *)
HistoryIsLogPrefix == IsPrefix(history, log)

RecoveredPrefix ==
  /\ HistoryIsLogPrefix
  /\ engine = "up" => history = log

(* §5, §9.2: an fsync failure is never answered with an abort. *)
NoKnownAbortAfterFsyncFailure ==
  \A t \in fsyncFailed : answer[t] # "aborted"

(* A known abort never contradicts the log: nothing told or recorded as
   aborted has a durable commit record. *)
KnownAbortNotInLog ==
  \A t \in Txns : (answer[t] = "aborted" \/ status[t] = "aborted") => t \notin TxnsOf(log)

(* §5: an unknown outcome releases nothing until recovery resolves it: the
   footprint is whole and every reservation, a zero one included, is
   exactly what the transaction held when the flush failed. *)
UnknownKeepsReservations ==
  \A t \in unresolved :
    /\ status[t] = "active"
    /\ held[t] = Requested(t)
    /\ reserved[t] = reservedAtUnknown[t]

============================================================================
