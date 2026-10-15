---------------------------- MODULE Resources ----------------------------
(* Kernel model slice 1, part A (issue #16): the resource namespace and
   admission protocol of docs/concept.md §4 - point, range and table
   resources, acquired in a fixed canonical order and held through commit
   (strict two-phase locking). See specs/README.md for the scenario, the
   abstractions this model makes, and what it does not cover. *)

EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS t1, t2, t3

Txns == {t1, t2, t3}
Keys == 1..3

Min(S) == CHOOSE m \in S : \A x \in S : m <= x

(* A resource's coverage is the set of keys it spans: a point is a
   singleton, a range an interval, a table the whole key space.
   Compatibility depends only on this coverage - concept.md §4's "equal
   keys conflict ... a point conflicts with any range or table resource
   covering it" generalizes to "two resources conflict iff their coverage
   intersects" - so the model carries no separate point/range/table tag. *)
Overlaps(a, b) == a \cap b # {}

(* t1 needs a point on key 1, then a range covering keys 2-3: a multi-step
   footprint acquired in canonical (ascending) order. t2 and t3 each need
   only the point on key 2 - identical footprints, so t2 and t3 exercise
   the "equal keys conflict" rule directly against each other, and both
   race t1's pending range the moment t1 has acquired its point and is
   trying to acquire the range that covers key 2 (concept.md §4's named
   race: "a point registration while a covering range is being acquired"). *)
Footprint ==
  [t \in Txns |->
    IF t = t1 THEN <<{1}, {2, 3}>>
    ELSE <<{2}>>]

IsSortedByMin(seq) ==
  \A i \in 1..(Len(seq) - 1) : Min(seq[i]) <= Min(seq[i + 1])

ASSUME \A t \in Txns : IsSortedByMin(Footprint[t])
ASSUME \A t \in Txns : \A i \in 1..Len(Footprint[t]) : Footprint[t][i] # {}

VARIABLES status, next, precedes

Vars == <<status, next, precedes>>

TxnStates == {"active", "committed"}

Init ==
  /\ status = [t \in Txns |-> "active"]
  /\ next = [t \in Txns |-> 0]
  /\ precedes = {}

HeldResources(t) ==
  IF status[t] = "active" THEN {Footprint[t][i] : i \in 1..next[t]} ELSE {}

HeldKeys(t) == UNION HeldResources(t)

Blocked(S, t) ==
  \E u \in Txns \ {t} : status[u] = "active" /\ Overlaps(HeldKeys(u), S)

FootprintOverlaps(u, S) ==
  \E i \in 1..Len(Footprint[u]) : Overlaps(Footprint[u][i], S)

(* precedes records a happens-before edge u -> t exactly when t's
   acquisition of an overlapping resource could only succeed because u had
   already committed - see Serializable below. *)
AcquireNext(t) ==
  /\ status[t] = "active"
  /\ next[t] < Len(Footprint[t])
  /\ LET S == Footprint[t][next[t] + 1] IN
       /\ ~Blocked(S, t)
       /\ next' = [next EXCEPT ![t] = @ + 1]
       /\ precedes' = precedes \cup
            { <<u, t>> :
                u \in { u \in Txns \ {t} :
                          status[u] = "committed" /\ FootprintOverlaps(u, S) } }
  /\ UNCHANGED status

Commit(t) ==
  /\ status[t] = "active"
  /\ next[t] = Len(Footprint[t])
  /\ status' = [status EXCEPT ![t] = "committed"]
  /\ UNCHANGED <<next, precedes>>

AllCommitted == \A t \in Txns : status[t] = "committed"

(* Self-loop once every transaction has committed, so a finished run is not
   mistaken for the deadlock TLC's default deadlock check looks for. *)
Terminating == AllCommitted /\ UNCHANGED Vars

Next == \E t \in Txns : AcquireNext(t) \/ Commit(t) \/ Terminating

Spec == Init /\ [][Next]_Vars

TypeOK ==
  /\ status \in [Txns -> TxnStates]
  /\ next \in [Txns -> Nat]
  /\ precedes \subseteq (Txns \X Txns)

(* Mutual exclusion of incompatible holders: no two live transactions ever
   hold resources whose coverage intersects. *)
MutualExclusion ==
  \A t, u \in Txns : t # u => HeldKeys(t) \cap HeldKeys(u) = {}

RECURSIVE ReflexiveTransitiveStep(_, _)
ReflexiveTransitiveStep(R, n) ==
  IF n = 0 THEN R
  ELSE LET R2 == R \cup
              { <<a, c>> \in Txns \X Txns : \E b \in Txns : <<a, b>> \in R /\ <<b, c>> \in R }
       IN ReflexiveTransitiveStep(R2, n - 1)

TransitiveClosure(R) == ReflexiveTransitiveStep(R, Cardinality(Txns))

(* Serializability method: `precedes` is a happens-before history recorded
   at acquisition time, not a separate serial-order check. Strict two-phase
   locking (MutualExclusion) means the only way t can acquire a resource
   overlapping one u also needed is for u to have already committed and
   released it, so every such edge is a true happens-before fact, and this
   graph never growing a cycle is exactly the standard conflict-
   serializability certificate: commit order is a valid serial order for
   every pair of transactions whose footprints actually overlapped. *)
Serializable == \A t \in Txns : <<t, t>> \notin TransitiveClosure(precedes)

============================================================================
