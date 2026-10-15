------------------------------ MODULE Kernel ------------------------------
(* Kernel model, slice 1 (issue #16, parent #6): docs/concept.md §4's one
   resource namespace with its admission protocol (strict two-phase
   locking, canonical order), §3.2/§5's typed reservations (unique claim,
   foreign-key pin with a durable reverse reference, escrow draw under
   balance >= 0) composed with ordinary point, range and table locks over
   the same keys, and real transactions whose observations and responses
   are checked against a serial execution. Scenarios instantiate `Prog`
   and `InitDb`; see specs/README.md for bounds, rules and results. *)

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
     Buy(k, a)          escrow draw of a against k's bal, sold += 1 *)
Read(k) == [op |-> "Read", k |-> k]
Scan(lo, hi) == [op |-> "Scan", lo |-> lo, hi |-> hi]
Update(k, val, bal) == [op |-> "Update", k |-> k, val |-> val, bal |-> bal]
Insert(k, p) == [op |-> "Insert", k |-> k, p |-> p]
Delete(k) == [op |-> "Delete", k |-> k]
Buy(k, a) == [op |-> "Buy", k |-> k, a |-> a]

Res(kind, cov) == [kind |-> kind, cov |-> cov]

(* The resources an operation registers, computed against the view it
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

(* Compatibility (§4, §5). Two resources whose coverage is disjoint never
   contend ("two keyed resources with distinct keys never contend"); with
   overlapping coverage - equal keys, or a point under a covering range or
   table resource - only these pairs are compatible. Where the concept is
   silent (a pin or a draw against an S lock, a pin against a draw) the
   conservative rule is incompatible; specs/README.md lists these gaps. *)
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
   justify it, which is why the live protocol evaluates it on the view
   only once every tentative holder it conflicts with is gone. *)
Result(resp, v, rejected) == [resp |-> resp, db |-> v, rejected |-> rejected]

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
         IF v[op.k].bal < op.a THEN Result("insufficient", v, TRUE)
         ELSE Result("ok",
                     [v EXCEPT ![op.k].bal = @ - op.a, ![op.k].val = @ + 1],
                     FALSE)

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
(* Canonical order (§4) over a program's complete acquisition footprint.
   Only a delete's resources depend on the view (the parent its row
   names), and a parent is set by InitDb or by an Insert in some program,
   so the static footprint pins every parent the row can have: an
   over-approximation that can only refuse a program, never admit one
   out of order. *)
Ops == UNION {{Prog[t][i] : i \in DOMAIN Prog[t]} : t \in Txns}
PossibleParents(k) ==
  ({InitDb[k].parent} \cup {op.p : op \in {op \in Ops : op.op = "Insert" /\ op.k = k}})
  \ {0}
Footprint(op) ==
  IF op.op = "Delete"
  THEN {Res("DEL", {op.k})} \cup {Res("PIN", {p}) : p \in PossibleParents(op.k)}
  ELSE Need(op, InitDb)
KeysOf(R) == UNION {r.cov : r \in R}
Before(t, i) == UNION {Footprint(Prog[t][j]) : j \in 1..(i - 1)}

(* Every key an operation covers for the first time lies above every key
   the program covered before it. A waiter then waits only on a holder
   that is further along in key order, so (together with NoUpgrade) the
   wait-for relation over exclusive resources is acyclic. *)
CanonicalOrder ==
  \A t \in Txns : \A i \in 2..Len(Prog[t]) :
    \A k \in KeysOf(Footprint(Prog[t][i])) \ KeysOf(Before(t, i)) :
      \A j \in KeysOf(Before(t, i)) : j < k

