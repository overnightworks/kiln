---------------------------- MODULE Pins ----------------------------
(* Kernel model slice 1, part B.2 (issue #16): the foreign-key pin of
   docs/concept.md §3.2/§5 - a durable reverse reference that outlives its
   creating transaction, and a parent deletion that must acquire an
   incompatible reservation against that reverse-reference set. See
   specs/README.md for the scenario and its bounds. *)

EXTENDS Naturals

CONSTANTS c1, c2, d1

(* One parent, so the reverse-reference set is simply the set of child
   transactions whose insert has committed and not been removed - this
   model never removes a child, since the property under test is whether
   a live reverse reference blocks deletion, not child lifecycle. c1 and
   c2 both pin the same parent (concurrent pins must not conflict with
   each other); d1 deletes that parent (a delete must conflict with every
   pin on it, tentative or durable). *)
ChildTxns == {c1, c2}
DeleteTxns == {d1}
Txns == ChildTxns \cup DeleteTxns

VARIABLES status, pinHeld, deleteHeld, liveChildren, parentDeleted

Vars == <<status, pinHeld, deleteHeld, liveChildren, parentDeleted>>

TxnStates == {"active", "committed", "aborted"}

Init ==
  /\ status = [t \in Txns |-> "active"]
  /\ pinHeld = [c \in ChildTxns |-> FALSE]
  /\ deleteHeld = [d \in DeleteTxns |-> FALSE]
  /\ liveChildren = {}
  /\ parentDeleted = FALSE

AnyPinHeld == \E c \in ChildTxns : status[c] = "active" /\ pinHeld[c]
AnyDeleteHeld == \E d \in DeleteTxns : status[d] = "active" /\ deleteHeld[d]

(* A pin is refused while an incompatible delete reservation is held -
   tentative or not - and once the parent's deletion has committed,
   permanently: a child may never reference an already-deleted parent,
   exactly as a committed claim value is taken forever (Claims.tla). *)
AcquirePin(c) ==
  /\ status[c] = "active"
  /\ ~pinHeld[c]
  /\ ~AnyDeleteHeld
  /\ ~parentDeleted
  /\ pinHeld' = [pinHeld EXCEPT ![c] = TRUE]
  /\ UNCHANGED <<status, deleteHeld, liveChildren, parentDeleted>>

(* Committing installs the durable reverse reference: it outlives this
   transaction's own "active" status, unlike a plain lock. *)
CommitChild(c) ==
  /\ status[c] = "active"
  /\ pinHeld[c]
  /\ status' = [status EXCEPT ![c] = "committed"]
  /\ liveChildren' = liveChildren \cup {c}
  /\ UNCHANGED <<pinHeld, deleteHeld, parentDeleted>>

AbortChild(c) ==
  /\ status[c] = "active"
  /\ status' = [status EXCEPT ![c] = "aborted"]
  /\ UNCHANGED <<pinHeld, deleteHeld, liveChildren, parentDeleted>>

(* The delete reservation can only be acquired when the reverse-reference
   set is already empty and no pin is tentatively in flight - both the
   durable and the tentative form of a live reference block it. *)
AcquireDelete(d) ==
  /\ status[d] = "active"
  /\ ~deleteHeld[d]
  /\ liveChildren = {}
  /\ ~AnyPinHeld
  /\ deleteHeld' = [deleteHeld EXCEPT ![d] = TRUE]
  /\ UNCHANGED <<status, pinHeld, liveChildren, parentDeleted>>

(* Re-checked at the commit point itself, exactly as concept.md §5
   requires, even though AcquireDelete's guard already makes this
   structurally true in this model (no new pin can start once the delete
   reservation is held). Committing marks the parent gone for good. *)
CommitDelete(d) ==
  /\ status[d] = "active"
  /\ deleteHeld[d]
  /\ liveChildren = {}
  /\ status' = [status EXCEPT ![d] = "committed"]
  /\ parentDeleted' = TRUE
  /\ UNCHANGED <<pinHeld, deleteHeld, liveChildren>>

AbortDelete(d) ==
  /\ status[d] = "active"
  /\ status' = [status EXCEPT ![d] = "aborted"]
  /\ UNCHANGED <<pinHeld, deleteHeld, liveChildren, parentDeleted>>

AllDone == \A t \in Txns : status[t] \in {"committed", "aborted"}

Terminating == AllDone /\ UNCHANGED Vars

Next ==
  \/ \E c \in ChildTxns : AcquirePin(c) \/ CommitChild(c) \/ AbortChild(c)
  \/ \E d \in DeleteTxns : AcquireDelete(d) \/ CommitDelete(d) \/ AbortDelete(d)
  \/ Terminating

Spec == Init /\ [][Next]_Vars

TypeOK ==
  /\ status \in [Txns -> TxnStates]
  /\ pinHeld \in [ChildTxns -> BOOLEAN]
  /\ deleteHeld \in [DeleteTxns -> BOOLEAN]
  /\ liveChildren \subseteq ChildTxns
  /\ parentDeleted \in BOOLEAN

(* Referential integrity: a parent is never deleted while a live reverse
   reference to it exists, and - once deleted - no live reference to it
   ever appears again (no child may commit a pin on an already-deleted
   parent, in this or any later state). *)
ReferentialIntegrity ==
  /\ \A d \in DeleteTxns : status[d] = "committed" => liveChildren = {}
  /\ parentDeleted => liveChildren = {}

(* Mutual exclusion between a delete reservation and any pin on the same
   parent - the half of the compatibility rule that is exclusive (two
   pins on the same parent are deliberately not checked against each
   other: both may be held, and TLC's full state enumeration reaches that
   configuration without violating ReferentialIntegrity). *)
NoConflictingHolds ==
  \A d \in DeleteTxns, c \in ChildTxns :
    ~(status[d] = "active" /\ deleteHeld[d] /\ status[c] = "active" /\ pinHeld[c])

============================================================================
