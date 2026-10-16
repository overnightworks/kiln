# E1 — TPC-C fidelity matrix

What a faithful lowering of TPC-C's five transactions must reproduce: the
inputs, accesses, responses, aborts and edge cases each clause of TPC-C
Revision 5.11 (`inputs.md` §1) demands. Every clause obligation is derived
from the clause text alone, never from a lowering, so it is the independent
expectation the lowering audit asks for (`inputs.md` §5, Q4.2). Where the
clause leaves a representation open, a row also pins an approved encoding
choice: *Source* says which a row holds — *Clause* for the obligation, an
E-number for each encoding, listed once under "Encodings" with its owner.
The audit checks both, but only the obligations are TPC-C's: a lowering
that meets every obligation with another encoding needs that encoding
approved by its owner, not a new clause reading. The IR meaning is owned by
`docs/concept.md` §2.2, the faithfulness and classification rules by
`inputs.md` §5 (Clarification 16.10.2026); this file states no category.
Pages are the specification's printed pages, which equal its PDF pages.

*Slice* names the #61 slice that owns the needed capability; "—" means the
current IR already has it. When #55 is regenerated (#61's done-when), each
row becomes test data: it holds when the lowered function, run through
`kiln-ir`, gives the stated input handling, access, response or abort.

## Encodings