(* A mode dominates another when every kind compatible with it is also
   compatible with the other, so a request in the dominated mode on a key
   already held in the dominating one can never be refused. A program that
   needs a stronger mode on a key it already holds would acquire out of
   order; it is refused here. Whether the protocol detects, restarts or
   forbids such an upgrade is decided in #24. *)
Kinds == {"S", "RS", "X", "CLAIM", "PIN", "DEL", "DRAW"}
Dominates(m, w) ==
  \A x \in Kinds : <<m, x>> \in CompatibleKinds => <<w, x>> \in CompatibleKinds
NoUpgrade ==
  \A t \in Txns : \A i \in 2..Len(Prog[t]) :
    \A r \in Footprint(Prog[t][i]) : \A k \in r.cov \cap KeysOf(Before(t, i)) :
      \E h \in Before(t, i) : k \in h.cov /\ Dominates(h.kind, r.kind)

ASSUME CanonicalOrder
ASSUME NoUpgrade
ASSUME \A k \in Keys : InitDb[k] \in Row /\ (~InitDb[k].present => InitDb[k] = AbsentRow)

----------------------------------------------------------------------------
VARIABLES
  status,  \* [Txns -> {"active", "committed", "rejected"}]
  pc,      \* [Txns -> Nat]: operations already executed
  held,    \* [Txns -> SUBSET Resource]: resources registered, held to the end
  wr,      \* [Txns -> partial Keys -> Row]: tentative writes, applied at commit
  draws,   \* [Txns -> [Keys -> Draw]]: tentative escrow draws, deltas at commit
  resp,    \* [Txns -> Seq(response)]: what the caller has been told
  db       \* [Keys -> Row]: committed state

Vars == <<status, pc, held, wr, draws, resp, db>>

Resource == [kind : Kinds, cov : SUBSET Keys]

NoWrites == [k \in {} |-> AbsentRow]

(* An escrow draw is a delta, never an overwrite: what a transaction has
   drawn (bal) and sold (val) so far on a key. *)
Draw == [bal : Nat, val : Nat]
NoDraws == [k \in Keys |-> [bal |-> 0, val |-> 0]]

Init ==
  /\ status = [t \in Txns |-> "active"]
  /\ pc = [t \in Txns |-> 0]
  /\ held = [t \in Txns |-> {}]
  /\ wr = [t \in Txns |-> NoWrites]
  /\ draws = [t \in Txns |-> NoDraws]
  /\ resp = [t \in Txns |-> <<>>]
  /\ db = InitDb

Active(t) == status[t] = "active"
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

(* Admission guard: every registered resource must be compatible with
   every resource a live transaction holds. *)
Conflicting(t, R) ==
  {u \in Txns \ {t} : Active(u) /\ \E r \in R, h \in held[u] : ~Compatible(r, h)}

(* Escrow allowance (§5): a tentative draw already reduces what a
   concurrent draw may reserve, so the allowance is the committed balance
   minus every live tentative draw on the key; a draw that fits the
   committed balance but not the allowance waits on those holders. *)
TentativeDraw(u, k) == draws[u][k].bal

RECURSIVE SumDraws(_, _)
SumDraws(U, k) ==
  IF U = {} THEN 0
  ELSE LET u == CHOOSE x \in U : TRUE IN TentativeDraw(u, k) + SumDraws(U \ {u}, k)

DrawHolders(t, k) == {u \in Txns \ {t} : Active(u) /\ TentativeDraw(u, k) > 0}

AllowanceBlocks(t, op) ==
  /\ op.op = "Buy"
  /\ View(t)[op.k].bal >= op.a
  /\ View(t)[op.k].bal - SumDraws(DrawHolders(t, op.k), op.k) < op.a

(* The wait-for relation: who a running transaction is waiting on right
   now. Empty exactly when its next operation may execute. *)
WaitsOn(t) ==
  IF ~Running(t) THEN {}
  ELSE LET op == NextOp(t)
           C == Conflicting(t, Need(op, View(t))) IN
       IF C # {} THEN C
       ELSE IF AllowanceBlocks(t, op) THEN DrawHolders(t, op.k)
       ELSE {}

Changed(t, v) == {k \in Keys : v[k] # View(t)[k]}

(* Execute the next operation: register its resources (atomically with
   the check, §4 "atomically at dispatch") and apply it to the view. A
   constraint rejection ends the transaction with the caller's failure
   response and releases everything it held; nothing it wrote survives. *)
DoOp(t) ==
  /\ Running(t)
  /\ WaitsOn(t) = {}
  /\ LET op == NextOp(t)
         r == ApplyOp(op, View(t)) IN
     /\ pc' = [pc EXCEPT ![t] = @ + 1]
     /\ resp' = [resp EXCEPT ![t] = Append(@, r.resp)]
     /\ IF r.rejected
        THEN /\ status' = [status EXCEPT ![t] = "rejected"]
             /\ held' = [held EXCEPT ![t] = {}]
             /\ wr' = [wr EXCEPT ![t] = NoWrites]
             /\ draws' = [draws EXCEPT ![t] = NoDraws]
        ELSE /\ held' = [held EXCEPT ![t] = @ \cup Need(op, View(t))]
             /\ IF op.op = "Buy"
                THEN /\ draws' = [draws EXCEPT ![t][op.k] =
                                   [bal |-> @.bal + op.a, val |-> @.val + 1]]
                     /\ UNCHANGED wr
                ELSE /\ wr' = [wr EXCEPT ![t] =
                                [k \in DOMAIN @ \cup Changed(t, r.db) |->
                                   Undrawn(t, k, r.db[k])]]
                     /\ UNCHANGED draws
             /\ UNCHANGED status
  /\ UNCHANGED db

(* Commit applies the tentative writes and draws atomically - the view
   becomes the committed state - and releases every resource (strict
   two-phase locking: held from registration to commit). *)
Commit(t) ==
  /\ Active(t) /\ pc[t] = Len(Prog[t])
  /\ status' = [status EXCEPT ![t] = "committed"]
  /\ db' = View(t)
  /\ held' = [held EXCEPT ![t] = {}]
  /\ wr' = [wr EXCEPT ![t] = NoWrites]
  /\ draws' = [draws EXCEPT ![t] = NoDraws]
  /\ UNCHANGED <<pc, resp>>

Finished == \A t \in Txns : ~Active(t)

(* Self-loop once every transaction has ended, so only a state with a
   live transaction and no enabled step counts as the deadlock TLC's
   default check reports. *)
Terminating == Finished /\ UNCHANGED Vars

Next == (\E t \in Txns : DoOp(t) \/ Commit(t)) \/ Terminating

Spec == Init /\ [][Next]_Vars

----------------------------------------------------------------------------
TypeOK ==
  /\ status \in [Txns -> {"active", "committed", "rejected"}]
  /\ pc \in [Txns -> Nat]
  /\ \A t \in Txns : held[t] \subseteq Resource
  /\ \A t \in Txns : DOMAIN wr[t] \subseteq Keys /\ \A k \in DOMAIN wr[t] : wr[t][k] \in Row
  /\ draws \in [Txns -> [Keys -> Draw]]
  /\ db \in [Keys -> Row]

(* No two live transactions hold incompatible resources. *)
MutualExclusion ==
  \A t, u \in Txns : t # u /\ Active(t) /\ Active(u) =>
    \A a \in held[t], b \in held[u] : Compatible(a, b)

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
Ended == {t \in Txns : ~Active(t)}

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

============================================================================
