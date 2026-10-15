---------------------------- MODULE Publication ----------------------------
(* Publication boundary for the kernel (issue #59, cut from #6):
   docs/concept.md §9.1 over the durable engine of Durable.tla.

   A commit's record becoming durable (Flush) proves it recoverable, not
   that readers may see it. Its derived structures (`Structures`: the base
   row and the reverse-reference set of §5) are installed one at a time
   afterwards, and a single visible-version `pointer` into the log says how
   much of the log readers see:
     Install(t, s)  structure s of durable commit t is installed
     Advance        the pointer passes the next commit, only when every
                    structure of it is installed
     Recover        as Durable's, then the engine is `recovering`: it
                    serves readers at the durable pointer while it finishes
                    installing every durable commit
     Open           recovery ends, only once the pointer has passed the
                    whole log; the pointer is thereby rebuilt from the log
   Installed structures and the pointer are durable; a crash keeps both.

   A reader sees, in each structure, the installed copy of the commits at
   or below the pointer (`VisibleIn`). Protocol transactions keep reading
   the committed state `db` under their locks (Kernel's responses, checked
   by `Serializable`); the pointer governs the snapshot readers of §9.1 and
   is checked by the properties below. Outside this module: installation
   running ahead of the pointer beyond one commit per step's need, several
   logs, byte layout, and a failure during recovery. *)

EXTENDS Durable

Structures == {"row", "refs"}

VARIABLES
  installed,    \* [Txns -> SUBSET Structures]: the structures of each durable commit that are installed
  pointer,      \* 0..Len(log): the visible version; readers see log[1..pointer]
  recovering,   \* BOOLEAN: recovery has started and has not yet passed the whole log
  interrupted   \* ghost: the commits that were not completely installed at the last failure

PVars == <<DVars, installed, pointer, recovering, interrupted>>

VisibleTxns == TxnsOf(SubSeq(log, 1, pointer))

VisibleIn(s) == {log[i].txn : i \in {j \in 1..pointer : s \in installed[log[j].txn]}}

----------------------------------------------------------------------------
PInit ==
  /\ DInit
  /\ installed = [t \in Txns |-> {}]
  /\ pointer = 0
  /\ recovering = FALSE
  /\ interrupted = {}

Install(t, s) ==
  /\ Serving /\ t \in TxnsOf(log) /\ s \notin installed[t]
  /\ installed' = [installed EXCEPT ![t] = @ \cup {s}]
  /\ UNCHANGED <<DVars, pointer, recovering, interrupted>>

CompletelyInstalled(t) == installed[t] = Structures

Advance ==
  /\ Serving /\ pointer < Len(log)
  /\ CompletelyInstalled(log[pointer + 1].txn)
  /\ pointer' = pointer + 1
  /\ UNCHANGED <<DVars, installed, recovering, interrupted>>

RecoverPublication ==
  /\ Recover
  /\ recovering' = TRUE
  /\ UNCHANGED <<installed, pointer, interrupted>>

Open ==
  /\ recovering /\ pointer = Len(log)
  /\ recovering' = FALSE
  /\ UNCHANGED <<DVars, installed, pointer, interrupted>>

(* Durable's steps, while the engine serves and is not recovering; a
   failure records which commits it left incompletely installed. *)
Operate ==
  /\ Serving /\ ~recovering /\ DNext
  /\ UNCHANGED <<installed, pointer, recovering>>
  /\ interrupted' = IF failuresUsed' > failuresUsed
                    THEN {t \in TxnsOf(log') : ~CompletelyInstalled(t)}
                    ELSE interrupted

PNext ==
  \/ Operate
  \/ \E t \in Txns, s \in Structures : Install(t, s)
  \/ Advance
  \/ RecoverPublication
  \/ Open

PSpec == PInit /\ [][PNext]_PVars

----------------------------------------------------------------------------
PTypeOK ==
  /\ DTypeOK
  /\ installed \in [Txns -> SUBSET Structures]
  /\ \A t \in Txns : installed[t] # {} => t \in TxnsOf(log)
  /\ pointer \in 0..Len(log)
  /\ recovering \in BOOLEAN
  /\ interrupted \subseteq TxnsOf(log)

(* §9.1: the pointer never passes a commit with a structure missing. *)
PointerBelowInstalled ==
  \A i \in 1..pointer : CompletelyInstalled(log[i].txn)

(* §9.1: the visible version never falls back, across recovery too. *)
PointerMonotone == [][pointer' >= pointer]_PVars

(* §9.1: whenever the engine serves a reader, each commit shows in every
   structure or in none - never in some but not all. *)
AtomicVisibility ==
  Serving =>
    \A t \in TxnsOf(log) :
      LET shownIn == {s \in Structures : t \in VisibleIn(s)} IN
      shownIn = {} \/ shownIn = Structures

(* The restart itself exposes nothing partial: the first reader after a
   failure sees at the durable pointer what AtomicVisibility demands. It
   is the case where the pointer advanced before the install finished and
   a crash fell in between. *)
RestartIsAtomic == [][RecoverPublication => AtomicVisibility']_PVars

----------------------------------------------------------------------------
(* Witnesses (negated; TLC's counterexample is the reachability proof). *)

(* A reader observes the pre-commit state while installation is in
   progress: a durable commit has one structure installed and shows in
   none, for the pointer is below it. *)
ReaderSeesPreCommitUnreached ==
  ~(Serving /\ \E t \in TxnsOf(log) :
      installed[t] # {} /\ ~CompletelyInstalled(t) /\ t \notin VisibleTxns)

(* The engine crashed with a durable record whose installation is
   incomplete. *)
CrashMidInstallUnreached ==
  ~(engine = "down" /\ \E t \in TxnsOf(log) : ~CompletelyInstalled(t))

(* Recovery installed a commit the failure had interrupted and the pointer
   then passed it. *)
RecoveryPublishesInterruptedUnreached ==
  ~(Serving /\ ~recovering /\ interrupted # {}
    /\ \A t \in interrupted : CompletelyInstalled(t) /\ t \in VisibleTxns)

============================================================================
