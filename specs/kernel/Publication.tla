---------------------------- MODULE Publication ----------------------------
(* Publication boundary for the kernel (issue #59, cut from #6):
   docs/concept.md §9.1 over the durable engine of Durable.tla.

   A commit's record becoming durable (Flush) proves it recoverable, not
   that anyone may see it. Its derived structures (`Structures`: the base
   row and the reverse-reference set of §5) are installed one at a time
   afterwards, and a single visible-version `pointer` into the log says
   how much of the log is published:
     PFlush         the oldest tail record becomes durable; the commit
                    keeps its locks and reservations and changes no state
     Install(t, s)  structure s of durable commit t is installed; any
                    logged commit may be installed, ahead of the pointer
     Advance        the pointer passes the next commit, only when every
                    structure of it is installed; the commit is published:
                    it takes effect in the served state `db` and releases
                    its locks and reservations
     RecoverPublication
                    restart from the log; the pointer is recomputed as the
                    largest prefix of the log whose commits are all fully
                    installed, never read from the retained pointer
     Open           recovery ends, only once the pointer has passed the
                    whole log
   Installed structures and the pointer are durable; a crash keeps both.

   Every read sees published state: protocol transactions read `db`, which
   is the log up to the pointer (`DbIsPublished`) and, through
   PointerBelowInstalled, only installed structures, plus their own
   writes; snapshot readers (`reader`) capture the pointer and read the
   installed structures at that version over several steps. A commit's
   locks outlive its durability, so a conflicting transaction cannot
   observe it before it is published. An acknowledgement follows
   publication (Durable's Ack), so an acknowledged commit is visible to
   every later reader. Outside this module: several logs, byte layout,
   and a failure during recovery (failures strike while the engine
   operates, never between RecoverPublication and Open). *)

EXTENDS Durable

Structures == {"row", "refs"}

VARIABLES
  installed,    \* [Txns -> SUBSET Structures]: the structures of each durable commit that are installed
  pointer,      \* 0..Len(log): the visible version; log[1..pointer] is published
  recovering,   \* BOOLEAN: recovery has started and has not yet passed the whole log
  reader,       \* the snapshot reader: its captured version and what it has observed so far
  interrupted,  \* ghost: the commits that were not completely installed at the last failure
  pointerAtFailure \* ghost: the pointer at the last failure

PVars == <<DVars, installed, pointer, recovering, reader, interrupted, pointerAtFailure>>

(* What a reader makes of a structure holding `state`: the base rows
   themselves, or for each key its set of referencing children. *)
Content(s, state) == IF s = "row" THEN state ELSE [k \in Keys |-> Refs(state, k)]

(* The snapshot reader: one session reads each structure once at the
   version it captured, then ends. A failure ends the session. *)
NoSnapshot == -1

IdleReader ==
  [version |-> NoSnapshot,
   read |-> {},
   seen |-> [s \in Structures |-> Content(s, InitDb)],
   hidden |-> [s \in Structures |-> {}]]

Published == SubSeq(log, 1, pointer)

VisibleTxns == TxnsOf(Published)

CompletelyInstalled(t) == installed[t] = Structures

(* The commits a reader at the pointer sees in structure s. *)
VisibleIn(s) == {log[i].txn : i \in {j \in 1..pointer : s \in installed[log[j].txn]}}

(* The state a structure holds for the commits among log[1..n] whose copy
   in it is installed. *)
RECURSIVE InstalledState(_, _)
InstalledState(s, n) ==
  IF n = 0 THEN InitDb
  ELSE LET before == InstalledState(s, n - 1) IN
       IF s \in installed[log[n].txn] THEN ApplyRecord(log[n], before) ELSE before

(* What a reader at `version` must see in structure s: all commits up to it. *)
AsOf(s, version) == Content(s, Replay(SubSeq(log, 1, version)))

(* Commits beyond `version` with a copy already in s while incomplete: the
   installation in progress that a reader at `version` must not see. *)
InFlightBeyond(s, version) ==
  {t \in TxnsOf(log) \ TxnsOf(SubSeq(log, 1, version)) :
     s \in installed[t] /\ ~CompletelyInstalled(t)}

----------------------------------------------------------------------------
PInit ==
  /\ DInit
  /\ installed = [t \in Txns |-> {}]
  /\ pointer = 0
  /\ recovering = FALSE
  /\ reader = IdleReader
  /\ interrupted = {}
  /\ pointerAtFailure = 0

(* The record is durable: the commit is recoverable. Everything the
   transaction holds stays held and `db` stays as it was. *)
PFlush ==
  /\ Serving /\ unflushed # <<>>
  /\ AppendToLog
  /\ UNCHANGED <<Vars, history, engine, answer, fsyncFailed, unresolved, reservedAtUnknown, failuresUsed>>

Install(t, s) ==
  /\ Serving /\ t \in TxnsOf(log) /\ s \notin installed[t]
  /\ installed' = [installed EXCEPT ![t] = @ \cup {s}]
  /\ UNCHANGED <<DVars, pointer, recovering, reader, interrupted, pointerAtFailure>>

Advance ==
  /\ Serving /\ pointer < Len(log)
  /\ CompletelyInstalled(log[pointer + 1].txn)
  /\ pointer' = pointer + 1
  /\ Publish(log[pointer + 1])
  /\ UNCHANGED <<seq, pc, resp, widened, log, unflushed, engine, answer, fsyncFailed, unresolved,
                 reservedAtUnknown, failuresUsed, installed, recovering, reader, interrupted,
                 pointerAtFailure>>

(* The pointer after a restart: from the log and the installation state
   alone, the end of the longest prefix of fully installed commits. *)
FullyInstalledPrefixes == {n \in 0..Len(log) : \A i \in 1..n : CompletelyInstalled(log[i].txn)}

Reconstructed == CHOOSE n \in FullyInstalledPrefixes : \A m \in FullyInstalledPrefixes : m <= n

RecoverPublication ==
  /\ RecoverTo(SubSeq(log, 1, Reconstructed))
  /\ pointer' = Reconstructed
  /\ recovering' = TRUE
  /\ UNCHANGED <<installed, reader, interrupted, pointerAtFailure>>

Open ==
  /\ recovering /\ pointer = Len(log)
  /\ recovering' = FALSE
  /\ UNCHANGED <<DVars, installed, pointer, reader, interrupted, pointerAtFailure>>

BeginRead ==
  /\ Serving /\ reader.version = NoSnapshot
  /\ reader' = [IdleReader EXCEPT !.version = pointer]
  /\ UNCHANGED <<DVars, installed, pointer, recovering, interrupted, pointerAtFailure>>

ReadStructure(s) ==
  /\ Serving /\ reader.version # NoSnapshot /\ s \notin reader.read
  /\ reader' = [reader EXCEPT
       !.read = @ \cup {s},
       !.seen[s] = Content(s, InstalledState(s, reader.version)),
       !.hidden[s] = InFlightBeyond(s, reader.version)]
  /\ UNCHANGED <<DVars, installed, pointer, recovering, interrupted, pointerAtFailure>>

EndRead ==
  /\ Serving /\ reader.version # NoSnapshot
  /\ reader.read = Structures
  /\ reader' = IdleReader
  /\ UNCHANGED <<DVars, installed, pointer, recovering, interrupted, pointerAtFailure>>

(* Durable's steps, with PFlush for Flush, while the engine serves and is
   not recovering. A failure ends the reader's session and records which
   commits it left incompletely installed. *)
Operate ==
  /\ Serving /\ ~recovering
  /\ (DSteps \/ PFlush \/ Crash \/ FsyncFail \/ DTerminating)
  /\ UNCHANGED <<installed, pointer, recovering>>
  /\ IF failuresUsed' > failuresUsed
     THEN /\ interrupted' = {t \in TxnsOf(log') : ~CompletelyInstalled(t)}
          /\ pointerAtFailure' = pointer
          /\ reader' = IdleReader
     ELSE UNCHANGED <<interrupted, pointerAtFailure, reader>>

