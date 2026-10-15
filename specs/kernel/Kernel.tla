------------------------------ MODULE Kernel ------------------------------
(* Kernel model (issue #6; slice 1 in #16, escrow admission in #24):
   docs/concept.md §4's one resource namespace with its admission protocol
   (the complete footprint, including each escrow key's reservation,
   acquired at dispatch and held through commit: strict two-phase locking),
   §3.2/§5's typed reservations (unique claim, foreign-key pin with a
   durable reverse reference, escrow draw under balance >= 0) composed
   with ordinary point, range and table locks over the same keys, and
   real transactions whose observations and responses are checked against
   a serial execution. Scenarios instantiate `Prog` and `InitDb`; see
   specs/README.md for bounds, rules and results. *)

EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS
  Prog,    \* [Txns -> Seq(Op)]: each transaction's fixed program
  InitDb   \* [Keys -> Row]: the committed state before any transaction

Txns == DOMAIN Prog
Keys == DOMAIN InitDb

(* A row of the one table. An absent row is normalized to AbsentRow so
   that "absent" has exactly one representation. `parent` = 0 means no
   foreign key; `bal` is the escrow-guarded quantity, `val` a plain
   column (e.g. tickets sold). *)
Row == [present : BOOLEAN, val : Int, bal : Int, parent : Keys \cup {0}]
AbsentRow == [present |-> FALSE, val |-> 0, bal |-> 0, parent |-> 0]

(* The reverse-reference set of §5: every live child referencing k. It is
   derived from the rows, so it is durable exactly as long as the child
   row exists. *)
Refs(v, k) == {c \in Keys : v[c].present /\ v[c].parent = k}