| E | Encoding choice | Owner |
|---|---|---|
| E1 | Exact numerics are integers at the clause's declared scale per attribute. | `inputs.md` §5 Clarification rule 8; `docs/concept.md` D9 *Values* |
| E2 | Null, which the clause defines as one fixed out-of-range value per attribute (1.3.1, p. 12), is that sentinel value. | Rule 8 |
| E3 | The current date and time enter as host-supplied arguments, an integer of one-second resolution. | D9 *Host boundary*; rule 9 |
| E4 | A lookup by C_LAST or by O_C_ID is a scan: the clause leaves the access method open, E1 uses only declared indexes, and the specification declares none. | Q2.1, Q2.5; KILN-E1-30 |
| E5 | OL_NUMBER is k, the line's entry position. The clause asks only uniqueness within the order (N11); k is deterministic, as replay comparison needs (D4), lies in the population's [1 .. O_OL_CNT] (4.3.3.1, p. 68), and no response shows it (O7 lists lines in any order). | This file, decided in #64 |
| E6 | Customers with equal C_FIRST keep key order, which the clause leaves open. | D9 *Local sequences* (stable sort over a scan's key order) |
| E7 | New-Order is unrolled over every ol_cnt in [5 .. 15]. | Rule 8 |
| E8 | A row-count bound is the source's reachable maximum, cited; where none exists, a first or last selection is a limited read. | Rule 8 |
| E9 | The separators between C_DATA's prepended fields are the lowering's declared choice; field order and display formats stay the clause's. | Rule 8 (the lowering's declaration in #55) |
| E10 | total-amount stays unrounded at scale 10: the clause names no rounding and the display is the terminal's. | Rule 8; D9 *Values* |
| E11 | Delivery is one database transaction, so a missing row discards every district (the clause permits up to ten). | Rule 9 |
| E12 | Delivery's unit excludes the queue: its enqueue, dequeue and queued screen are host work. | Rule 9 |
| E13 | What Delivery's result file records is a response field, written by the host only after the commit. | D9 *Host boundary*; rule 9 |
| E14 | HISTORY, which has no primary key, is a keyless table written by append. | D9 *Append*; rule 6 |
| E15 | Payment's and Order-Status's by-id and by-name cases form one function each. | Q1.5 |

## Shared

| Row | Expected | Clause, page | Source | Slice |
|---|---|---|---|---|
| T1 | Keys in declared component order: WAREHOUSE (W_ID); DISTRICT (D_W_ID, D_ID); CUSTOMER (C_W_ID, C_D_ID, C_ID); NEW-ORDER (NO_W_ID, NO_D_ID, NO_O_ID); ORDER (O_W_ID, O_D_ID, O_ID); ORDER-LINE (OL_W_ID, OL_D_ID, OL_O_ID, OL_NUMBER); ITEM (I_ID); STOCK (S_W_ID, S_I_ID). HISTORY is keyless. | 1.3.1, pp. 12-17 | Clause, E14 | S1a, S2 |
| T2 | Ten foreign keys, each onto a whole primary key: DISTRICT (D_W_ID) and STOCK (S_W_ID) to WAREHOUSE; STOCK (S_I_ID) to ITEM; CUSTOMER (C_W_ID, C_D_ID) and HISTORY (H_W_ID, H_D_ID) to DISTRICT; HISTORY (H_C_W_ID, H_C_D_ID, H_C_ID) and ORDER (O_W_ID, O_D_ID, O_C_ID) to CUSTOMER; NEW-ORDER (NO_W_ID, NO_D_ID, NO_O_ID) and ORDER-LINE (OL_W_ID, OL_D_ID, OL_O_ID) to ORDER; ORDER-LINE (OL_SUPPLY_W_ID, OL_I_ID) to STOCK. | 1.3.1, pp. 13-17 | Clause | S1c |
| T3 | No secondary index and no unique constraint besides the keys; a lookup by C_LAST or O_C_ID is therefore a scan (E4). | 1.3.1, pp. 11-17 | Clause, E4 | S5b |
| T4 | Exact numerics as integers at a pinned scale: 2 for W_YTD, D_YTD, C_CREDIT_LIM, C_BALANCE, C_YTD_PAYMENT, H_AMOUNT, OL_AMOUNT, I_PRICE; 4 for W_TAX, D_TAX, C_DISCOUNT; signed where declared. | 1.3.1, pp. 12-17 | Clause, E1 | — |
| T5 | Only O_CARRIER_ID and OL_DELIVERY_D may be null; null is one fixed out-of-range value (E2). Neither is a key or foreign-key column. | 1.3.1, pp. 12, 16 | Clause, E2 | — |
| T6 | O_ENTRY_D, H_DATE and OL_DELIVERY_D are the current date and time, supplied by the host as call arguments; date and time as an integer of at least one-second resolution. | 2.4.1.6, p. 29; 2.5.1.4, p. 33; 2.7.1.3, p. 40; 1.3.1, p. 12 | Clause, E3 | S3 |
| T7 | A required row that is absent, or a failed insert or delete, is detected, reported and rolled back: a typed abort with no effect. Where the population always provides the row, this is the only abort a lowering may add. | 2.3.6, p. 27; 4.3.3.1, pp. 65-68 | Clause | — |
| T8 | Bounds the source reaches: at most 15 lines per order (O_OL_CNT and ol_cnt in [5 .. 15], New-Order the only writer of ORDER-LINE); exactly 3,000 customers per district (no transaction inserts one). NEW-ORDER and ORDER grow with every New-Order, so no row-count bound applies to them; a first or last selection there uses a limited read. | 2.4.1.3, p. 28; 2.4.2.2, pp. 29-30; 4.3.3.1, pp. 67-68 | Clause, E8 | S5a, S5b |

## New-Order (2.4)

| Row | Expected | Clause, page | Source | Slice |
|---|---|---|---|---|
| N1 | Input: W_ID, D_ID, C_ID; per line k: OL_I_ID, OL_SUPPLY_W_ID, OL_QUANTITY in [1 .. 10]; ol_cnt in [5 .. 15], taken from the entered group's length, never entered; O_ENTRY_D (T6). An unrolled lowering covers every ol_cnt from 5 to 15. | 2.4.1.2-2.4.1.5, p. 28; 2.4.3.2, p. 31 | Clause, E7 | — |
| N2 | WAREHOUSE point read (W_ID): W_TAX. | 2.4.2.2, p. 29 | Clause | — |
| N3 | DISTRICT point read (W_ID, D_ID): D_TAX, D_NEXT_O_ID; D_NEXT_O_ID incremented by one; O_ID is the value read. | 2.4.2.2, p. 29 | Clause | S1a |
| N4 | CUSTOMER point read (W_ID, D_ID, C_ID): C_DISCOUNT, C_LAST, C_CREDIT. | 2.4.2.2, p. 29 | Clause | S1a |
| N5 | Insert ORDER (W_ID, D_ID, O_ID): O_C_ID = C_ID, O_ENTRY_D, O_CARRIER_ID null (T5), O_OL_CNT = ol_cnt, O_ALL_LOCAL 1 when every OL_SUPPLY_W_ID equals W_ID, else 0. | 2.4.2.2, p. 29; 2.4.1.7, p. 29 | Clause, E2 | S1a, S4a |
| N6 | Insert NEW-ORDER (W_ID, D_ID, O_ID). | 2.4.2.2, p. 29 | Clause | S1a |
| N7 | Per line, in entry order: ITEM point read (OL_I_ID): I_PRICE, I_NAME, I_DATA. | 2.4.2.2, p. 30 | Clause | — |
| N8 | Per line: STOCK point read (OL_SUPPLY_W_ID, OL_I_ID): S_QUANTITY, S_DIST_xx for xx = D_ID (all ten values), S_DATA. S_QUANTITY becomes q − OL_QUANTITY when q ≥ OL_QUANTITY + 10, else q − OL_QUANTITY + 91; S_YTD += OL_QUANTITY; S_ORDER_CNT += 1; S_REMOTE_CNT += 1 when OL_SUPPLY_W_ID ≠ W_ID. | 2.4.2.2, p. 30 | Clause | S1a, S4a |
| N9 | OL_AMOUNT = OL_QUANTITY × I_PRICE, exact at scale 2. | 2.4.2.2, p. 30 | Clause, E1 | S4a |
| N10 | brand-generic is "B" when I_DATA and S_DATA both contain "ORIGINAL", else "G". | 2.4.2.2, p. 30 | Clause | S4a, S4b |
| N11 | Insert ORDER-LINE (W_ID, D_ID, O_ID, OL_NUMBER), OL_NUMBER unique within the order (= k, E5): OL_I_ID, OL_SUPPLY_W_ID, OL_DELIVERY_D null, OL_QUANTITY, OL_AMOUNT, OL_DIST_INFO = S_DIST_xx. | 2.4.2.2, p. 30 | Clause, E2, E5 | S1a |
| N12 | total-amount = sum(OL_AMOUNT) × (1 − C_DISCOUNT) × (1 + W_TAX + D_TAX), exact at scale 10 and unrounded (E10). At the declared maxima it stays below 9.1 × 10^15, inside `i64`. | 2.4.2.2, p. 30; 2.4.3.3, p. 31 | Clause, E1, E10 | S4a |
| N13 | Committed response (S_QUANTITY as retrieved is the reading of 2.4.2.2 and 2.4.3.3 adopted in #64): W_ID, D_ID, C_ID, O_ID, O_OL_CNT, C_LAST, C_CREDIT, C_DISCOUNT, W_TAX, D_TAX, O_ENTRY_D, total-amount; per line in entry order: OL_SUPPLY_W_ID, OL_I_ID, I_NAME, OL_QUANTITY, S_QUANTITY as retrieved (N8), brand-generic, I_PRICE, OL_AMOUNT. | 2.4.3.3, pp. 31-32 | Clause | S3, S6d |
| N14 | Abort: an OL_I_ID with no ITEM row, at any position (the generator puts it last when rbk = 1), rolls back every effect: the D_NEXT_O_ID increment, ORDER, NEW-ORDER, earlier STOCK updates and ORDER-LINE rows. | 2.4.1.4-2.4.1.5, p. 28; 2.4.2.3, p. 30 | Clause | — |
| N15 | Abort payload: W_ID, D_ID, C_ID, C_LAST, C_CREDIT, the allocated O_ID, and "Item number is not valid". | 2.4.3.4, p. 32 | Clause | S3 |
| N16 | Before the abort, every earlier line is processed in full; the unused item's STOCK read and ORDER-LINE insert are not required, and nothing else is skipped. The aborted call keeps its attempted accesses (D5). | 2.4.2.3, p. 30 | Clause | — |
| N17 | Edge: one item on two lines from the same warehouse: the second STOCK read sees the first line's update; no abort. | 2.4.1.5, p. 28; 2.4.2.2, p. 30 | Clause | — |
| N18 | Edge: a remote line sets S_REMOTE_CNT and O_ALL_LOCAL = 0; both arms of every such argument test are covered. | 2.4.1.8, p. 29; 2.4.2.2, pp. 29-30 | Clause | S4a |
| N19 | No abort besides N14 and T7: no row-count bound (lines unrolled per ol_cnt), no overflow at legal values (N12). | 2.4.2.2, pp. 29-30 | Clause, E7, E8 | — |

## Payment (2.5)

| Row | Expected | Clause, page | Source | Slice |
|---|---|---|---|---|
| P1 | Input: W_ID, D_ID, C_W_ID, C_D_ID, either C_ID or C_LAST, H_AMOUNT in [1.00 .. 5,000.00]; H_DATE (T6). The entered field selects the case; both cases form one function (Q1.5). | 2.5.1, p. 33; 2.5.3.2, p. 35 | Clause, E3, E15 | — |
| P2 | WAREHOUSE point read (W_ID): W_NAME, W_STREET_1, W_STREET_2, W_CITY, W_STATE, W_ZIP; W_YTD += H_AMOUNT. | 2.5.2.2, p. 34 | Clause | — |
| P3 | DISTRICT point read (W_ID, D_ID): D_NAME, D_STREET_1, D_STREET_2, D_CITY, D_STATE, D_ZIP; D_YTD += H_AMOUNT. | 2.5.2.2, p. 34 | Clause | S1a |
| P4 | By id: CUSTOMER point read (C_W_ID, C_D_ID, C_ID): C_FIRST, C_MIDDLE, C_LAST, C_STREET_1, C_STREET_2, C_CITY, C_STATE, C_ZIP, C_PHONE, C_SINCE, C_CREDIT, C_CREDIT_LIM, C_DISCOUNT, C_BALANCE; C_BALANCE −= H_AMOUNT; C_YTD_PAYMENT += H_AMOUNT; C_PAYMENT_CNT += 1. | 2.5.2.2, p. 34 | Clause | S1a |
| P5 | By name: scan of CUSTOMER prefix (C_W_ID, C_D_ID) for C_LAST = input, sorted by C_FIRST ascending; of the n rows, the one at position ⌈n/2⌉ gives C_ID and the P4 fields and takes the P4 updates. Bound 3,000 (T8). | 2.5.2.2, p. 34 | Clause, E4, E8 | S5b, S6c |
| P6 | Edge: n = 1, 2, 3, 4 select positions 1, 1, 2, 2; rows with equal C_FIRST keep key order (E6). | 2.5.2.2, p. 34 | Clause, E6 | S6c |
| P7 | Edge: n = 0 never arises (every district holds all 1,000 last names) and aborts as a missing record (T7). | 4.3.3.1, p. 67; 2.3.6, p. 27 | Clause | S6c |
| P8 | C_CREDIT = "BC": C_DATA is read; C_ID, C_D_ID, C_W_ID, D_ID, W_ID and H_AMOUNT, in that order and in their output fields' display format (separators E9), go in front of it, and the result is cut to 500 characters and written. "GC": C_DATA is neither read nor written. | 2.5.2.2, pp. 34-35 | Clause, E9 | S4b, S4c |
| P9 | H_DATA = W_NAME, four spaces, D_NAME. | 2.5.2.2, p. 35 | Clause | S4b |
| P10 | Append HISTORY: H_C_ID = the customer's C_ID, H_C_D_ID = C_D_ID, H_C_W_ID = C_W_ID, H_D_ID = D_ID, H_W_ID = W_ID, H_DATE, H_AMOUNT, H_DATA. Two identical payments leave two rows; both foreign keys are checked. | 2.5.2.2, p. 35; 1.3.1, p. 15 | Clause, E14 | S2, S1c |
| P11 | Response: W_ID, D_ID, C_ID, C_D_ID, C_W_ID, W_STREET_1, W_STREET_2, W_CITY, W_STATE, W_ZIP, D_STREET_1, D_STREET_2, D_CITY, D_STATE, D_ZIP, C_FIRST, C_MIDDLE, C_LAST, C_STREET_1, C_STREET_2, C_CITY, C_STATE, C_ZIP, C_PHONE, C_SINCE, C_CREDIT, C_CREDIT_LIM, C_DISCOUNT, C_BALANCE after the payment, H_AMOUNT, H_DATE, and only when "BC" the first 200 characters of the new C_DATA. | 2.5.3.3, p. 36; screen, p. 35 | Clause | S3, S4b |
| P12 | Edge: C_BALANCE may become negative (signed); no abort. | 1.3.1, p. 14 | Clause | — |
| P13 | No abort besides T7. | 2.5.2.2, pp. 34-35 | Clause | — |

## Order-Status (2.6)

| Row | Expected | Clause, page | Source | Slice |
|---|---|---|---|---|
| O1 | Input: W_ID, D_ID, either C_ID or C_LAST; C_W_ID = W_ID, C_D_ID = D_ID. | 2.6.1.2, p. 37; 2.6.3.2, p. 38 | Clause | — |
| O2 | By id: CUSTOMER point read (W_ID, D_ID, C_ID): C_BALANCE, C_FIRST, C_MIDDLE, C_LAST. | 2.6.2.2, p. 37 | Clause | S1a |
| O3 | By name: as P5-P7, retrieving these fields. | 2.6.2.2, p. 38 | Clause, E4, E6 | S5b, S6c |
| O4 | ORDER: among rows of prefix (W_ID, D_ID) with O_C_ID = the customer's C_ID, the largest O_ID: O_ID, O_ENTRY_D, O_CARRIER_ID. No index on O_C_ID (T3): a scan of the prefix, last match in key order, no row-count bound (T8). | 2.6.2.2, p. 38 | Clause, E4, E8 | S5b |
| O5 | Edge: a customer without an order never arises (the population gives each one) and aborts (T7). | 4.3.3.1, p. 68 | Clause | S5b |
| O6 | ORDER-LINE prefix range (W_ID, D_ID, O_ID), every line: OL_I_ID, OL_SUPPLY_W_ID, OL_QUANTITY, OL_AMOUNT, OL_DELIVERY_D. Bound 15 (T8). | 2.6.2.2, p. 38 | Clause, E8 | S1b |
| O7 | Response: W_ID, D_ID, C_ID, C_FIRST, C_MIDDLE, C_LAST, C_BALANCE, O_ID, O_ENTRY_D, O_CARRIER_ID (null when undelivered); per line, in any order: OL_SUPPLY_W_ID, OL_I_ID, OL_QUANTITY, OL_AMOUNT, OL_DELIVERY_D (null when undelivered). | 2.6.3.3, p. 39 | Clause, E2 | S3, S6d |
| O8 | No write; no abort besides T7. | 2.6.2.2, pp. 37-38 | Clause | — |

## Delivery (2.7)

| Row | Expected | Clause, page | Source | Slice |
|---|---|---|---|---|
| D1 | Input: W_ID, O_CARRIER_ID in [1 .. 10], OL_DELIVERY_D (T6). The queue times are the host's; enqueue, dequeue and the "Delivery has been queued" screen lie outside the unit (`inputs.md` §5, Clarification 16.10.2026, rule 9). | 2.7.1, p. 40; 2.7.2.2, p. 40; 2.7.3.3, p. 41 | Clause, E3, E12 | S3 |
| D2 | Every district D_ID = 1 .. 10 is processed, in any order. | 2.7.4.2, p. 42; 2.3.1, p. 26 | Clause | — |
| D3 | NEW-ORDER: the row of prefix (W_ID, D_ID) with the lowest NO_O_ID gives NO_O_ID: the first row in key order of that prefix range, with no row-count bound (T8). | 2.7.4.2, p. 42 | Clause, E8 | S5a |
| D4 | Edge: no such row skips this district only; the others are still delivered, and the skip shows in the response. | 2.7.4.2, p. 42 | Clause, E13 | S5a, S6a |
| D5 | Delete NEW-ORDER (W_ID, D_ID, NO_O_ID). | 2.7.4.2, p. 43 | Clause | S1a |
| D6 | ORDER point read (W_ID, D_ID, NO_O_ID): O_C_ID; O_CARRIER_ID set to the input. | 2.7.4.2, p. 43 | Clause | S1a |
| D7 | ORDER-LINE prefix range (W_ID, D_ID, NO_O_ID): each OL_DELIVERY_D set to the input time; OL_AMOUNT summed. Bound 15 (T8). | 2.7.4.2, p. 43 | Clause, E8 | S1b, S6b |
| D8 | CUSTOMER point read (W_ID, D_ID, O_C_ID): C_BALANCE += the sum; C_DELIVERY_CNT += 1. | 2.7.4.2, p. 43 | Clause | S1a |
| D9 | Abort: a missing ORDER or CUSTOMER row aborts (T7); as one database transaction, that discards every district's delivery, an encoding the clause permits. | 2.7.4.1, p. 42; 2.3.6, p. 27 | Clause, E11 | — |
| D10 | Response: W_ID, O_CARRIER_ID and (D_ID, O_ID) per delivered order, so the skipped districts are the missing ones. The host writes the result file only after the commit. | 2.7.2.2, p. 40; 2.7.2.3, p. 41; 2.7.4.2, p. 42 | Clause, E13 | S3, S6d |

## Stock-Level (2.8)

| Row | Expected | Clause, page | Source | Slice |
|---|---|---|---|---|
| L1 | Input: W_ID, D_ID (fixed per terminal), threshold in [10 .. 20]. | 2.8.1, p. 44 | Clause | — |
| L2 | DISTRICT point read (W_ID, D_ID): D_NEXT_O_ID. | 2.8.2.2, p. 44 | Clause | S1a |
| L3 | ORDER-LINE prefix (W_ID, D_ID) with OL_O_ID in [D_NEXT_O_ID − 20, D_NEXT_O_ID − 1]. Bound 300: 20 orders of at most 15 lines (T8). | 2.8.2.2, p. 44 | Clause, E8 | S1b |
| L4 | The distinct OL_I_ID of those lines; an item on several lines counts once. | 2.8.2.2, p. 44 | Clause | S6b |
| L5 | Per distinct item: STOCK point read (W_ID, OL_I_ID): S_QUANTITY; low_stock counts the items with S_QUANTITY < threshold. | 2.8.2.2, p. 44 | Clause | S6a, S6b |
| L6 | Edge: an empty line range gives low_stock = 0, no abort. | 2.8.2.2, p. 44 | Clause | S6b |
| L7 | Response: W_ID, D_ID, threshold, low_stock. | 2.8.3.3, p. 45 | Clause | S3 |
| L8 | Relaxed isolation and a split into several database transactions are permitted; the unit stays one function under the serial reference meaning. | 2.8.2.3, p. 45 | Clause | — |