PNext ==
  \/ Operate
  \/ \E t \in Txns, s \in Structures : Install(t, s)
  \/ Advance
  \/ RecoverPublication
  \/ Open
  \/ BeginRead
  \/ \E s \in Structures : ReadStructure(s)
  \/ EndRead

PSpec == PInit /\ [][PNext]_PVars

----------------------------------------------------------------------------
PTypeOK ==
  /\ DTypeOK
  /\ installed \in [Txns -> SUBSET Structures]
  /\ \A t \in Txns : installed[t] # {} => t \in TxnsOf(log)
  /\ pointer \in 0..Len(log)
  /\ recovering \in BOOLEAN
  /\ reader.version \in {NoSnapshot} \cup 0..Len(log)
  /\ interrupted \subseteq TxnsOf(log)
  /\ pointerAtFailure \in 0..Len(log)

(* §9.1: the pointer never passes a commit with a structure missing. *)
PointerBelowInstalled ==
  \A i \in 1..pointer : CompletelyInstalled(log[i].txn)

(* §9.1: the visible version never falls back, across recovery too. *)
PointerMonotone == [][pointer' >= pointer]_PVars

(* The served history is the published part of the log, and the committed
   state is the log up to the pointer: a protocol read sees published
   state, nothing a durable but unpublished commit did. *)
DbIsPublished ==
  /\ history = Published
  /\ db = Replay(Published)

(* #51's RecoveredPrefix, restated for a pointer: the served history is a
   prefix of the log, and recovery ends only on the whole log. *)
RecoveryOpensOnWholeLog == [][Open => history' = log']_PVars

(* An acknowledged commit is published, so every reader that begins
   after the acknowledgement sees it. *)
AcknowledgedAreVisible ==
  \A t \in Txns : answer[t] = "committed" => t \in VisibleTxns

(* A published commit has taken effect and let go; an unpublished one
   still holds its footprint, so no later transaction can have read it. *)
PublishedAreCommitted ==
  \A t \in Txns : (t \in VisibleTxns => status[t] = "committed")
                  /\ (t \in TxnsOf(log) \ VisibleTxns => status[t] = "active" /\ pc[t] = Len(Prog[t]))

(* §9.1: whenever the engine serves, each commit shows in every structure
   or in none - never in some but not all. *)
AtomicVisibility ==
  Serving =>
    \A t \in TxnsOf(log) :
      LET shownIn == {s \in Structures : t \in VisibleIn(s)} IN
      shownIn = {} \/ shownIn = Structures

(* §9.1: every observation of a snapshot reader is the structure as of
   the version it captured, however many steps its reads span and
   whatever is being installed or published meanwhile. *)
ReaderConsistent ==
  \A s \in Structures :
    s \in reader.read => reader.seen[s] = AsOf(s, reader.version)

(* The restart itself exposes nothing partial: the first reader after a
   failure sees at the reconstructed pointer what AtomicVisibility
   demands. *)
RestartIsAtomic == [][RecoverPublication => AtomicVisibility']_PVars

----------------------------------------------------------------------------
(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* A reader observed a structure while a commit beyond its version had a
   copy there and was not completely installed: the pre-commit state,
   read while installation is in progress. *)
ReaderSeesPreCommitUnreached ==
  ~(\E s \in reader.read : reader.hidden[s] # {})

(* The engine crashed with a durable record whose installation is
   incomplete. *)
CrashMidInstallUnreached ==
  ~(engine = "down" /\ \E t \in TxnsOf(log) : ~CompletelyInstalled(t))

(* Recovery installed a commit the failure had interrupted and the pointer
   then passed it. *)
RecoveryPublishesInterruptedUnreached ==
  ~(Serving /\ ~recovering /\ interrupted # {}
    /\ \A t \in interrupted : CompletelyInstalled(t) /\ t \in VisibleTxns)

(* A failure struck after the pointer had passed a commit: recovery keeps
   that published prefix and, while it installs a later interrupted commit
   (half installed by recovery itself, not necessarily at the failure),
   the pointer stays below it. *)
RecoveryRebuildsPublishedPrefixUnreached ==
  ~(recovering /\ pointerAtFailure >= 1 /\ pointer >= pointerAtFailure
    /\ \E t \in interrupted : installed[t] # {} /\ ~CompletelyInstalled(t) /\ t \notin VisibleTxns)

============================================================================