----------------------------------------------------------------------------
(* Operations. Each is one caller-visible step of a transaction program.
     Read(k)            S point lock, returns the row
     Scan(lo, hi)       shared range lock over lo..hi (the whole key space
                        is a table lock), returns the present keys and vals
     Update(k, val, bal) X point lock, returns the previous row, sets both
                        columns if the row is present
     Insert(k, p)       unique claim on k (+ pin on parent p if p # 0)
     Delete(k)          exclusive reservation against k's reverse-reference
                        set (+ pin on k's own parent while unlinking)
     Buy(k, a)          escrow draw of a against k's bal, sold += 1
     BuyUnder(k, a, cap) the same draw, taken only while fewer than cap are
                        sold: a branch on the row, so it also reads k (S) and
                        the budget covers it either way
     BuyVal(k, j)       draws row j's val from k: an amount the program
                        does not know statically (budget unbounded) *)
Read(k) == [op |-> "Read", k |-> k]
Scan(lo, hi) == [op |-> "Scan", lo |-> lo, hi |-> hi]
Update(k, val, bal) == [op |-> "Update", k |-> k, val |-> val, bal |-> bal]
Insert(k, p) == [op |-> "Insert", k |-> k, p |-> p]
Delete(k) == [op |-> "Delete", k |-> k]
Buy(k, a) == [op |-> "Buy", k |-> k, a |-> a]
BuyUnder(k, a, cap) == [op |-> "BuyUnder", k |-> k, a |-> a, cap |-> cap]
BuyVal(k, j) == [op |-> "BuyVal", k |-> k, j |-> j]

DrawOps == {"Buy", "BuyUnder", "BuyVal"}
Draws(op) == op.op \in DrawOps

Res(kind, cov) == [kind |-> kind, cov |-> cov]

(* The resources an operation relies on, computed against the view it
   runs on (a delete's pin depends on the parent the row names). *)
Need(op, v) ==
  CASE op.op = "Read"   -> {Res("S", {op.k})}
    [] op.op = "Scan"   -> {Res("RS", op.lo..op.hi)}
    [] op.op = "Update" -> {Res("X", {op.k})}
    [] op.op = "Insert" -> {Res("CLAIM", {op.k})}
                           \cup (IF op.p = 0 THEN {} ELSE {Res("PIN", {op.p})})
    [] op.op = "Delete" -> {Res("DEL", {op.k})}
                           \cup (IF v[op.k].present /\ v[op.k].parent # 0
                                 THEN {Res("PIN", {v[op.k].parent})} ELSE {})
    [] op.op = "Buy"    -> {Res("DRAW", {op.k})}
    [] op.op = "BuyUnder" -> {Res("DRAW", {op.k}), Res("S", {op.k})}
    [] op.op = "BuyVal" -> {Res("DRAW", {op.k}), Res("S", {op.j})}

(* Compatibility (§4, §5). Two resources whose coverage is disjoint never
   contend ("two keyed resources with distinct keys never contend"); with
   overlapping coverage - equal keys, or a point under a covering range or
   table resource - only these pairs are compatible. Where the concept is
   silent (a pin or a draw against an S lock, a pin against a draw) the
   conservative rule is incompatible; specs/README.md lists these gaps. *)
Kinds == {"S", "RS", "X", "CLAIM", "PIN", "DEL", "DRAW"}
CompatibleKinds ==
  {<<"S", "S">>, <<"S", "RS">>, <<"RS", "S">>, <<"RS", "RS">>,
   <<"PIN", "PIN">>, <<"DRAW", "DRAW">>}

Compatible(a, b) ==
  \/ a.cov \cap b.cov = {}
  \/ <<a.kind, b.kind>> \in CompatibleKinds

----------------------------------------------------------------------------
(* Pure operation semantics on a view v (the committed rows overlaid with
   the running transaction's own writes). The same function drives the
   live protocol and the serial reference execution, so a divergence
   between them can only come from interleaving, never from two
   definitions of what an operation does. A rejected result is a
   constraint failure the caller sees; §5 lets only committed state
   justify it. *)
Result(resp, v, rejected) == [resp |-> resp, db |-> v, rejected |-> rejected]

DrawFrom(k, a, v) ==
  IF v[k].bal < a THEN Result("insufficient", v, TRUE)
  ELSE Result("ok", [v EXCEPT ![k].bal = @ - a, ![k].val = @ + 1], FALSE)

ApplyOp(op, v) ==
  CASE op.op = "Read" ->
         Result(v[op.k], v, FALSE)
    [] op.op = "Scan" ->
         Result({<<k, v[k].val>> : k \in {k \in op.lo..op.hi : v[k].present}},
                v, FALSE)
    [] op.op = "Update" ->
         IF v[op.k].present
         THEN Result(v[op.k],
                     [v EXCEPT ![op.k].val = op.val, ![op.k].bal = op.bal],
                     FALSE)
         ELSE Result("missing", v, FALSE)
    [] op.op = "Insert" ->
         IF v[op.k].present THEN Result("exists", v, TRUE)
         ELSE IF op.p # 0 /\ ~v[op.p].present THEN Result("noparent", v, TRUE)
         ELSE Result("ok",
                     [v EXCEPT ![op.k] =
                        [present |-> TRUE, val |-> 0, bal |-> 0, parent |-> op.p]],
                     FALSE)
    [] op.op = "Delete" ->
         IF ~v[op.k].present THEN Result("missing", v, FALSE)
         ELSE IF Refs(v, op.k) # {} THEN Result("referenced", v, TRUE)
         ELSE Result("ok", [v EXCEPT ![op.k] = AbsentRow], FALSE)
    [] op.op = "Buy" ->
         DrawFrom(op.k, op.a, v)
    [] op.op = "BuyUnder" ->
         IF v[op.k].val < op.cap THEN DrawFrom(op.k, op.a, v)
         ELSE Result("skipped", v, FALSE)
    [] op.op = "BuyVal" ->
         DrawFrom(op.k, v[op.j].val, v)

----------------------------------------------------------------------------
(* Serial reference execution: run the transactions of `perm` one after
   another from InitDb; a rejected transaction leaves no effect. *)
RECURSIVE RunOps(_, _, _)
RunOps(ops, v, rs) ==
  IF ops = <<>> THEN [db |-> v, resps |-> rs, rejected |-> FALSE]
  ELSE LET r == ApplyOp(Head(ops), v) IN
       IF r.rejected THEN [db |-> v, resps |-> Append(rs, r.resp), rejected |-> TRUE]
       ELSE RunOps(Tail(ops), r.db, Append(rs, r.resp))

RunTxn(t, v) ==
  LET r == RunOps(Prog[t], v, <<>>) IN
  [db |-> IF r.rejected THEN v ELSE r.db,
   outcome |-> [resps |-> r.resps,
                status |-> IF r.rejected THEN "rejected" ELSE "committed"]]

(* `outcomes` maps each transaction run so far to its responses and its
   terminal status. *)
RECURSIVE RunSerial(_, _, _)
RunSerial(perm, v, outcomes) ==
  IF perm = <<>> THEN [db |-> v, outcomes |-> outcomes]
  ELSE LET r == RunTxn(Head(perm), v) IN
       RunSerial(Tail(perm), r.db, outcomes @@ (Head(perm) :> r.outcome))

RECURSIVE Perms(_)
Perms(S) ==
  IF S = {} THEN {<<>>}
  ELSE UNION {{<<x>> \o p : p \in Perms(S \ {x})} : x \in S}

----------------------------------------------------------------------------
(* The complete footprint (§4): every resource a program needs, computed
   statically from the program and InitDb. Only a delete's resources
   depend on the view (the pin on the parent its row names), and a parent
   is set by InitDb or by an Insert in some program, so the footprint pins
   every parent the row can have. The over-approximation costs at most a
   spurious conflict; `FootprintCovers` checks that it never misses a
   resource an operation turns out to need. *)
Ops == UNION {{Prog[t][i] : i \in DOMAIN Prog[t]} : t \in Txns}
PossibleParents(k) ==
  ({InitDb[k].parent} \cup {op.p : op \in {op \in Ops : op.op = "Insert" /\ op.k = k}})
  \ {0}
OpFootprint(op) ==
  IF op.op = "Delete"
  THEN {Res("DEL", {op.k})} \cup {Res("PIN", {p}) : p \in PossibleParents(op.k)}
  ELSE Need(op, InitDb)
Footprint(t) == UNION {OpFootprint(Prog[t][i]) : i \in DOMAIN Prog[t]}

(* The escrow budget (§4, rule 1): a program's statically known total draw
   on a key, the sum of its draw amounts there. A draw whose amount only
   the data decides (BuyVal) makes the budget unknown, which counts as
   unbounded. A branch (BuyUnder) counts in full whether or not it is
   taken. *)
EscrowKeys(t) == {k \in Keys : Res("DRAW", {k}) \in Footprint(t)}

BudgetKnown(t, k) ==
  ~\E i \in DOMAIN Prog[t] : Prog[t][i].op = "BuyVal" /\ Prog[t][i].k = k

RECURSIVE SumAmounts(_, _)
SumAmounts(ops, k) ==
  IF ops = <<>> THEN 0
  ELSE (IF Head(ops).op \in {"Buy", "BuyUnder"} /\ Head(ops).k = k THEN Head(ops).a ELSE 0)
       + SumAmounts(Tail(ops), k)

Budget(t, k) == SumAmounts(Prog[t], k)

ASSUME \A k \in Keys : InitDb[k] \in Row /\ (~InitDb[k].present => InitDb[k] = AbsentRow)

----------------------------------------------------------------------------
VARIABLES
  status,   \* [Txns -> {"pending", "active", "committed", "rejected"}]
  seq,      \* [Txns -> Nat]: dispatch sequence number, 0 before dispatch
  pc,       \* [Txns -> Nat]: operations already executed
  held,     \* [Txns -> SUBSET Resource]: the footprint, held from admission to the end
  reserved, \* [Txns -> [Keys -> Nat]]: the escrow reservation per key, admission to the end
  wr,       \* [Txns -> partial Keys -> Row]: tentative writes, applied at commit
  draws,    \* [Txns -> [Keys -> Draw]]: tentative escrow draws, deltas at commit
  resp,     \* [Txns -> Seq(response)]: what the caller has been told
  db        \* [Keys -> Row]: committed state

Vars == <<status, seq, pc, held, reserved, wr, draws, resp, db>>

Resource == [kind : Kinds, cov : SUBSET Keys]

NoWrites == [k \in {} |-> AbsentRow]

(* An escrow draw is a delta, never an overwrite: what a transaction has
   drawn (bal) and sold (val) so far on a key. *)
Draw == [bal : Nat, val : Nat]
NoDraws == [k \in Keys |-> [bal |-> 0, val |-> 0]]
NoReservation == [k \in Keys |-> 0]

Init ==
  /\ status = [t \in Txns |-> "pending"]
  /\ seq = [t \in Txns |-> 0]
  /\ pc = [t \in Txns |-> 0]
  /\ held = [t \in Txns |-> {}]
  /\ reserved = [t \in Txns |-> NoReservation]
  /\ wr = [t \in Txns |-> NoWrites]
  /\ draws = [t \in Txns |-> NoDraws]
  /\ resp = [t \in Txns |-> <<>>]
  /\ db = InitDb

(* A pending transaction holds nothing; once dispatched it is queued for
   admission in dispatch order; an active one holds its footprint. *)
Pending(t) == status[t] = "pending"
Queued(t) == Pending(t) /\ seq[t] > 0
Active(t) == status[t] = "active"
Live(t) == Pending(t) \/ Active(t)
Running(t) == Active(t) /\ pc[t] < Len(Prog[t])
NextOp(t) == Prog[t][pc[t] + 1]

(* The view a running transaction evaluates on: committed rows overlaid
   with its own tentative writes and draws (read-your-writes). *)
Drawn(t, k, row) ==
  [row EXCEPT !.bal = @ - draws[t][k].bal, !.val = @ + draws[t][k].val]
Undrawn(t, k, row) ==
  [row EXCEPT !.bal = @ + draws[t][k].bal, !.val = @ - draws[t][k].val]

View(t) ==
  [k \in Keys |-> Drawn(t, k, IF k \in DOMAIN wr[t] THEN wr[t][k] ELSE db[k])]

(* One rule for draws (§4, rule 1). A draw consumes the transaction's own
   reservation: the balance it sees is what remains of that reservation,
   never the committed balance other holders are drawing from. The one
   exception: a transaction that holds a covering resource whose kind is
   incompatible with DRAW is the key's only draw holder, so no other
   reservation can be outstanding on it, and it draws on its real view
   (an Update that raised the balance in the same transaction is drawn
   from; a branch that read the row sees what it read). *)
SoleDrawHolder(t, k) == \E h \in held[t] : h.kind # "DRAW" /\ k \in h.cov

DrawView(t, k) ==
  IF SoleDrawHolder(t, k) THEN View(t)
  ELSE [View(t) EXCEPT ![k].bal = reserved[t][k] - draws[t][k].bal]

OpView(t, op) == IF Draws(op) THEN DrawView(t, op.k) ELSE View(t)

(* Admission guard: every registered resource must be compatible with
   every resource a live transaction holds. *)
Conflicting(t, R) ==
  {u \in Txns \ {t} : Active(u) /\ \E r \in R, h \in held[u] : ~Compatible(r, h)}

(* Escrow admission (§4, rule 1; §5): a transaction's reservation on an
   escrow key is min(budget, committed balance). Live reservations are
   funded from the committed balance, so a transaction is admitted only
   while every other live reservation on the key leaves room for its own:
   a reservation capped at the committed balance therefore waits until
   every other reservation on that key has resolved (rule 2), and its
   draws then fail only on committed state. *)
RECURSIVE SumOver(_, _)
SumOver(U, k) ==
  IF U = {} THEN 0
  ELSE LET u == CHOOSE x \in U : TRUE IN reserved[u][k] + SumOver(U \ {u}, k)

Capped(t, k) == ~BudgetKnown(t, k) \/ Budget(t, k) > db[k].bal

Reservation(t, k) == IF Capped(t, k) THEN db[k].bal ELSE Budget(t, k)

ReservationHolders(t, k) == {u \in Txns \ {t} : Active(u) /\ reserved[u][k] > 0}

EscrowBlocked(t, k) ==
  SumOver(ReservationHolders(t, k), k) + Reservation(t, k) > db[k].bal

EscrowBlockers(t) ==
  UNION {ReservationHolders(t, k) : k \in {k \in EscrowKeys(t) : EscrowBlocked(t, k)}}

(* Admission order (§4, rule 3): one queue per resource in dispatch
   order. A queued transaction waits on every older queued transaction
   whose footprint shares a resource with its own, so it never bypasses
   it; disjoint footprints proceed independently. *)
Shares(t, u) == \E a \in Footprint(t), b \in Footprint(u) : a.cov \cap b.cov # {}

OlderQueued(t) == {u \in Txns \ {t} : Queued(u) /\ seq[u] < seq[t] /\ Shares(t, u)}

(* The wait-for relation: who a live transaction is waiting on right now.
   A queued transaction waits on every holder its footprint conflicts
   with, on the reservation holders that leave no room for its own, and
   on older queued transactions sharing a resource. An active one holds
   its whole footprint and never waits again (rule 1). Empty exactly when
   its next step may run. *)
WaitsOn(t) ==
  IF Queued(t) THEN Conflicting(t, Footprint(t)) \cup EscrowBlockers(t) \cup OlderQueued(t)
  ELSE {}

Changed(t, v) == {k \in Keys : v[k] # View(t)[k]}

(* Dispatch: the request enters the admission queue with the next
   sequence number. Queued sequence numbers are dense (1..n): admission
   closes the gap it leaves, so the numbers stay bounded when requests
   repeat. *)
Dispatch(t) ==
  /\ Pending(t) /\ seq[t] = 0
  /\ seq' = [seq EXCEPT ![t] = 1 + Cardinality({u \in Txns : Queued(u)})]
  /\ UNCHANGED <<status, pc, held, reserved, wr, draws, resp, db>>

(* Acquire the complete footprint in one step, atomically with the
   compatibility check and the escrow reservations (§4: registered
   "atomically at dispatch, held through commit"; a covering resource is
   acquired "before any read or write touches the underlying state"). The
   implementation acquires the same set resource by resource in canonical
   order; the step abstracts that sequence, so no partial footprint is
   ever held. *)
Acquire(t) ==
  /\ Queued(t)
  /\ WaitsOn(t) = {}
  /\ status' = [status EXCEPT ![t] = "active"]
  /\ held' = [held EXCEPT ![t] = Footprint(t)]
  /\ reserved' = [reserved EXCEPT ![t] =
                   [k \in Keys |-> IF k \in EscrowKeys(t) THEN Reservation(t, k) ELSE 0]]
  /\ seq' = [u \in Txns |->
             IF u = t THEN 0 ELSE IF Queued(u) /\ seq[u] > seq[t] THEN seq[u] - 1 ELSE seq[u]]
  /\ UNCHANGED <<pc, wr, draws, resp, db>>

(* Execute the next operation on the view under the footprint already
   held; it acquires nothing and waits for nothing. A constraint rejection
   ends the transaction with the caller's failure response and releases
   everything it held, the unused reservation included; nothing it wrote
   survives. *)
DoOp(t) ==
  /\ Running(t)
  /\ LET op == NextOp(t)
         v == OpView(t, op)
         r == ApplyOp(op, v) IN
     /\ pc' = [pc EXCEPT ![t] = @ + 1]
     /\ resp' = [resp EXCEPT ![t] = Append(@, r.resp)]
     /\ IF r.rejected
        THEN /\ status' = [status EXCEPT ![t] = "rejected"]
             /\ held' = [held EXCEPT ![t] = {}]
             /\ reserved' = [reserved EXCEPT ![t] = NoReservation]
             /\ wr' = [wr EXCEPT ![t] = NoWrites]
             /\ draws' = [draws EXCEPT ![t] = NoDraws]
        ELSE /\ UNCHANGED <<status, held, reserved>>
             /\ IF Draws(op)
                THEN /\ draws' = [draws EXCEPT ![t][op.k] =
                                   [bal |-> @.bal + (v[op.k].bal - r.db[op.k].bal),
                                    val |-> @.val + (r.db[op.k].val - v[op.k].val)]]
                     /\ UNCHANGED wr
                ELSE /\ wr' = [wr EXCEPT ![t] =
                                [k \in DOMAIN @ \cup Changed(t, r.db) |->
                                   Undrawn(t, k, r.db[k])]]
                     /\ UNCHANGED draws
  /\ UNCHANGED <<seq, db>>

(* Commit applies the tentative writes and draws atomically - the view
   becomes the committed state - and releases every resource and the
   unused reservation (strict two-phase locking: held from admission to
   commit). *)
Commit(t) ==
  /\ Active(t) /\ pc[t] = Len(Prog[t])
  /\ status' = [status EXCEPT ![t] = "committed"]
  /\ db' = View(t)
  /\ held' = [held EXCEPT ![t] = {}]
  /\ reserved' = [reserved EXCEPT ![t] = NoReservation]
  /\ wr' = [wr EXCEPT ![t] = NoWrites]
  /\ draws' = [draws EXCEPT ![t] = NoDraws]
  /\ UNCHANGED <<seq, pc, resp>>

Finished == \A t \in Txns : ~Live(t)

(* Self-loop once every transaction has ended, so only a state with a
   live transaction and no enabled step counts as the deadlock TLC's
   default check reports. *)
Terminating == Finished /\ UNCHANGED Vars

Next ==
  \/ \E t \in Txns : Dispatch(t) \/ Acquire(t) \/ DoOp(t) \/ Commit(t)
  \/ Terminating

(* Rearrival: a finished transaction requests its program again, from
   a clean slate; the committed rows stay. Only the repeated-arrival
   scenarios (`RepeatSpec`) take this step: the cycle it closes is what
   lets a lasso starve a request. The step only makes the transaction
   pending again; the request starts with the next `Dispatch`.
   Rearrival resets the transaction histories, which `Serializable` and
   `Uniqueness` read, so those history-based checks do not apply to
   repeated executions and those scenarios do not check them. *)
Rearrive(t) ==
  /\ ~Live(t)
  /\ status' = [status EXCEPT ![t] = "pending"]
  /\ pc' = [pc EXCEPT ![t] = 0]
  /\ resp' = [resp EXCEPT ![t] = <<>>]
  /\ UNCHANGED <<seq, held, reserved, wr, draws, db>>

(* Weak fairness (concept §9.5): a step of a transaction that stays
   enabled is taken - admission, execution, completion. Dispatch is the
   caller's request, not a step the protocol owes. *)
Fairness ==
  \A t \in Txns : WF_Vars(Acquire(t)) /\ WF_Vars(DoOp(t)) /\ WF_Vars(Commit(t))

Spec == Init /\ [][Next]_Vars /\ Fairness

RepeatSpec == Init /\ [][Next \/ \E t \in Txns : Rearrive(t)]_Vars /\ Fairness

----------------------------------------------------------------------------
TypeOK ==
  /\ status \in [Txns -> {"pending", "active", "committed", "rejected"}]
  /\ seq \in [Txns -> Nat]
  /\ pc \in [Txns -> Nat]
  /\ \A t \in Txns : held[t] \subseteq Resource
  /\ reserved \in [Txns -> [Keys -> Nat]]
  /\ \A t \in Txns : DOMAIN wr[t] \subseteq Keys /\ \A k \in DOMAIN wr[t] : wr[t][k] \in Row
  /\ draws \in [Txns -> [Keys -> Draw]]
  /\ db \in [Keys -> Row]

(* No two live transactions hold incompatible resources. *)
MutualExclusion ==
  \A t, u \in Txns : t # u /\ Active(t) /\ Active(u) =>
    \A a \in held[t], b \in held[u] : Compatible(a, b)

(* An operation runs without acquiring anything: the footprint taken at
   admission holds every resource it needs on the view it runs on. *)
FootprintCovers ==
  \A t \in Txns : Running(t) => Need(NextOp(t), View(t)) \subseteq held[t]

(* Escrow conservation (§4, rule 1). Live reservations are funded: their
   sum on a key never exceeds its committed balance. Draws stay within the
   reservation, the one rule of DrawView: only the key's sole draw holder
   draws on its real view and may exceed what it reserved. *)
ReservationsFunded ==
  \A k \in Keys : SumOver({t \in Txns : Active(t)}, k) <= db[k].bal

DrawsWithinReservation ==
  \A t \in Txns, k \in Keys :
    Active(t) /\ ~SoleDrawHolder(t, k) => draws[t][k].bal <= reserved[t][k]

RECURSIVE Closure(_, _)
Closure(R, n) ==
  IF n = 0 THEN R
  ELSE Closure(R \cup {<<a, c>> \in Txns \X Txns :
                         \E b \in Txns : <<a, b>> \in R /\ <<b, c>> \in R}, n - 1)

WaitFor == {<<t, u>> \in Txns \X Txns : u \in WaitsOn(t)}

NoWaitCycle == \A t \in Txns : <<t, t>> \notin Closure(WaitFor, Cardinality(Txns))

(* Serializability, checked for real: the transactions that have ended
   must have produced, in some serial order, exactly the responses they
   returned, exactly the terminal outcome each reached (committed, or
   rejected with its caller-visible failure as the last response), and
   exactly the committed state. *)
Ended == {t \in Txns : ~Live(t)}

Serializable ==
  \E perm \in Perms(Ended) :
    LET r == RunSerial(perm, InitDb, [t \in {} |-> {}]) IN
    /\ r.db = db
    /\ \A t \in Ended :
         r.outcomes[t] = [resps |-> resp[t], status |-> status[t]]

(* The typed reservations' own invariants over committed state. *)
OpOk(t, kind, k) ==
  \E i \in 1..Len(resp[t]) :
    Prog[t][i].op = kind /\ Prog[t][i].k = k /\ resp[t][i] = "ok"
Committed == {t \in Txns : status[t] = "committed"}

Uniqueness ==
  \A k \in Keys :
    Cardinality({t \in Committed : OpOk(t, "Insert", k)})
      <= Cardinality({t \in Committed : OpOk(t, "Delete", k)}) + 1

ReferentialIntegrity ==
  \A k \in Keys : db[k].present /\ db[k].parent # 0 => db[db[k].parent].present

BalanceNonNegative == \A k \in Keys : db[k].bal >= 0

(* Liveness (concept §9.5): every request ends, committed or rejected,
   under the fairness of `Spec`. After a `Rearrive` the transaction is
   pending and undispatched; the obligation starts with the later
   `Dispatch`, so a repeated request is a new request. *)
RequestedLeadsToTerminal == \A t \in Txns : Queued(t) ~> ~Live(t)

============================================================================
