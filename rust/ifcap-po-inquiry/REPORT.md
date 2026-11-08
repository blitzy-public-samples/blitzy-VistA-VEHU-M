# IFCAP purchase-order display (PRCH DISPLAY PO) — Rust port report

## Comparison

<!-- COMPARISON BEGIN -->
Written by `comparison_report` in `tests/equivalence.rs`, the only writer of this section. NOT COMPARED means a required file is missing; exit 0 of the equivalence suite never means equivalence is accepted.

**Totals:** PASS 0, FAIL 0, NOT COMPARED 5

### GC-01 — gc_01_standard_order

- **Case:** existing order with one line (2 × $125.50)
- **Actual test name:** `gc_01_standard_order`
- **Status:** NOT COMPARED
- **First difference:** `missing: tests/golden/ifcap/GC-01/input.txt, tests/golden/ifcap/GC-01/transcript.txt, tests/golden/ifcap/GC-01/returned.txt, tests/golden/ifcap/GC-01/db-before.zwr, tests/golden/ifcap/GC-01/db-after.zwr, tests/golden/ifcap/GC-01/meta.txt, tests/golden/ifcap/GC-01/normalisation.txt, tests/fixtures/ifcap/cases.zwr`
- **Predicted contracts and branches:** PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b8), PO-10 to PO-40, then PO-09 (b3) (BLOCKED at PO-12.b1 while O-01 is open)
- **Actual branch:** NOT EVALUATED (readiness: required files missing)
- **Readiness: NOT READY, 8 required files missing:**
  - `tests/golden/ifcap/GC-01/input.txt`
  - `tests/golden/ifcap/GC-01/transcript.txt`
  - `tests/golden/ifcap/GC-01/returned.txt`
  - `tests/golden/ifcap/GC-01/db-before.zwr`
  - `tests/golden/ifcap/GC-01/db-after.zwr`
  - `tests/golden/ifcap/GC-01/meta.txt`
  - `tests/golden/ifcap/GC-01/normalisation.txt`
  - `tests/fixtures/ifcap/cases.zwr`
- **Inputs:** NOT EVALUATED (readiness: required files missing)
- **Fixture hashes:** NOT EVALUATED (readiness: required files missing)
- **Capture hashes:** NOT EVALUATED (readiness: required files missing)
- **Hash summary:** NOT EVALUATED (readiness: required files missing)
- **Meta:** NOT EVALUATED (readiness: required files missing)
- **Context:** expected (`STUB_CONTEXT`) `DUZ=".5"`, `IOF=UNRESOLVED`, `IOSL=UNRESOLVED`, `STATION="500"`; NOT EVALUATED (readiness: required files missing)
- **Normalisation rules:** NOT EVALUATED (readiness: required files missing)
- **Expected returned:** NOT EVALUATED (readiness: required files missing)
- **Actual returned:** NOT EVALUATED (readiness: required files missing)
- **Returned comparison:** NOT EVALUATED (readiness: required files missing)
- **Transcript comparison:** NOT EVALUATED (readiness: required files missing)
- **Baseline (db-before.zwr against the fixture store):** NOT EVALUATED (readiness: required files missing)
- **Snapshot identity:** NOT EVALUATED (readiness: required files missing)
- **Original side (db-before.zwr against db-after.zwr):** NOT EVALUATED (readiness: required files missing)
- **Rust side:** NOT EVALUATED (readiness: required files missing)
- **Failures:** none

### GC-02 — gc_02_multiple_lines_and_decimals

- **Case:** three lines (3 × $19.95, 2 × $125.50, 1 × $0.99)
- **Actual test name:** `gc_02_multiple_lines_and_decimals`
- **Status:** NOT COMPARED
- **First difference:** `missing: tests/golden/ifcap/GC-02/input.txt, tests/golden/ifcap/GC-02/transcript.txt, tests/golden/ifcap/GC-02/returned.txt, tests/golden/ifcap/GC-02/db-before.zwr, tests/golden/ifcap/GC-02/db-after.zwr, tests/golden/ifcap/GC-02/meta.txt, tests/golden/ifcap/GC-02/normalisation.txt, tests/fixtures/ifcap/cases.zwr`
- **Predicted contracts and branches:** PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b8), PO-10 to PO-40, then PO-09 (b3) (BLOCKED at PO-12.b1 while O-01 is open)
- **Actual branch:** NOT EVALUATED (readiness: required files missing)
- **Readiness: NOT READY, 8 required files missing:**
  - `tests/golden/ifcap/GC-02/input.txt`
  - `tests/golden/ifcap/GC-02/transcript.txt`
  - `tests/golden/ifcap/GC-02/returned.txt`
  - `tests/golden/ifcap/GC-02/db-before.zwr`
  - `tests/golden/ifcap/GC-02/db-after.zwr`
  - `tests/golden/ifcap/GC-02/meta.txt`
  - `tests/golden/ifcap/GC-02/normalisation.txt`
  - `tests/fixtures/ifcap/cases.zwr`
- **Inputs:** NOT EVALUATED (readiness: required files missing)
- **Fixture hashes:** NOT EVALUATED (readiness: required files missing)
- **Capture hashes:** NOT EVALUATED (readiness: required files missing)
- **Hash summary:** NOT EVALUATED (readiness: required files missing)
- **Meta:** NOT EVALUATED (readiness: required files missing)
- **Context:** expected (`STUB_CONTEXT`) `DUZ=".5"`, `IOF=UNRESOLVED`, `IOSL=UNRESOLVED`, `STATION="500"`; NOT EVALUATED (readiness: required files missing)
- **Normalisation rules:** NOT EVALUATED (readiness: required files missing)
- **Expected returned:** NOT EVALUATED (readiness: required files missing)
- **Actual returned:** NOT EVALUATED (readiness: required files missing)
- **Returned comparison:** NOT EVALUATED (readiness: required files missing)
- **Transcript comparison:** NOT EVALUATED (readiness: required files missing)
- **Baseline (db-before.zwr against the fixture store):** NOT EVALUATED (readiness: required files missing)
- **Snapshot identity:** NOT EVALUATED (readiness: required files missing)
- **Original side (db-before.zwr against db-after.zwr):** NOT EVALUATED (readiness: required files missing)
- **Rust side:** NOT EVALUATED (readiness: required files missing)
- **Failures:** none

### GC-03 — gc_03_missing_order

- **Case:** syntactically valid identifier absent from the fixture database
- **Actual test name:** `gc_03_missing_order`
- **Status:** NOT COMPARED
- **First difference:** `missing: tests/golden/ifcap/GC-03/input.txt, tests/golden/ifcap/GC-03/transcript.txt, tests/golden/ifcap/GC-03/returned.txt, tests/golden/ifcap/GC-03/db-before.zwr, tests/golden/ifcap/GC-03/db-after.zwr, tests/golden/ifcap/GC-03/meta.txt, tests/golden/ifcap/GC-03/normalisation.txt, tests/fixtures/ifcap/cases.zwr`
- **Predicted contracts and branches:** PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b14), then PO-09 (b3) (BLOCKED at PO-09.b14 while O-03 is open)
- **Actual branch:** NOT EVALUATED (readiness: required files missing)
- **Readiness: NOT READY, 8 required files missing:**
  - `tests/golden/ifcap/GC-03/input.txt`
  - `tests/golden/ifcap/GC-03/transcript.txt`
  - `tests/golden/ifcap/GC-03/returned.txt`
  - `tests/golden/ifcap/GC-03/db-before.zwr`
  - `tests/golden/ifcap/GC-03/db-after.zwr`
  - `tests/golden/ifcap/GC-03/meta.txt`
  - `tests/golden/ifcap/GC-03/normalisation.txt`
  - `tests/fixtures/ifcap/cases.zwr`
- **Inputs:** NOT EVALUATED (readiness: required files missing)
- **Fixture hashes:** NOT EVALUATED (readiness: required files missing)
- **Capture hashes:** NOT EVALUATED (readiness: required files missing)
- **Hash summary:** NOT EVALUATED (readiness: required files missing)
- **Meta:** NOT EVALUATED (readiness: required files missing)
- **Context:** expected (`STUB_CONTEXT`) `DUZ=".5"`, `IOF=UNRESOLVED`, `IOSL=UNRESOLVED`, `STATION="500"`; NOT EVALUATED (readiness: required files missing)
- **Normalisation rules:** NOT EVALUATED (readiness: required files missing)
- **Expected returned:** NOT EVALUATED (readiness: required files missing)
- **Actual returned:** NOT EVALUATED (readiness: required files missing)
- **Returned comparison:** NOT EVALUATED (readiness: required files missing)
- **Transcript comparison:** NOT EVALUATED (readiness: required files missing)
- **Baseline (db-before.zwr against the fixture store):** NOT EVALUATED (readiness: required files missing)
- **Snapshot identity:** NOT EVALUATED (readiness: required files missing)
- **Original side (db-before.zwr against db-after.zwr):** NOT EVALUATED (readiness: required files missing)
- **Rust side:** NOT EVALUATED (readiness: required files missing)
- **Failures:** none

### GC-04 — gc_04_invalid_input

- **Case:** `INVALID!` at the order-selection prompt, then `^`
- **Actual test name:** `gc_04_invalid_input`
- **Status:** NOT COMPARED
- **First difference:** `missing: tests/golden/ifcap/GC-04/input.txt, tests/golden/ifcap/GC-04/transcript.txt, tests/golden/ifcap/GC-04/returned.txt, tests/golden/ifcap/GC-04/db-before.zwr, tests/golden/ifcap/GC-04/db-after.zwr, tests/golden/ifcap/GC-04/meta.txt, tests/golden/ifcap/GC-04/normalisation.txt, tests/fixtures/ifcap/cases.zwr`
- **Predicted contracts and branches:** PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b14), then PO-09 (b3) (BLOCKED at PO-09.b14 while O-03 is open)
- **Actual branch:** NOT EVALUATED (readiness: required files missing)
- **Readiness: NOT READY, 8 required files missing:**
  - `tests/golden/ifcap/GC-04/input.txt`
  - `tests/golden/ifcap/GC-04/transcript.txt`
  - `tests/golden/ifcap/GC-04/returned.txt`
  - `tests/golden/ifcap/GC-04/db-before.zwr`
  - `tests/golden/ifcap/GC-04/db-after.zwr`
  - `tests/golden/ifcap/GC-04/meta.txt`
  - `tests/golden/ifcap/GC-04/normalisation.txt`
  - `tests/fixtures/ifcap/cases.zwr`
- **Inputs:** NOT EVALUATED (readiness: required files missing)
- **Fixture hashes:** NOT EVALUATED (readiness: required files missing)
- **Capture hashes:** NOT EVALUATED (readiness: required files missing)
- **Hash summary:** NOT EVALUATED (readiness: required files missing)
- **Meta:** NOT EVALUATED (readiness: required files missing)
- **Context:** expected (`STUB_CONTEXT`) `DUZ=".5"`, `IOF=UNRESOLVED`, `IOSL=UNRESOLVED`, `STATION="500"`; NOT EVALUATED (readiness: required files missing)
- **Normalisation rules:** NOT EVALUATED (readiness: required files missing)
- **Expected returned:** NOT EVALUATED (readiness: required files missing)
- **Actual returned:** NOT EVALUATED (readiness: required files missing)
- **Returned comparison:** NOT EVALUATED (readiness: required files missing)
- **Transcript comparison:** NOT EVALUATED (readiness: required files missing)
- **Baseline (db-before.zwr against the fixture store):** NOT EVALUATED (readiness: required files missing)
- **Snapshot identity:** NOT EVALUATED (readiness: required files missing)
- **Original side (db-before.zwr against db-after.zwr):** NOT EVALUATED (readiness: required files missing)
- **Rust side:** NOT EVALUATED (readiness: required files missing)
- **Failures:** none

### GC-05 — gc_05_cancelled_order

- **Case:** existing cancelled order with retained line-item history
- **Actual test name:** `gc_05_cancelled_order`
- **Status:** NOT COMPARED
- **First difference:** `missing: tests/golden/ifcap/GC-05/input.txt, tests/golden/ifcap/GC-05/transcript.txt, tests/golden/ifcap/GC-05/returned.txt, tests/golden/ifcap/GC-05/db-before.zwr, tests/golden/ifcap/GC-05/db-after.zwr, tests/golden/ifcap/GC-05/meta.txt, tests/golden/ifcap/GC-05/normalisation.txt, tests/fixtures/ifcap/cases.zwr`
- **Predicted contracts and branches:** PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b8), PO-10 to PO-40, then PO-09 (b3) (BLOCKED at PO-12.b1 while O-01 is open)
- **Actual branch:** NOT EVALUATED (readiness: required files missing)
- **Readiness: NOT READY, 8 required files missing:**
  - `tests/golden/ifcap/GC-05/input.txt`
  - `tests/golden/ifcap/GC-05/transcript.txt`
  - `tests/golden/ifcap/GC-05/returned.txt`
  - `tests/golden/ifcap/GC-05/db-before.zwr`
  - `tests/golden/ifcap/GC-05/db-after.zwr`
  - `tests/golden/ifcap/GC-05/meta.txt`
  - `tests/golden/ifcap/GC-05/normalisation.txt`
  - `tests/fixtures/ifcap/cases.zwr`
- **Inputs:** NOT EVALUATED (readiness: required files missing)
- **Fixture hashes:** NOT EVALUATED (readiness: required files missing)
- **Capture hashes:** NOT EVALUATED (readiness: required files missing)
- **Hash summary:** NOT EVALUATED (readiness: required files missing)
- **Meta:** NOT EVALUATED (readiness: required files missing)
- **Context:** expected (`STUB_CONTEXT`) `DUZ=".5"`, `IOF=UNRESOLVED`, `IOSL=UNRESOLVED`, `STATION="500"`; NOT EVALUATED (readiness: required files missing)
- **Normalisation rules:** NOT EVALUATED (readiness: required files missing)
- **Expected returned:** NOT EVALUATED (readiness: required files missing)
- **Actual returned:** NOT EVALUATED (readiness: required files missing)
- **Returned comparison:** NOT EVALUATED (readiness: required files missing)
- **Transcript comparison:** NOT EVALUATED (readiness: required files missing)
- **Baseline (db-before.zwr against the fixture store):** NOT EVALUATED (readiness: required files missing)
- **Snapshot identity:** NOT EVALUATED (readiness: required files missing)
- **Original side (db-before.zwr against db-after.zwr):** NOT EVALUATED (readiness: required files missing)
- **Rust side:** NOT EVALUATED (readiness: required files missing)
- **Failures:** none

## Stubs

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`; `4+INSTITUTION.zwr` and `200+NEW PERSON.zwr` are `Packages/Kernel/Globals/`; the 411 export is `Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr`.

`STUB_CONTEXT` in `src/inquiry.rs` is exactly `[("DUZ", Some(".5")), ("IOF", None), ("IOSL", None), ("STATION", Some("500"))]`. Every capture's `meta.txt` `context` is validated against it key by key: a missing key, an extra key, a different value or an unresolved (`None`) constant all fail, and no capture value overrides a constant. Until O-01 closes, every capture fails with `CONTEXT MISMATCH IOF: expected UNRESOLVED, actual <value>`.

| Stub | Original source | Fixed value or behaviour | Status |
|---|---|---|---|
| `DUZ` | Test code `D DUZ^XUP(.5)` [ZOSVGUT1.m:13]; POSTMASTER `^VA(200,.5,0)` [200+NEW PERSON.zwr:4]; node 20 piece 2 `POSTMASTER` [200+NEW PERSON.zwr:12], so `D A` never writes [PRCFSITE.m:38]. The request cites tech-spec section 9.1 for this test code; the code is in `ZOSVGUT1.m`, and 9.1 records only station 500 (conflict C10) | `".5"` | SOURCE-RESOLVED |
| `STATION` | Default `$O(^PRC(411,"AC","Y",0))` [PRCFSITE.m:14] = 500 [411 export:290]; accepted by RETURN [PRCFSITE.m:18] | `"500"` | SOURCE-RESOLVED |
| `IOF` | `W:$Y>0 @IOF` [PRCHDP1.m:12] [PRCHDP2.m:42]; set by `^%ZIS` from the HOME device's terminal type, default `"#"` [ZIS1.m:103, 106, 113] | UNRESOLVED (`None`) | BLOCKED (O-01) |
| `IOSL` | `$Y+5>IOSL` [PRCHDP2.m:31, 54]; set by `^%ZIS` [ZIS1.m:103-104] | UNRESOLVED (`None`) | BLOCKED (O-01) |
| `prcfsite` + `dic` output (PO-03 to PO-05) | `W !` [PRCFSITE.m:8]; `W !` [PRCFSITE.m:18]; `W !` [DIC.m:25]; prompt and default [DIC11.m:15-17]; echo [DIC2.m:10, 32] through `^DD(411,0,"ID",.1)` [DD.zwr:488724] and `^DIC(4,500,0)="CAMP MASTER^…"` [4+INSTITUTION.zwr:494] | Writes `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// `; consumes one input line; an empty line writes `      CAMP MASTER` (six spaces, no newline) and sets `PRC("SITE")=500`; a non-empty line returns `UnsupportedBranch("PO-05.b2")`; no line left is `Halt::InputExhausted`. Reads nothing from `Store` | SOURCE-RESOLVED (byte trace R1 in SPEC.md) |
| `duz_prcfsite` (PO-04) | [PRCFSITE.m:35-40] | No output; no `Store` read; `PRC("PER")` not modelled because no output reads it | SOURCE-RESOLVED |
| `ix_dic` lookup (PO-09) | `IX^DIC` [DIC.m:84-86] and the lookup routines | Lookup rules over `Store` data (R5, R6 in SPEC.md), never a list of known identifiers. The unique-match echo is resolved; the search-exhausted outcome PO-09.b14 is BLOCKED by O-03 | b3, b8 SOURCE-RESOLVED; b14 BLOCKED (O-03) |
| `pct_zis` (PO-12) | [ZIS.m:28-37] [ZIS1.m:103-113] | Always `UnsupportedBranch("PO-12.b1")` | BLOCKED (O-01) |
| `pct_dt` (PO-41) | `D ^%DT` with `%DT=""`, `X="T"` [PRCFSITE.m:15]; `%DT` writes only under the `A` or `E` flag [DIDT.m:11-12, 63, 67, 92] [ZTDT.m:3-4, 43, 46, 55] | No output and no value. The date feeds only `PRC("FY")`, `PRC("QTR")`, `A` and `B` [PRCFSITE.m:15]; `OUT` kills `A`, `B`, `X`, `Y` and leaves `PRC("FY")`, `PRC("QTR")` defined [PRCFSITE.m:29]; their only readers on the path are the copy into `X` at `FY`, which `SE1` overwrites [PRCFSITE.m:21, 26], and the year and quarter prompts and budget line that `PRCF("X")="SP"` skips [PRCFSITE.m:20, 23, 28] [PRCHRPT.m:5]; `EN3` runs `ST` once [PRCHRPT.m:70-75] and no display routine reads them. So they are omitted | SOURCE-RESOLVED (runtime identity O-04) |
| Control point | No control-point read or prompt anywhere on the `EN3` path (conflict C11) | None needed | SOURCE-RESOLVED |
| Current date and time | Not stubbed: the one date the path computes, through `^%DT`, reaches no output | Handled only through `normalisation.txt`, expected empty | Team confirms with the captures |
| `DTIME`, `U`, `DT` | `R X:DTIME` [PRCHDP2.m:10, 41]; `DT` [PRCFSITE.m:7]; `U` [PRCFSITE.m:9] | Preconditions of the capture command only; the Rust READ never times out | README states them |

## Flagged hard-MUMPS features

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`.

Rule (conflict C8): a hard feature whose operand is a literal fixed in the pinned source or data dictionary, on an exercised path, is source-resolved and reproduced as fixed logic, and is still listed here. A feature whose operand depends on runtime state is flagged "requires runtime shim, not converted", and its branch is BLOCKED.

| Feature | Routine and line | Resolution | Branch effect |
|---|---|---|---|
| `D ^%ZIS` device open, then `W:$Y>0 @IOF` | [PRCHDP1.m:12] | Requires runtime shim, not converted: `IOF` depends on the HOME device | PO-12.b1 and PO-10.b4 BLOCKED; a found order returns `UnsupportedBranch("PO-12.b1")`, the first BLOCKED branch it reaches |
| `W:$Y>0 @IOF,!!…` column header | [PRCHDP2.m:42] [PRCHDP4.m:35] | Requires runtime shim, not converted; the postcondition governs every argument of the WRITE | PO-26.b1 BLOCKED; the `PRCHDP4` occurrence is n/e |
| `$Y+5>IOSL` paging | [PRCHDP2.m:31, 54] | Requires runtime shim, not converted: `IOSL` unresolved | PO-30.b1 BLOCKED |
| `X DIC("S")` screen | [DIC3.m:124] | Literal built at [PRCHRPT.m:9] and [PRCHPAT.m:81]: `I +$P(^(0),U,1)=PRC("SITE"),$P(^(0),U,2)<10!($P(^(0),U,2)=25)`, the naked reference on `^PRC(442,ien,0)` [DIC3.m:122] | Resolved as a fixed predicate in `ix_dic` |
| Station screen `I +Y<1000000` | [PRCFSITE.m:18] [DIC3.m:124] | Literal | Resolved; 500 passes |
| Identifier XECUTE: `DIC("W")` built from `^DD(file,0,"ID",…)`; for 442 it overflows 224 characters and becomes `D WOV^DICQ1`, which XECUTEs each identifier after `W "  ",$E(^PRC(442,Y,0),0)` resets the naked reference | [DIC1.m:46-60] [DICQ1.m:86-88] [DD.zwr:514202-514204] | Literal DD code; output reproduced as fixed logic, never by executing the code (R2 in SPEC.md) | Resolved |
| Station identifier `^DD(411,0,"ID",.1)` with `@(…)` and `D Y^DIQ` | [DD.zwr:488724] (exported again at 488748) | Literal; evaluated once for the fixed context into the `dic` constant (R1 in SPEC.md) | Resolved |
| Pre-lookup node `X ^DD(411,.01,7.5)` | [DIC.m:36] [DD.zwr:488792] | Literal `S PRCRI=$P(^PRC(411,0),U,3)`; no output | Resolved (inside the `dic` stub) |
| Pointer-transform screen: `P^DICM0` builds `DIC("S")` from literal index and file names and XECUTEs it inside a nested lookup; `G @Y` dispatches on the literal field type | [DICM0.m:8-41] [DICM1.m:8-11] | Reproduced as the pointer-candidate rule (R6 in SPEC.md). The nested lookup keeps `M` and runs the pointed-to file's own transforms, which may write a `%DT` date echo [DICM1.m:13-23] | PO-09.b11 n/e; PO-09.b14 BLOCKED (O-03) |
| Name indirection `@(DIC_…)` | [DIC1.m:41] [DIC3.m:42, 63, 83, 109-125] | `DIC` fixed to `^PRC(442,` [PRCHPAT.m:79] or file 411 [PRCFSITE.m:18] | Resolved |
| `G @DIPGM`; `G @^("LOOK")` | [DIC.m:21, 38] [DICM.m:10] | Target nodes absent from the DD | Not reached |
| `RCR^DIWW` `\|` windows | [DIWP.m:40] | Not converted | PO-15.b2 n/e |
| `%XY^%RCR` indirection `@%Z`, `$Q(@%Z)` | [DIRCR.m:29] (the Kernel copy uses `$N` [ZTRCR.m:24-27]) | Literal `%X` and `%Y` [PRCHDP1.m:54] [PRCHDP2.m:48] | Resolved as a copy of scratch lines |
| `D EN^DIQ`; `D Y^DIQ` | [PRCHDP1.m:9] [PRCHUTL.m:76-77] | Not converted | PO-10.b2, PO-29.b3 n/e |
| `D @PRCHQ` | [PRCHRPT.m:43] | `EN0` path only | Not reached |
| `LOCK` | `L +^PRC(442.6,…)` [PRCHPAT.m:29] is in `NUM`, reached from `EN`, not `EN3` | None on the path | Not reached |
| Naked references `^(…)` | Throughout, e.g. [PRCHDP1.m:4] [PRCHDP2.m:11, 49-50] [PRCHDP1.m:61-62] | Each resolved to the full global reference it denotes at that point | Resolved |
| `R X:DTIME`, `W !`, `W ?n`, `$C(7)` | Throughout | Modelled in `src/mumps.rs`; not a hard feature | None |

## Unexercised and BLOCKED branches

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`; the 411 export is `Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr`.

### Branch granularity

- **Decisions:** an `IF`, with its `ELSE`, that governs a `DO` or extrinsic call, a `GOTO`, `QUIT` or `READ`, a loop or a block; a postcondition on a `DO`, `GOTO` or `QUIT`; a loop over stored entries whose body runs a `DO`, `GOTO`, `QUIT` or block, with the outcomes "at least one entry" and "no entry".
- **Not decisions** (they belong to the enclosing branch and are implemented with it): an `IF` or postcondition governing only `SET`, `WRITE` or `KILL` commands; a `$SELECT` arm; a loop whose body only formats the entries it visits; a fixed-count loop.
- **Branch IDs:** each outcome of a decision is a branch, numbered `PO-NN.bM` in source order within its contract. PO-09.b15 and PO-09.b16 were found after the plan fixed b1 to b14 and are appended so existing IDs stay stable. An exercised outcome may be described inside its enclosing branch's row; every outcome no case exercises has its own ID, because `UnsupportedBranch` must name it; decisions inside an n/e branch are covered by that branch's ID.
- **Coverage words:** "exercised by GC-0N (predicted)" or "not exercised, not implemented" (n/e), which returns `UnsupportedBranch` with the branch ID and writes nothing. "Unreachable" marks an n/e outcome that a fixed constant on this path rules out; it needs no code. BLOCKED marks an exercised branch whose source cannot be settled; it returns `UnsupportedBranch` too.

### Branch inventory

| Branch | Source | Outcome | Coverage |
|---|---|---|---|
| PO-01.b1 | [PRCHRPT.m:70-72] | `D ST`, then fall into `EN30` | exercised by GC-01 to GC-05 (predicted) |
| PO-02.b1 | [PRCHRPT.m:5-7] | `PRCF("X")="SP"`, `D ^PRCFSITE` | exercised by GC-01 to GC-05 (predicted) |
| PO-03.b1 | [PRCFSITE.m:5] | `D DUZ` leaves `%=0`: `G Q` | not exercised, not implemented |
| PO-03.b2 | [PRCFSITE.m:7] | `DT` undefined: `NOW^%DTC` | not exercised, not implemented |
| PO-03.b3 | [PRCFSITE.m:8-9, 13, 17-18] | Site file missing or empty, or holding one site so that no station prompt is written; the snapshot holds six [411 export:3] | not exercised, not implemented (the stub fixes the snapshot's site file) |
| PO-03.b4 | [PRCFSITE.m:15, 21, 29] | `PRC("FY")` undefined: `D ^%DT` (PO-41); fiscal year and quarter computed with no output. The defined outcome is unreachable: `PRC` is undefined when `EN3` starts | exercised by GC-01 to GC-05 (predicted) |
| PO-03.b5 | [PRCFSITE.m:16] | `AFU^PRCFSI1` (flag `A`) | not exercised, not implemented (unreachable: `PRCF("X")="SP"`) |
| PO-03.b6 | [PRCFSITE.m:18] | The lookup returns station 500: `PRC("SITE")=500` | exercised by GC-01 to GC-05 (predicted) |
| PO-03.b7 | [PRCFSITE.m:18] | The lookup returns no station (`+Y<0`): `G Q`. A non-empty station answer is reported as PO-05.b2 | not exercised, not implemented |
| PO-03.b8 | [PRCFSITE.m:19-25] | `AFU1`, fiscal-year and quarter prompts (flags `A`, `F`, `Q`) | not exercised, not implemented (unreachable: `PRCF("X")="SP"`) |
| PO-03.b9 | [PRCFSITE.m:27] | ISMS flag (`PRCFASYS` defined) | not exercised, not implemented |
| PO-04.b1 | [PRCFSITE.m:35, 37, 39] | `DUZ>0`; node 20 piece 2 present | exercised by GC-01 to GC-05 (predicted) |
| PO-04.b2 | [PRCFSITE.m:36] | "YOU ARE NOT IN THE 'NEW PERSON' FILE" | not exercised, not implemented |
| PO-04.b3 | [PRCFSITE.m:34, 38] | Node 20 piece 2 empty: `D A` writes `^VA(200,DUZ,20)` | not exercised, not implemented |
| PO-05.b1 | [DIC11.m:15-17, 29-32] [DIC.m:36, 48] [DIC5.m:21-31] [DIC3.m:20-31] [DIC2.m:8-32, 49-66] | RETURN accepts the default 500, selected by the exact `B` match and echoed `      CAMP MASTER` | exercised by GC-01 to GC-05 (predicted) |
| PO-05.b2 | [DIC11.m:17-33] | Any non-empty answer | not exercised, not implemented |
| PO-06.b1 | [PRCHRPT.m:72] | `Q:'$D(PRCHPO)`: the lookup ended without a selection, so `EN3` returns | exercised by GC-01 to GC-05 (predicted) |
| PO-06.b2 | [PRCHRPT.m:73-75] | Selection: the bell and ` >>> Status makes this record non-specifiable here.` when `X<10`, then the display, `K` and `G EN30` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-07.b1 | [PRCHRPT.m:9-15] | Default screen, prompt text, `D EN3^PRCHPAT`; conditional SETs for `PRCHPC`, `PRCHDELV` and MOP 8 | exercised by GC-01 to GC-05 (predicted) |
| PO-08.b1 | [PRCHPAT.m:79] | `PRC("SITE")` undefined: quit | not exercised, not implemented |
| PO-08.b2 | [PRCHPAT.m:80-83] | Index `C` lookup; `Q:+Y<0` with no selection | exercised by GC-01 to GC-05 (predicted) |
| PO-08.b3 | [PRCHPAT.m:83] | Selection sets `PRCHPO`, `DA`, and `X` to the supply status order (`0` becomes `""`) | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-09.b1 | [DIC11.m:18] | Timeout at the read | not exercised, not implemented (unreachable: the Rust READ never times out) |
| PO-09.b2 | [DIC11.m:19-22, 24-25] | A character outside `.ANP`, an over-long number, or over 250 characters | not exercised, not implemented |
| PO-09.b3 | [DIC11.m:23, 33] [DIC.m:45] | `^`, or any value beginning with `^`: `Y=-1`, no output | exercised by GC-01 to GC-05 (predicted) |
| PO-09.b4 | [DIC11.m:26] [DIC.m:42-44] | `?` help | not exercised, not implemented |
| PO-09.b5 | [DIC11.m:27] [DIC.m:47, 49] | A backtick value or a single space | not exercised, not implemented |
| PO-09.b6 | [DIC11.m:29-31] [DIC.m:45] | Empty input: `Y=-1`, no output | not exercised, not implemented |
| PO-09.b7 | [DIC.m:48] [DIC5.m:21-32] | Selection by IEN | not exercised, not implemented (unreachable: `IX^DIC` defines `DF` [DIC.m:84-85], so `BYIEN2` quits [DIC5.m:22]) |
| PO-09.b8 | [DIC3.m:15-32, 59-71] [DIC2.m:8-16, 49-66] [DICQ1.m:86-88] | After the whole search, exactly one candidate, an exact match on `C` that passes the screen; echoed and selected | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-09.b9 | [DIC3.m:34-71] | Exactly one candidate that is a partial match or comes from a later index | not exercised, not implemented |
| PO-09.b10 | [DIC3.m:72, 155] [DIC1.m:74-107] | Several candidates: the choose list | not exercised, not implemented |
| PO-09.b11 | [DICM.m:32-34] [DICM1.m:10-11] [DICM0.m:8-60] | A candidate through a pointer transform | not exercised, not implemented |
| PO-09.b12 | [DICM.m:33-34] [DICM1.m:13-40] | A candidate through a date or set-of-codes transform on a 442 index | not exercised, not implemented (unreachable: no index of 442 after `C` is on a date, set or variable-pointer field [DD.zwr:514219-514231]) |
| PO-09.b13 | [DICM.m:42-66] | A candidate through the uppercase, comma or long-input retry | not exercised, not implemented |
| PO-09.b14 | [DICM.m:71, 85-87] [DIC1.m:157-159] [DIC.m:63-65, 80-81, 25] | Search exhausted; output `<BEL> ??`, `\n`, `\n` and the re-asked prompt. Screen-rejected entries [DIC3.m:94, 124] and pointed-to entries without a qualifying parent count as none | exercised by GC-03, GC-04 (predicted); BLOCKED (O-03) |
| PO-09.b15 | [DIC0.m:22] [DIC1.m:41-49] | `^PRC(442,0)` absent, or its piece 2 not `442I`: no prompt, or other identifiers | not exercised, not implemented |
| PO-09.b16 | [DICM.m:71-81] | No `^PRC(442)` subscript collates after `"A["`: the whole file is scanned for an equal `.01`, writing `..` every 20 entries | not exercised, not implemented |
| PO-10.b1 | [PRCHDP1.m:8] | Node 0 empty: quit | not exercised, not implemented |
| PO-10.b2 | [PRCHDP1.m:9] | Node 1 empty with comments present: `EN^DIQ` | not exercised, not implemented |
| PO-10.b3 | [PRCHDP1.m:10] | Node 1 empty: quit | not exercised, not implemented |
| PO-10.b4 | [PRCHDP1.m:10-44] | The page-1 header (ship-to, `^%ZIS`, `@IOF`, labels, vendor and ship-to rows, underscores) | exercised by GC-01, GC-02, GC-05 (predicted); BLOCKED (O-01) at line 12 |
| PO-10.b5 | [PRCHDP1.m:16-17] | Node 11 present: the `LAST PARTIAL RECD.` loop | not exercised, not implemented |
| PO-10.b6 | [PRCHDP1.m:32-34] | EDI vendor (440 field .2 `Y`) and 442 field 116 not `n`: the EDI block | not exercised, not implemented |
| PO-10.b7 | [PRCHDP1.m:45-52] | Field 65 equal to `RMPR`: the prosthetics block | not exercised, not implemented |
| PO-11.b1 | [PRCHFPNT.m:44-45, 49, 52-53] | Facility type from site piece 7 through `^PRC(411.2)` piece 2 | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-11.b2 | [PRCHFPNT.m:46-47] | Substation, node 23 piece 7 | not exercised, not implemented |
| PO-12.b1 | [ZIS.m:28-37] [ZIS1.m:103-113] | `IOP="HOME"` device set-up | exercised by GC-01, GC-02, GC-05 (predicted); BLOCKED (O-01) |
| PO-13.b1 | [PRCHDP1.m:53-57] | FOB, proposal wrap at width 16, authority loop, cost center, type, buyer or agent label | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-14.b1 | [PRCUTL.m:33-35] | `D ^DIWP` with `DA` saved | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-15.b1 | [DIWP.m:28-77] | Plain text, `DIWF` empty, no `\|` and no `TAB` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-15.b2 | [DIWP.m:40] | `\|` window evaluation (`RCR^DIWW`) | not exercised, not implemented |
| PO-15.b3 | [DIWP.m:30-38, 47, 55-57] | Other `DIWF` letters and `TAB` | not exercised, not implemented |
| PO-16.b1 | [DIRCR.m:24-30] | Copy of the `^UTILITY($J,"W")` subtree to `^TMP($J,"W")` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-17.b1 | [PRCHDP1.m:71-72] | Order type from the source code through `$SELECT` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-18.b1 | [PRCHDP1.m:58-60] | Delivery date; buyer or agent name; no node 5 | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-18.b2 | [PRCHDP1.m:60] | Node 5 present: discount terms | not exercised, not implemented |
| PO-19.b1 | [PRCHDP1.m:69-70] | `M/D/YYYY` when `Y` is non-zero (a conditional WRITE) | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-20.b1 | [PRCHDP1.m:61-62] | A contract number in the item index `AC`: its count and number on the row | not exercised, not implemented |
| PO-20.b2 | [PRCHDP1.m:61, 63-64] | No further contract number, in each of the three rows; then dashes and `G ^PRCHDP2` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-21.b1 | [PRCHDP1.m:65] | `APP: ` from fields 1.4 and 1 | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-22.b1 | [PRCHDP1.m:66] | `DATE: ` from field .1 through `DT` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-23.b1 | [PRCHDP1.m:68] | Field .08 not `Y`: quit, nothing written | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-23.b2 | [PRCHDP1.m:68] | Field .08 equal to `Y`: `ESTIMATED` | not exercised, not implemented |
| PO-24.b1 | [PRCHDP1.m:67] | `TOTAL: ` as `$J(field 91,8,2)` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-25.b1 | [PRCHDP2.m:10] | An answer containing `^` at `ENTER '^' TO HALT`: `G ASK2` | not exercised, not implemented |
| PO-25.b2 | [PRCHDP2.m:10-11] | Otherwise: `D HDR`, then `ITEM` once per item | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-25.b3 | [PRCHDP2.m:11] | No line item | not exercised, not implemented |
| PO-25.b4 | [PRCHDP2.m:11] | `PRCHDQ` set after an item, reachable only through PO-30.b2: `G ASK2` | not exercised, not implemented |
| PO-25.b5 | [PRCHDP2.m:12, 67] | Discount entries in node 3: `DIS^PRCHDP2` | not exercised, not implemented |
| PO-25.b6 | [PRCHDP2.m:13-14] | The estimated-shipping line when field 13 > 0 (a conditional WRITE); no node 15: `G COM` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-25.b7 | [PRCHDP2.m:14-18] | Certifications in node 15 | not exercised, not implemented |
| PO-26.b1 | [PRCHDP2.m:42] | `W:$Y>0 @IOF,…` column header, then dashes | exercised by GC-01, GC-02, GC-05 (predicted); BLOCKED (O-01) |
| PO-27.b1 | [PRCHDP2.m:44-53, 55-66] | A regular item and its detail lines | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-27.b2 | [PRCHDP2.m:47] | An item with no description line | not exercised, not implemented |
| PO-27.b3 | [PRCHDP2.m:54] | Continuation description lines (`PRCHDCNT>1`) | not exercised, not implemented |
| PO-27.b4 | [PRCHDP2.m:55] | `PRCHDQ` set, reachable only through PO-30.b2: quit | not exercised, not implemented |
| PO-27.b5 | [PRCHDP2.m:57] | Partial receipts in item node 3 | not exercised, not implemented |
| PO-28.b1 | [PRCHDP2.m:96-102] | Every item's description lines scanned; none contains `\| IN ` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-28.b2 | [PRCHDP2.m:98-100] | One does: `Q:PURPIPE` | not exercised, not implemented |
| PO-28.b3 | [PRCHDP2.m:99] | An item with no description line | not exercised, not implemented |
| PO-29.b1 | [PRCHUTL.m:73] | Item node 2 absent | not exercised, not implemented |
| PO-29.b2 | [PRCHUTL.m:73-74, 80] | Node 2 present, EDI status codes 1 and 2 empty: quit | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-29.b3 | [PRCHUTL.m:75-78] | EDI status lines through `Y^DIQ` | not exercised, not implemented |
| PO-30.b1 | [PRCHDP2.m:31] | `$Y+5'>IOSL`: no prompt | exercised by GC-01, GC-02, GC-05 (predicted); BLOCKED (O-01, `IOSL`) |
| PO-30.b2 | [PRCHDP2.m:31] | Paging prompt and `HDR` | not exercised, not implemented |
| PO-31.b1 | [PRCHDP2.m:19] | No node 4: `G PT` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-31.b2 | [PRCHDP2.m:19-21] | Comments | not exercised, not implemented |
| PO-32.b1 | [PRCHDP2.m:22-25] | No node 13; `D AMENDS`; no node 6; `K ^TMP($J,"PRCHDP6")` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-32.b2 | [PRCHDP2.m:22] | Transaction numbers | not exercised, not implemented |
| PO-32.b3 | [PRCHDP2.m:24] | Amendment lines | not exercised, not implemented |
| PO-33.b1 | [PRCHDP6.m:127] | No node 6: quit | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-33.b2 | [PRCHDP6.m:128-152] | Amendment history | not exercised, not implemented |
| PO-34.b1 | [PRCHDP2.m:26] | `PRCHDQ` clear: `EN^PRCHDP4`; no node 11: `G ASK1` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-34.b2 | [PRCHDP2.m:26-30] | Receiving-report review | not exercised, not implemented |
| PO-34.b3 | [PRCHDP2.m:26] | `PRCHDQ` set: `EN^PRCHDP4` skipped | not exercised, not implemented |
| PO-35.b1 | [PRCHDP4.m:4-5] | Sets `PRCHONL`, `PRCHQUIT` and falls into `EN2` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-36.b1 | [PRCHDP4.m:8] | `D0` undefined: quit | not exercised, not implemented (unreachable: `D0` is set at [PRCHRPT.m:74]) |
| PO-36.b2 | [PRCHDP4.m:8] | Empty PO number: `G EXDELV` | not exercised, not implemented |
| PO-36.b3 | [PRCHDP4.m:9-11, 16] | Delivery-schedule entries in `^PRC(442.8,"B",PO)`: `BLD`, then print | not exercised, not implemented |
| PO-36.b4 | [PRCHDP4.m:8-10] | No `B` entry, so no `^TMP($J,"MDEL")`: `G EXDELV` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-37.b1 | [PRCHDP4.m:13-14] | `W !`; kills | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-38.b1 | [PRCHDP2.m:33-40] | `PRCHAMNT=2`: the amendment warning | not exercised, not implemented |
| PO-38.b2 | [PRCHDP2.m:41] | `PRCHDQ` set: `G Q` | not exercised, not implemented |
| PO-38.b3 | [PRCHDP2.m:41] | `END OF DISPLAY` read, then `G Q` | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-39.b1 | [PRCHDP2.m:94-95] | Kills locals and scratch; no output | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-40.b1 | [PRCHRPT.m:106-108] | Kills locals and returns | exercised by GC-01, GC-02, GC-05 (predicted) |
| PO-41.b1 | [DIDT.m:11-24] | `%DT` has no `A` or `E` flag: no prompt, no output; `Y` gets today's date, which no output uses | exercised by GC-01 to GC-05 (predicted) |

### Tags without a contract

Reachable only through an n/e branch, so given no contract and no Rust function. IFCAP: `PT1`, `ADJCHK`, `AMD`, `DIS` and `DT^PRCHDP2`; `^PRCHDP3`; `^PRCFYN`; `START^PRCHDP5`; `BLD`, `PRT1`, `PRT2`, `H` and `PAUSE^PRCHDP4`; `A` and `HELP1^PRCFSITE`; `AFU` and `AFU1^PRCFSI1`; `$$ISMSFLAG^PRCPUX2`. Kernel and FileMan: `NOW^%DTC`; `DD^%DT`; `EN^DIQ`; `Y^DIQ`; `DT^DIQ`; `RCR^DIWW`; FileMan help, list and LAYGO code (`DSPHLP^DICQ`, `Y^DIC1`, `L^DICM`).

### Exhausted searches versus unsupported outcomes

In PO-09.b11, b12 and b13 an alternative route finds a candidate. When no route does, the search continues and ends in PO-09.b14, never in the unsupported ID of a route that found nothing. Routes that find nothing: a numeric value, which is never tried as an IEN [DIC5.m:22]; a lowercase value whose uppercase form matches nothing [DICM.m:44-47]; candidates that all fail the screen [DIC3.m:124]; pointed-to entries without a qualifying parent [DICM0.m:25-37], which the nested lookup rejects without output of its own [DICM.m:107]. While PO-09.b14 is BLOCKED by O-03, every input that reaches it returns `UnsupportedBranch("PO-09.b14")` alike, and no identifier is treated specially.

### Fixture profile

The predicted coverage assumes the fixture profile stated in `tests/golden/ifcap/README.md` (and in SPEC.md, section "Source corrections and interface departures"). A capture whose records leave that profile reaches an n/e branch; that case is FAIL and its contract reopens.

## Scope statement

- **Covered:** one read-only VistA workflow, OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the repository at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`, with the fixed context `DUZ` `.5` and station 500.
- **Not claimed:** no VA-wide equivalence; equivalence with any other image, snapshot, site, user, device or option; or any behaviour the five cases do not exercise. The five cases (GC-01 to GC-05) show only their exercised paths.
- **Read-only:** the port has no write path. `PRCHQ1` (OPTION 10295, New RFQ, `EN2^PRCHQ2`) was named in the request but creates RFQs, so `PRCH DISPLAY PO` was substituted. The original's own non-business writes (`^DISV`, word-wrap and delivery scratch under `^UTILITY` and `^TMP`) are documented and not reproduced.
- **Out of scope:** every write path (creating, editing, receiving, amending or cancelling orders); every other IFCAP function and option; VistA GUI clients, terminal devices and terminal emulators; a Java port; every other VistA package; the earlier GMV vitals slice; and the request's "must not be built" list: a write-guard framework, a CLI or exit-code scheme, git or provenance modules, deterministic-report machinery, Windows or macOS branches, an async runtime, any network service other than the loopback bridge, FFI or C bindings, procedural macros, a second crate or workspace, more than two integration test files, a front-end framework or build tooling, authentication or TLS.
- **Unexercised branches:** every branch marked "not exercised, not implemented", and every BLOCKED branch, returns `UnsupportedBranch` with its branch ID and writes no further output.

<!-- COMPARISON END -->

## Execution evidence

Written by the run from the files saved under `test-results/20261108T100819Z/` (run-id `20261108T100819Z`), the evidence run on the delivered tree. Every file there was produced by shell redirection of the command it names, from the working directory `rust/ifcap-po-inquiry/`, with `RUSTUP_AUTO_INSTALL=0` exported before every `rustc` and `cargo` command. Two items ran only in the earlier run `test-results/20261108T035854Z/` and are cited there: step 00, and the `# A7 (SPEC.md)` block. A claim below without a cited file is not made.

**Vocabulary.** Outcomes of checks and cases are BLOCKED, NOT COMPARED, FAIL, PASS, NOT RUN and PENDING (AAP 0.8.3). `ok`, `passed`, `failed` and `ignored` are cargo's words, copied from saved output; a cargo `ok` is not a case PASS. Contract and open-item states (OPEN, SOURCE-RESOLVED, closed) are `SPEC.md`'s evidence vocabulary.

### Environment

From `test-results/20261108T100819Z/env.txt`, which also records each command line as it ran:

- **Working directory:** `/tmp/blitzy/blitzy-VistA-VEHU-M/blitzy-d3cd5e06-5ae1-42c7-8b11-7bb911fc681f_d7b31c/rust/ifcap-po-inquiry`.
- **Times (UTC):** from `20261108T100819Z` (`utc_start`, the run-id) to `2026-11-08T10:08:22Z` (`utc_end`, written last).
- **OS and architecture:** `Linux 6.12.94+ x86_64` (`uname -srm`).
- **Revision:** `HEAD` is `b55a56c4b186a23856b99ce8e3062b2d23397362`, not the baseline `dd3fd0f3124c8178657d9fff4173c324da30f7ae`: it is the baseline plus this crate's commits. `git merge-base --is-ancestor dd3fd0f3124c8178657d9fff4173c324da30f7ae HEAD` exited 0, and the supplementary A7 diff below shows the source unchanged since the baseline.
- **Working tree:** dirty (`tree: dirty`). `git -C ../.. status --porcelain` printed the two front-end files staged on top of `HEAD`, which are the front-end changes this commit delivers (`M  rust/ifcap-po-inquiry/frontend/app.js`, `M  rust/ifcap-po-inquiry/frontend/index.html`), Cargo's build output (`?? rust/ifcap-po-inquiry/target/`) and the run's own untracked evidence directory (`?? rust/ifcap-po-inquiry/test-results/20261108T100819Z/`). Every test and search below ran on that working tree.
- **Image digest:** `image_digest: NOT AVAILABLE TO THE RUN`; no `meta.txt` exists.
- **Toolchain configuration:** `RUSTUP_AUTO_INSTALL=0`; `TMPDIR=/tmp/ifcap-clone-64`.
- **Fixtures:** the eleven repository copies `411.2.zwr`, `411.zwr`, `420.5.zwr`, `420.8.zwr`, `440.zwr`, `442.3.zwr`, `442.4.zwr`, `442.5.zwr`, `442.8.zwr`, `49.zwr` and `5.zwr`, plus `MANIFEST.txt`, each fixture with `snapshot_id` `git:dd3fd0f3124c8178657d9fff4173c324da30f7ae` (`cut -f1,3 tests/fixtures/ifcap/MANIFEST.txt`, exit 0). `cases.zwr` is absent.
- **Captures:** `capture directories: none` (`ls -d tests/golden/ifcap/GC-0*/` exited 2).
- **`REPORT.md` around step 06:** before step 06, `grep -c` found the two comparison markers (exit 0), and `sha256sum REPORT.md` gave `5cc05404a894a0de36ce278399dd4d48523a9be485acc0157154f0d88a6219d1` both before and after step 06, so `comparison_report` left the file byte for byte as it was.

### Commands

| Step | Command | Exit status | Result, copied from the saved output |
|---|---|---|---|
| 00 | `cargo generate-lockfile --offline` | 0 (`test-results/20261108T035854Z/00-generate-lockfile.status`) | Ran only in the earlier run `20261108T035854Z`, while `Cargo.lock` was absent; `00-generate-lockfile.stdout` and `.stderr` there are empty. Not run under `20261108T100819Z`, because `Cargo.lock` existed, so that run has no `00-*` files |
| 01 | `rustc -Vv` | 0 (`test-results/20261108T100819Z/01-rustc-Vv.status`) | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `commit-hash: 58cf8f9c35b355199727627d07ae34a4e2846d67`, `host: x86_64-unknown-linux-gnu`, `release: 1.99.1`, `LLVM version: 23.1.1` (`01-rustc-Vv.stdout`). Matches the pin `channel = "1.99.1"` |
| 02 | `cargo -V` | 0 (`test-results/20261108T100819Z/02-cargo-V.status`) | `cargo 1.99.1 (5f94df478 2026-08-27)` (`02-cargo-V.stdout`) |
| 03 | `cargo build --offline --locked` | 0 (`test-results/20261108T100819Z/03-build.status`) | ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.02s`` and no warning (`03-build.stderr`); `03-build.stdout` empty. There is no `Compiling` line: `target/` was already up to date with these sources |
| 04 | `cargo test --offline --locked --lib -- --nocapture` | 0 (`test-results/20261108T100819Z/04-test-lib.status`) | `test result: ok. 93 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s` (`04-test-lib.stdout`): `bridge::tests` 34, `inquiry::tests` 27, `mumps::tests` 11, `store::tests` 21 |
| 05 | `cargo test --offline --locked --test bridge -- --nocapture --test-threads=1` | 0 (`test-results/20261108T100819Z/05-test-bridge.status`) | `test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s` (`05-test-bridge.stdout`) |
| 06 | `cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1` | 0 (`test-results/20261108T100819Z/06-test-equivalence.status`) | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (`06-test-equivalence.stdout`): `comparison_report`, `console_summary_shows_causes_never_details`, `context_cause_names_only_stub_keys`, `gc_01_standard_order`, `gc_02_multiple_lines_and_decimals`, `gc_03_missing_order`, `gc_04_invalid_input`, `gc_05_cancelled_order`, `manifest_rows_keep_order_and_reject_repeats` and `returned_txt_framing`, each `ok`. Each of the five cases printed NOT COMPARED (A3 below). Exit 0 never means equivalence is accepted |

Named unit tests in step 04, each reported `ok` in `test-results/20261108T100819Z/04-test-lib.stdout`:

- **`inquiry::tests` (27):** `absent_identifiers_follow_no_match_path`, `candidates_all_rejected_by_screen_follow_no_match_path`, `date_prints_month_day_year_unpadded`, `estimated_flag_decides_est_branch`, `found_order_stops_at_device_gate`, `help_at_order_prompt_is_unsupported_branch`, `input_exhaustion_returns_partial_transcript`, `item_line_runs_to_paging_gate`, `leftover_input_ignored`, `lowercase_value_whose_retry_fails_follows_no_match_path`, `lowercase_value_whose_retry_matches_is_unsupported_branch`, `missing_file_header_is_unsupported_branch`, `non_empty_station_answer_is_unsupported_branch`, `numeric_value_naming_no_ien_follows_no_match_path`, `order_filed_under_two_indexes_is_one_candidate`, `order_prompt_read_checks`, `page_one_block_runs_to_column_header_gate`, `referenced_vendor_whose_orders_fail_screen_is_not_a_candidate`, `referenced_vendor_with_qualifying_order_is_unsupported_branch`, `several_candidates_are_unsupported_branch`, `single_partial_match_is_unsupported_branch`, `store_unchanged_after_session`, `unique_order_match_writes_identifier_echo`, `unit_cost_format_follows_stored_decimals`, `unreferenced_vendor_with_matching_name_is_not_a_candidate`, `word_wrap_reproduces_plain_text_rules`, `word_wrap_unconverted_forms_are_unsupported_branches`.
- **`mumps::tests` (11):** `canonical_number_forms`, `collation_numbers_before_strings`, `collation_strings_byte_order`, `int_div_and_modulo_on_fileman_dates`, `justify_two_and_three_argument`, `num_leading_prefix_coercion`, `order_over_mixed_subscripts`, `order_skips_null_subscripts`, `piece_extract_get_data`, `read_returns_none_when_exhausted`, `write_newline_and_tab_track_x_y`.
- **`store::tests` (21):** `blank_lines_ignored`, `crlf_line_endings_accepted`, `data_and_get_report_value_and_descendants`, `dollar_c_value_rejected`, `doubled_quotes_unescaped`, `empty_input_gives_empty_store`, `fixture_parse_error_names_file_and_line`, `format_ref_quotes_strings_and_round_trips`, `header_lines_ignored`, `load_fixtures_order_puts_cases_last`, `load_fixtures_without_cases_is_not_an_error`, `malformed_line_forms_rejected`, `malformed_line_reports_line_number`, `non_canonical_unquoted_subscript_rejected`, `null_subscript_loaded_and_invisible_to_order`, `order_collates_numbers_before_strings`, `quoted_canonical_number_stored_as_number`, `repeated_key_replaces`, `sub_from_text_classifies_by_canonical_text`, `unquoted_numeric_value_kept_as_text`, `unreadable_fixture_file_named`.
- **`bridge::tests` (34):** listed under A6.

The `inquiry::tests` assertions follow `SPEC.md` section "Resolution decisions binding src/inquiry.rs": while O-03 is open the five no-match tests assert `UnsupportedBranch("PO-09.b14")`, `referenced_vendor_with_qualifying_order_is_unsupported_branch` asserts `UnsupportedBranch("PO-09.b11")`, and `found_order_stops_at_device_gate` asserts `UnsupportedBranch("PO-12.b1")`.

### A3: evidence commands and case outcomes

The six commands 01 to 06 ran in order, each with its `.stdout`, `.stderr` and `.status` saved, and each exited 0 (table above). Step 06 printed, in `test-results/20261108T100819Z/06-test-equivalence.stdout`:

- `GC-01 gc_01_standard_order: NOT COMPARED`
- `GC-02 gc_02_multiple_lines_and_decimals: NOT COMPARED`
- `GC-03 gc_03_missing_order: NOT COMPARED`
- `GC-04 gc_04_invalid_input: NOT COMPARED`
- `GC-05 gc_05_cancelled_order: NOT COMPARED`

Each line continues with `missing:` and the same eight files for its case: `tests/golden/ifcap/GC-0N/input.txt`, `transcript.txt`, `returned.txt`, `db-before.zwr`, `db-after.zwr`, `meta.txt` and `normalisation.txt`, and `tests/fixtures/ifcap/cases.zwr`. No case is PASS and none is FAIL. `comparison_report` reported `ok` and wrote the comparison section, whose totals read `PASS 0, FAIL 0, NOT COMPARED 5`. The run-completion check A3 is met. A3 stays open as an acceptance item, because all five comparisons remain NOT COMPARED until the captures arrive (B1).

The run created `REPORT.md` with the empty marker pair under `## Comparison` before step 06 and never wrote between the markers. Everything there is `comparison_report` output, including the per-case hash fields (NOT EVALUATED while readiness fails) and the four `SPEC.md` sections it copies verbatim. That is harness output, not run evidence. The run's hash evidence is under "Hashes" below.

### A6: bridge tests

- **`tests/bridge.rs` (step 05; 24 passed, 0 failed, 0 ignored):** 23 synthetic routing tests reported `ok`: `synthetic_400_element_with_line_break`, `synthetic_400_inputs_missing_or_not_array`, `synthetic_400_malformed_json`, `synthetic_400_non_string_element`, `synthetic_404_non_post_on_api`, `synthetic_404_unknown_backend`, `synthetic_404_unknown_path`, `synthetic_422_unsupported_branch_end_to_end`, `synthetic_502_capture_file_missing`, `synthetic_502_capture_file_non_utf8`, `synthetic_502_capture_file_unreadable`, `synthetic_502_context_mismatch`, `synthetic_502_malformed_meta`, `synthetic_502_malformed_normalisation`, `synthetic_502_not_recorded`, `synthetic_502_not_recorded_lists_unreadable_input`, `synthetic_compare_match_false`, `synthetic_compare_match_true`, `synthetic_json_escaping_round_trip`, `synthetic_mumps_replay_200`, `synthetic_rust_session_200`, `synthetic_shutdown_joins_within_bound`, `synthetic_static_paths_exact_match`. Synthetic fixtures test routing only and are never parity evidence.
- **`gc_02_end_to_end_compare` (not synthetic):** it printed `gc_02_end_to_end_compare (not synthetic): NOT COMPARED, missing:`, then the seven `GC-02/` capture files and `tests/fixtures/ifcap/cases.zwr`, and returned. Cargo reports it `ok`; its outcome is NOT COMPARED, not PASS (`test-results/20261108T100819Z/05-test-bridge.stdout`).
- **Bridge unit tests in `src/bridge.rs` (step 04; 34 of the 93, all `ok`):** `accept_errors_retried_or_returned`, `compare_match_needs_transcript_and_returned`, `compare_match_null_when_mumps_returned_unavailable`, `context_duplicates_found_with_a_set`, `decide_live_result_non_utf8`, `decide_live_result_non_zero_exit`, `decide_live_result_output_too_large`, `decide_live_result_precedence_timeout_over_output_limit`, `decide_live_result_spawn`, `decide_live_result_success`, `decide_live_result_timeout`, `exec_result_rejects_failed_panicked_and_unfinished_workers`, `inquiry_error_to_response_corrupt_store_500`, `inquiry_error_to_response_unsupported_branch_422`, `json_reads_escapes_and_surrogate_pairs`, `json_writes_escapes`, `mumps_side_without_captures`, `normalisation_rules_and_malformed_lines`, `parse_meta_reads_seven_keys_and_rejects_bad_lines`, `read_bounded_reports_failure_retry_limit_and_cancel`, `reap_records_kill_and_wait_failures`, `refused_connection_answered_503_then_closed`, `request_head_parsing`, `route_errors_repeat_no_request_data`, `run_command_joins_workers_held_open_by_a_descendant`, `run_command_kills_on_output_limit`, `run_command_kills_on_timeout`, `run_command_partial_worker_start_kills_reaps_and_joins`, `run_command_writes_stdin_and_reports_spawn_and_exit`, `session_request_validation`, `split_input_lines_trailing_newline`, `validate_context_messages`, `watch_records_a_child_it_cannot_wait_on`, `write_input_reports_refusal_failure_retry_and_cancel` (`test-results/20261108T100819Z/04-test-lib.stdout`).
- **NOT RUN:** none. The tests that need an external program ran: `run_command_kills_on_timeout`, `run_command_joins_workers_held_open_by_a_descendant` and `run_command_partial_worker_start_kills_reaps_and_joins` (`sleep`), `run_command_kills_on_output_limit` (`yes`), and `run_command_writes_stdin_and_reports_spawn_and_exit` (`cat`, `false`). No saved stdout of steps 04 to 06 contains `NOT RUN`.
- **Statuses not reachable end to end:**
  - HTTP 500: every `CorruptStore` site on the Rust path lies after the BLOCKED PO-12.b1. `inquiry_error_to_response_corrupt_store_500` covers it.
  - `match: null`: it arises only in live mode. `compare_match_null_when_mumps_returned_unavailable` covers it.

### A4: safety searches

From `test-results/20261108T100819Z/search-a4.txt`, scope `src/`:

| Command | Output | Exit |
|---|---|---|
| `grep -rn 'unsafe' src/` | `src/lib.rs:9:#![forbid(unsafe_code)]`, the one expected hit | 0 |
| `grep -rnE '\.unwrap\(\|\.expect\(\|panic!\|unreachable!\|todo!\|unimplemented!' src/` | none | 1 |
| `grep -rnE '[A-Za-z0-9_)]\[' src/` | none, so no index expression needs justifying | 1 |

Result: A4 holds.

### A5: golden-content access

From `test-results/20261108T100819Z/search-a5.txt`: `grep -rnE 'golden|GC-0|include_str!|include_bytes!|input\.txt|transcript\.txt|returned\.txt|normalisation\.txt|meta\.txt' src/` exited 0 with 34 hits, 29 in the MUMPS-adapter code of `src/bridge.rs` and 5 in `src/bin/bridge.rs`:

| Hit lines | Item holding them |
|---|---|
| `src/bin/bridge.rs:9` | Module doc of the binary: describes `GOLDEN_DIR` |
| `src/bin/bridge.rs:23, 27, 33, 35` | `main`: builds the default `tests/golden/ifcap/` path and passes `golden_dir` into `MumpsMode`; it reads nothing |
| `src/bridge.rs:71, 72, 77, 78, 79, 81, 84` | `enum MumpsMode`: doc comments and the `golden_dir` fields of `Replay` and `Live` |
| `src/bridge.rs:934, 937, 938, 939, 940, 941` | The capture constants `CASE_DIRS`, `INPUT_FILE`, `TRANSCRIPT_FILE`, `RETURNED_FILE`, `NORMALISATION_FILE` and `META_FILE`, used by the replay and live adapters |
| `src/bridge.rs:1038, 1041, 1042` | `run_mumps`: the MUMPS-side dispatch that `mumps_session` and `compare_session` call |
| `src/bridge.rs:1067, 1070, 1073` | `replay`: the replay adapter |
| `src/bridge.rs:1124, 1125` | `live`: the live adapter |
| `src/bridge.rs:1147, 1149, 1158` | `live_meta`: reads `meta.txt` in live mode |
| `src/bridge.rs:3093` | `#[cfg(test)]` unit test `route_errors_repeat_no_request_data`, which builds `MumpsMode::Replay` with an empty `golden_dir`; every request it sends is answered 404 before a backend runs, so no capture is read |
| `src/bridge.rs:3130, 3132, 3141, 3144` | `#[cfg(test)]` unit test `mumps_side_without_captures`, which runs `run_mumps` against a temporary directory that does not exist |

There are no hits in `src/inquiry.rs`, `src/store.rs`, `src/mumps.rs` or `src/lib.rs`. Result: A5 holds, and `inquire` and `Store` never read golden content.

### Front-end no-validation search

`test-results/20261108T100819Z/search-frontend.txt`: `grep -rnE 'RegExp|\.test\(|\.match\(|\.search\(|\.replace\(|\.trim\(|toUpperCase|toLowerCase|\.length *[<>=!]|isNaN|parseInt|parseFloat|Number\(|pattern=|required|maxlength|minlength|type="number"' frontend/` printed nothing and exited 1. The search has zero hits. The browser checks are B3 and B4 (NOT RUN).

### A7: source unchanged

Two saved blocks: `# A7 (SPEC.md)` in `test-results/20261108T035854Z/source-unchanged.txt`, which `SPEC.md` section "Source-unchanged check" quotes, and `# A7 (end of run)` in `test-results/20261108T100819Z/source-unchanged.txt`, which decides A7 for this run.

| Block | `HEAD` | `git -C ../.. diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION` | `git -C ../.. status --porcelain -- . ':(exclude)rust/ifcap-po-inquiry'` | Supplementary `git -C ../.. diff --quiet dd3fd0f3124c8178657d9fff4173c324da30f7ae HEAD -- …` |
|---|---|---|---|---|
| `# A7 (SPEC.md)`, `2026-11-08T05:07:58Z` (`test-results/20261108T035854Z/source-unchanged.txt`) | `8719e11c7415616878d50994ff1d8e898a36876b` | exit 0 | no output, exit 0 | exit 0 |
| `# A7 (end of run)`, `2026-11-08T10:08:22Z` (`test-results/20261108T100819Z/source-unchanged.txt`) | `b55a56c4b186a23856b99ce8e3062b2d23397362` | exit 0 | no output, exit 0 | exit 0 |

Result: A7 holds at the end of the run. No file outside `rust/ifcap-po-inquiry/` changed, and the MUMPS routines and ZWR exports are byte for byte the baseline's.

### Hashes

- **`test-results/20261108T100819Z/sha256-fixtures.txt`** (`sha256sum tests/fixtures/ifcap/*`, exit 0): twelve hashes, for the eleven fixtures and `MANIFEST.txt` (`975228315e47a12ed9a01ef9e57f582dc41bd9b506a3544fbeddd22db7823639`). Each fixture hash equals the fourth column of its `MANIFEST.txt` line. The manifest has no `cases.zwr` line, and no `cases.zwr` file exists.
- **`test-results/20261108T100819Z/sha256-captures.txt`** (`sha256sum tests/golden/ifcap/GC-0*/*`, exit 1): `sha256sum: 'tests/golden/ifcap/GC-0*/*': No such file or directory`. No capture file exists.

### All-target check and lint (run-id `20261108T100819Z`)

Run in the same evidence run, after the searches and before the end-of-run A7 block, saved under `test-results/20261108T100819Z/` by shell redirection from `rust/ifcap-po-inquiry/`, with `RUSTUP_AUTO_INSTALL=0` exported first. The toolchain is the one steps 01 and 02 confirmed (`release: 1.99.1`, `cargo 1.99.1 (5f94df478 2026-08-27)`). It supersedes the earlier all-target session `test-results/20261108T083835Z/`, which checked `HEAD` `879100e9b886dbbd03617195c0dadd48ad4933ad`.

Each command ran with its own `CARGO_TARGET_DIR=$(mktemp -d)` under `/tmp/ifcap-clone-64/` (`tmp.Zd9h3lOnLl` for check, `tmp.j1LqW68eQ5` for clippy), named in `env.txt` and removed afterwards (`rm -rf`, exit 0). From the targets `Cargo.toml` and `tests/` define, `--all-targets` selects the library and its unit tests, the binary `bridge` and its unit tests, and the integration tests `bridge` and `equivalence`. Cargo prints one `Checking` line per package, not per target. Clippy ran at its default lint levels, with no warnings-as-errors setting.

| Command | Exit status | Result, copied from the saved output |
|---|---|---|
| `CARGO_TARGET_DIR=$(mktemp -d) cargo check --offline --locked --all-targets` | 0 (`test-results/20261108T100819Z/check-all-targets.status`) | ``Checking ifcap-po-inquiry v0.1.0 (…)``, ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.62s``; 0 `warning` and 0 `error` lines (`check-all-targets.stderr`); `check-all-targets.stdout` empty |
| `CARGO_TARGET_DIR=$(mktemp -d) cargo clippy --offline --locked --all-targets` | 0 (`test-results/20261108T100819Z/clippy-all-targets.status`) | ``Checking ifcap-po-inquiry v0.1.0 (…)``, ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.66s``; 0 `warning` and 0 `error` lines (`clippy-all-targets.stderr`); `clippy-all-targets.stdout` empty |

## Review

Written by the run. Citations: `PRC*.m` are `Packages/IFCAP/Routines/`; `DI*.m` are `Packages/VA FileMan/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`; `19+OPTION.zwr` is `Packages/Kernel/Globals/`; the `442+…` and `442.5+…` exports are `Packages/IFCAP/Globals/`. Section names in quotes are `SPEC.md` sections.

### Source verification and open items

The Source verification finding is OPEN ("Source verification"). The workflow has not executed in the matching VEHU environment, the HOME device context (O-01) is unresolved, and the image digest (O-05) is unrecorded. The capture mapping is predicted only. Commit, menu option, entry point, source location and data dependencies are SOURCE-RESOLVED there.

| ID | Item | Status as `SPEC.md` records it ("Open items", "Resolution decisions binding src/inquiry.rs") | This run |
|---|---|---|---|
| O-01 | HOME device context: the bytes `^%ZIS` writes, the `IOF` value and the bytes `W @IOF` emits, `IOSL`, and `$Y` after a form feed | OPEN. PO-12.b1, PO-10.b4, PO-26.b1 and PO-30.b1 are BLOCKED, so GC-01, GC-02 and GC-05 are BLOCKED. `STUB_CONTEXT` holds `IOF` and `IOSL` as `None` | Unchanged. No capture exists to confirm the form-feed bytes |
| O-02 | FileMan echo bytes: station default, unique order match, no-match sequence | Closed by R1, R2 and R3, so PO-03, PO-05.b1 and PO-09.b8 are SOURCE-RESOLVED. R1 gives `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// ` and, for RETURN, `      CAMP MASTER` (six spaces, no newline). R2 gives `  <.01>  <Z.01>  <Z.02>  \n` plus 13 spaces and `FCP: <fcp>     $ <total>`. R3 gives `<BEL> ??\n\nP.O./REQ.NO.: `. A capture that disagrees reopens O-02 | Source-traced, not runtime-verified: no capture |
| O-03 | Transforms for the GC-03 and GC-04 values | Narrowed, still OPEN (R4). Settled: no date, set or variable-pointer index of file 442 after `C`, so PO-09.b12 is unreachable; a nested pointer miss writes nothing; no soundex echo. Open, two questions: the nested pointer lookups run the pointed-to files' own transforms, where a `%DT` conversion writes a date echo and a set-of-codes match writes its code echo or a `CHOOSE 1-n: ` prompt that reads an input, and the GC-03 value is not chosen. PO-09.b14 is BLOCKED | Unchanged |
| O-04 | Which `%RCR` and `%DT` are installed at runtime (`DIRCR`/`DIDT` or `ZTRCR`/`ZTDT`) | OPEN, not blocking: both pairs behave the same on this path | Unchanged |
| O-05 | Image digest, and whether the runtime database is the baseline (`snapshot_id`) | OPEN; the harness checks baseline identity | `image_digest: NOT AVAILABLE TO THE RUN` (`test-results/20261108T100819Z/env.txt`) |
| O-06 | How live mode obtains the original's return value | OPEN; `returned` and `match` are `null` in live mode | Live mode NOT RUN |
| O-07 | `docker_exec_args` and the capture command's context set-up | OPEN; the team confirms | Unchanged |
| O-08 | Pinned toolchain on the build host | OPEN in `SPEC.md`, which leaves the build host's versions to this report | On this host, `rustc 1.99.1` and `cargo 1.99.1` (steps 01 and 02, exit 0), so the question is answered for this host. Any other host still needs the pinned toolchain, and lacking it is BLOCKED with no download |

### Workflow substitution

The request named `PRCHQ1`. It is OPTION 10295, `PRCHQ1^New RFQ`, type R [19+OPTION.zwr:81374]. Its description imports a 2237 into a Request for Quotation [19+OPTION.zwr:81376-81378], its entry action is `S:'$$ESIG^PRCHQ1(DUZ) XQUIT=""` [19+OPTION.zwr:81379], and its routine is `EN2^PRCHQ2` [19+OPTION.zwr:81380]. It is a write path, so it was substituted by `PRCH DISPLAY PO`: IEN 1901, `^DIC(19,1901,0)="PRCH DISPLAY PO^Purchase Order Display^^R^11740^^^^^^^"` [19+OPTION.zwr:14687], with an empty entry action [19+OPTION.zwr:14690] and the routine `EN3^PRCHRPT` [19+OPTION.zwr:14691]. The other candidates and the reasons they were rejected are in "Workflow selection".

### Source corrections and interface departures

Each is recorded identically in `SPEC.md` ("Source corrections and interface departures"), the implementation and the tests.

- **C9, input sequences.** The request's two-line `input.txt` does not end a found-order session cleanly: every session answers the station prompt first, a found order pauses before its items and again at the end, and the option re-prompts. Source governs, one input per line:
  - GC-01, GC-02, GC-05: empty line, the order's `C` value, empty line, empty line, `^`.
  - GC-03: empty line, an absent `C` value with at least one letter (`3UN3N`), `^`.
  - GC-04: empty line, `INVALID!`, `^`.
  - One more empty line per extra `ENTER '^' TO HALT: ` prompt if the original pages (O-01).
- **Identifier form.** The text typed is the common-numbering part after `500-`, the `C` value [DD.zwr:514318-514320]. `meta.txt` `record_ids` records both the IEN and the full PO number.
- **Context keys.** `{"DUZ":".5","IOF":…,"IOSL":…,"STATION":"500"}`. `IOF` and `IOSL` are added to the user's `{"DUZ":".5","STATION":"500"}` because the display reads them.
- **Corrections found while resolving the gate:**
  - The station echo is `      CAMP MASTER`, not the plan's `  500  CAMP MASTER` (R1).
  - The no-match re-ask has two newlines, `<BEL> ??\n\nP.O./REQ.NO.: ` (R3).
  - No IEN attempt is made at the order prompt, so PO-09.b7 is unreachable (R5).
  - PO-09.b12 is unreachable, and PO-09.b15 and PO-09.b16 are appended (R4, R5).
  - A value beginning with `^` exits like `^` and is reported as PO-09.b3.
  - `^XUTL("XQ",$J)` is read, not written, on this path.
  - The pointer-candidate rule also reads files 445 `^PRCP(445,` and 440.5 `^PRC(440.5,`, which the plan's 29 files omit; `SPEC.md` adds them as a source-correction supplement to "FileMan definitions read" (R6).
- **Interface departures:**
  - C12: the capture-format and loader helpers `split_input_lines`, `parse_normalisation`, `normalise`, `parse_meta`, `Meta` and `validate_context` in `src/bridge.rs`, and `load_fixtures` and `Store::format_ref` in `src/store.rs`, are public. Two components must apply each identically, and "No other public operation exists" governs inquiry operations only.
  - C17: `MumpsMode::Live { container: String, golden_dir: PathBuf }`. Live mode must read `meta.txt`, `tests/bridge.rs` must cover that with synthetic directories, and no test may set an environment variable, so the directory reaches the bridge only as an argument.

### Stubs and their rationale

The authority is `## Stubs` in `SPEC.md`, which `comparison_report` copies verbatim into the comparison section. `STUB_CONTEXT` is `[("DUZ", Some(".5")), ("IOF", None), ("IOSL", None), ("STATION", Some("500"))]`.

- **`DUZ` `.5`:** POSTMASTER, set up by the test code `D DUZ^XUP(.5)` [ZOSVGUT1.m:13]. Its node 20 piece 2 is present, so `D A` never writes. The request's tech-spec citation is corrected (C10).
- **`STATION` `500`:** the default `$O(^PRC(411,"AC","Y",0))` [PRCFSITE.m:14], accepted by RETURN.
- **`IOF`, `IOSL` UNRESOLVED:** both are set by `^%ZIS` from the runtime HOME device, which source cannot fix. They stay `None`, so `pct_zis` always returns `UnsupportedBranch("PO-12.b1")` (O-01).
- **`prcfsite` + `dic` output (PO-03 to PO-05):** the constant output of R1, which consumes one input line. An empty line selects 500; a non-empty line returns `UnsupportedBranch("PO-05.b2")`. The site-selection and FileMan code is out of the slice, and its outcome for the fixed context is fully cited.
- **`duz_prcfsite` (PO-04):** no output and no `Store` read. `PRC("PER")` is not modelled, because no output reads it.
- **`ix_dic` (PO-09):** lookup rules over `Store` data (R5, R6), never a list of known identifiers. The rules cover the source's search order, the screen and the pointer-candidate rule for the outcomes the cases exercise.
- **`pct_dt` (PO-41):** no output and no value. `%DT=""` carries no `A` or `E` flag, and the date feeds only fiscal-year locals that nothing on the path displays.
- **Control point:** none is needed. No control-point read or prompt exists on the `EN3` path (C11).
- **Current date and time:** not stubbed, because no output uses them. Normalisation is expected empty.
- **`DTIME`, `U`, `DT`:** preconditions of the capture command only. The Rust READ never times out.

### The original's non-business writes

None is business data, and none is reproduced: the Rust `Store` is borrowed immutably and never written, and the word-wrap scratch lives in `Locals`.

- `^DISV(DUZ,…)` on every successful lookup: `ACT I DIC(0)'["F",$D(DUZ)#2 S ^DISV(DUZ,$E(DIC,1,28))=$E(DIC,29,999)_+Y` [DIC2.m:65].
- `^UTILITY($J,"W")` and `^TMP($J,"W")`, the word-wrap scratch and its copy [PRCHDP1.m:53-54] [PRCHDP2.m:95].
- `K ^TMP($J,"PRCHDP6")` [PRCHDP2.m:25] and `K ^TMP($J,"MDEL")` [PRCHDP4.m:8, 13].
- `^XUTL("XQ",$J)`: read by `^%ZIS` for the HOME device [ZIS.m:28]. Its `IO*` nodes are written by `SAVEVAR^%ZIS` at sign-on [ZIS.m:82-86], not on this path.

### Places where the original looks wrong

Each line was re-read at the baseline commit. Each is reproduced where it lies on an implemented path, and recorded otherwise. None is fixed.

- **The non-specifiable status still displays the order.** `I X<10 W $C(7)," >>> Status makes this record non-specifiable here."` [PRCHRPT.m:73] is followed unconditionally by `S D0=PRCHPO D ^PRCHDP1,K` [PRCHRPT.m:74]. Reproduced in `en30_prchrpt` (PO-06.b2).
- **A read-only inquiry writes `^DISV`.** See [DIC2.m:65] above. Recorded, not reproduced.
- **`LAST PARTIAL RECD.` uses a two-digit year:** `S:X>0 X=X_"  "_$E(Y,4,5)_"/"_$E(Y,6,7)_"/"_$E(Y,2,3)` [PRCHDP1.m:17]. The date is written only when node 11 exists (PO-10.b5, n/e), so it is recorded. The label itself is written by the implemented `prchdp1`.
- **The label `M.O.P.` reads a file named `PAT TYPE`.** `W !,"M.O.P.: ",$P($G(^PRCD(442.5,+$P(PRCHD0,U,2),0)),U,1)` [PRCHDP1.m:16] reads file 442.5, whose header is `^PRCD(442.5,0)="PAT TYPE^442.5^26^13"` [442.5+PAT TYPE.zwr:3]. Reproduced in `prchdp1`, which runs only past the BLOCKED PO-12.b1.
- **`^DD(411,0,"ID",.1)` is exported twice,** with identical text [DD.zwr:488724, 488748]. Recorded. The identifier is evaluated once, into the R1 constant.
- **Two index nodes of file 442 point to a record that does not exist.** `^PRC(442,"AH",31,99999949)=""` and `^PRC(442,"AI",31,99999949)=""` [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:4-5] sit under a header that counts 0 entries [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3], and the export has no `^PRC(442,99999949,…)` node. Recorded. The 442 export is not a fixture.

### Risks

- **O-01 blocks every found-order case.** GC-01, GC-02 and GC-05 stop at `UnsupportedBranch("PO-12.b1")`, and every capture fails on `CONTEXT MISMATCH IOF` until the HOME device is fixed and recorded.
- **O-03 blocks both miss cases.** GC-03 and GC-04 stop at `UnsupportedBranch("PO-09.b14")`. The R3 bytes are recorded but not emitted, and R6's labelling approximation must be made exact before PO-09.b14 can become a transcript.
- **The FileMan echo bytes are traced from source only.** R1 and R2 are source-traced and implemented, but no capture has confirmed them. A disagreement reopens O-02 and keeps the case FAIL.
- **The fixture profile is assumed.** The predicted paths depend on the profile in `tests/golden/ifcap/README.md`. A cancelled order with retained history (GC-05) may carry node 6, node 11 or item partial receipts, which would take it off the profile and make it FAIL.
- **The capture data is not yet supplied.** Neither `cases.zwr` nor its `MANIFEST.txt` line exists yet (`test-results/20261108T100819Z/sha256-fixtures.txt`), so baseline identity, the snapshot match and both database checks are unevaluated. The GC-03 identifier is also still to be chosen.
- **The toolchain must be present on each host.** 1.99.1 is present here. A host without it gets BLOCKED, because rustup's automatic install is disabled.
- **Live mode is untested against VistA.** It has never run against a real container: no VEHU container was available to the run, `docker_exec_args` is O-07, and the live return value is O-06. Live comparison stays NOT COMPARED while O-06 is open.

### A8 coverage

- **Normalisation rules:** none exist. No `normalisation.txt` has been captured (`test-results/20261108T100819Z/sha256-captures.txt`). The format is defined once, by `parse_normalisation` and `normalise` in `src/bridge.rs`:
  - Parsing: blank lines are ignored, and every other line holds exactly one tab between two literal, non-empty delimiters, open then close.
  - Applying: rules apply in file order. Each replaces the text from an open delimiter through the next close delimiter, both included, with `<NORM>`, and an open delimiter without a later close changes nothing.
  - Both sides get the same rules, and there is no other transformation. `comparison_report` reports every rule with the original text it replaced.
- **Stubs:** `## Stubs`, summarised above.
- **Flagged hard features:** `## Flagged hard-MUMPS features`, in three groups:
  - "Requires runtime shim, not converted", so their branches are BLOCKED:
    - `D ^%ZIS` with `W:$Y>0 @IOF` [PRCHDP1.m:12];
    - `W:$Y>0 @IOF` at the column header [PRCHDP2.m:42];
    - `$Y+5>IOSL` paging [PRCHDP2.m:31, 54].
  - Literal operands, reproduced as fixed logic:
    - the `DIC("S")` and station screens;
    - the identifier XECUTE through `WOV^DICQ1`;
    - the station identifier;
    - pre-lookup node 7.5;
    - the pointer-transform screen;
    - name indirection;
    - `%XY^%RCR`;
    - naked references.
  - Not converted (n/e) or not reached:
    - `RCR^DIWW` and `EN^DIQ`/`Y^DIQ`, not converted (n/e);
    - `G @DIPGM`, `G @^("LOOK")`, `D @PRCHQ` and `LOCK`, not reached.
- **Unexercised and BLOCKED branches:** `## Unexercised and BLOCKED branches`. Its inventory has 108 branches: 42 exercised (predicted), 5 BLOCKED and 61 not exercised, not implemented.
  - BLOCKED: PO-09.b14 (O-03); PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1 (O-01).
  - Not exercised and unreachable: PO-03.b5, PO-03.b8, PO-09.b1, PO-09.b7, PO-09.b12, PO-36.b1.
  - Not exercised, not implemented: PO-03.b1, b2, b3, b7, b9; PO-04.b2, b3; PO-05.b2; PO-08.b1; PO-09.b2, b4, b5, b6, b9, b10, b11, b13, b15, b16; PO-10.b1, b2, b3, b5, b6, b7; PO-11.b2; PO-15.b2, b3; PO-18.b2; PO-20.b1; PO-23.b2; PO-25.b1, b3, b4, b5, b7; PO-27.b2, b3, b4, b5; PO-28.b2, b3; PO-29.b1, b3; PO-30.b2; PO-31.b2; PO-32.b2, b3; PO-33.b2; PO-34.b2, b3; PO-36.b2, b3; PO-38.b1, b2.
  - Each returns `UnsupportedBranch` with its ID and writes nothing further.
- **Where the original looks wrong:** "Places where the original looks wrong", above.
- **Scope statement:** `## Scope statement`. It covers this one read-only workflow, OPTION `PRCH DISPLAY PO` (`EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the baseline commit, with `DUZ` `.5` and station 500. No VA-wide equivalence is claimed, and the five cases show only their exercised paths.

The four `SPEC.md` sections named above are copied verbatim into the comparison section by `comparison_report`. The run does not copy them here.

### Run-completion criteria A1 to A8

A1 and A2 are document checks of `SPEC.md` as committed at `1f0c348d3`. The A1 counts are of the bold labels in each `### PO-NN` entry: an inline label such as `**Inputs:** none.` counts, and so does a variant such as `**Reads (original):**`, `**Outputs on b2:**` or `**Outputs, in order, on b4**`. Each "Branch inventory" row is classified by its Coverage column. A3 to A8 restate the subsections above and the saved files they cite.

| Criterion | Check (AAP 0.8.1) | Evidence | Disposition |
|---|---|---|---|
| A1 | `SPEC.md` holds PO-01 to PO-41 under the contract template (AAP 0.3.3), and every branch of every routine from the order prompt through the display is marked "exercised by GC-0N (predicted)" or "not exercised, not implemented" | `SPEC.md` section "Contracts" and its `### PO-NN` entries; "Branch inventory" in `## Unexercised and BLOCKED branches` | **Contracts:** 41 headings, `### PO-01` to `### PO-41`, in order. All eight template groups, **ID and location**, **Rust item**, **Inputs**, **Reads**, **Branches**, **Outputs**, **Side effects** and **Dependencies and coverage**, appear in all 41 entries. **Branches:** 108 rows with 108 distinct IDs, covering all 41 contracts. 42 rows are "exercised by GC-0N (predicted)". 5 are "exercised by GC-0N (predicted)" and BLOCKED: PO-09.b14 (O-03), and PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1 (O-01). 61 are "not exercised, not implemented", 6 of them unreachable. No row lacks a marking or carries both, and every branch ID that a contract entry names is in the inventory. **Result:** the contract headings, the template and the branch marking hold, so A1 is met as a document check |
| A2 | `SPEC.md` Source verification records all seven fields, each open field marked open | `SPEC.md` section "Source verification": its table and "Closure criteria still unmet" | All seven fields are recorded. Baseline: commit SOURCE-RESOLVED, digest OPEN (O-05). Menu option, Entry point and Source location: SOURCE-RESOLVED. Dependencies: IFCAP SOURCE-RESOLVED, with O-01, O-03 and O-04 open (O-02 is closed by R1, R2 and R3; see "Source verification and open items"). Data dependencies: SOURCE-RESOLVED. Capture mapping: Predicted, OPEN until runtime. Every open field is marked open, so the A2 check is met. The Source verification finding itself stays OPEN, because its closure criteria are unmet: no run in the matching VEHU environment, O-01 unresolved, O-05 unrecorded |
| A3 | The six evidence commands ran, each with stdout, stderr and exit status saved; every case is reported NOT COMPARED, none PASS | "Commands" and "A3: evidence commands and case outcomes" above; `test-results/20261108T100819Z/01-rustc-Vv.*` to `06-test-equivalence.*`; the comparison section | Steps 01 to 06 each saved `.stdout`, `.stderr` and `.status`, and each `.status` reads `0`. `06-test-equivalence.stdout` reports GC-01 to GC-05 NOT COMPARED, and the comparison totals read `PASS 0, FAIL 0, NOT COMPARED 5`. Met as a run check. Open as an acceptance item: the five cases stay NOT COMPARED until the captures arrive (B1) |
| A4 | Recorded searches of `src/` for `unsafe`, `unwrap`/`expect`/`panic!` and index expressions | "A4: safety searches" above; `test-results/20261108T100819Z/search-a4.txt` | `unsafe`: one hit, `src/lib.rs:9:#![forbid(unsafe_code)]`, exit 0. The `unwrap`/`expect`/`panic!` search: no hit, exit 1. Index expressions: no hit, exit 1. A4 holds |
| A5 | Recorded search of `src/` for golden-content access | "A5: golden-content access" above; `test-results/20261108T100819Z/search-a5.txt` | 34 hits, exit 0: 29 in the MUMPS-adapter code of `src/bridge.rs` and 5 in `src/bin/bridge.rs`. None is in `src/inquiry.rs`, `src/store.rs`, `src/mumps.rs` or `src/lib.rs`. A5 holds |
| A6 | `cargo test --test bridge` (plus the bridge unit tests in `--lib`) ran; the named assertions and the pass, fail and ignore counts are copied from the saved output | "Commands" and "A6: bridge tests" above; `test-results/20261108T100819Z/04-test-lib.*` and `05-test-bridge.*` | Step 05 exited 0: 24 passed, 0 failed, 0 ignored. That is 23 synthetic routing tests plus `gc_02_end_to_end_compare`, whose outcome is NOT COMPARED, not PASS. Step 04 exited 0: 93 passed, 0 failed, 0 ignored, 34 of them in `bridge::tests`. No test printed NOT RUN. A6 holds as a run check. Synthetic tests are routing evidence, never parity evidence |
| A7 | `git diff --quiet …` exits 0 and `git status --porcelain` outside the crate is empty | "A7: source unchanged" above; `test-results/20261108T035854Z/source-unchanged.txt` (block `# A7 (SPEC.md)`); `test-results/20261108T100819Z/source-unchanged.txt` (block `# A7 (end of run)`); `SPEC.md` section "Source-unchanged check" | Both blocks agree: `# A7 (SPEC.md)` at `HEAD` `8719e11c7415616878d50994ff1d8e898a36876b`, and `# A7 (end of run)` at `HEAD` `b55a56c4b186a23856b99ce8e3062b2d23397362`. In each, the diff exited 0, the status printed nothing and exited 0, and the supplementary diff against the baseline exited 0. A7 holds at the end of the run |
| A8 | `REPORT.md` states the normalisation rules, every stub, every flagged hard feature, every n/e and BLOCKED branch, every place the original looks wrong and is reproduced, and the scope statement (this workflow on `worldvista/vehu:202609` only; no VA-wide equivalence; five cases show only their exercised paths) | "A8 coverage" and the Review subsections it names; the four `SPEC.md` sections that `comparison_report` copies into the comparison section (`## Stubs`, `## Flagged hard-MUMPS features`, `## Unexercised and BLOCKED branches`, `## Scope statement`) | All six are stated. The normalisation rules: none is captured (`test-results/20261108T100819Z/sha256-captures.txt`), and the format and how it applies are given. Also stated: the stubs, the three groups of hard features, the 5 BLOCKED and 61 n/e branches by ID, the six places the original looks wrong, and the scope statement. A8 is met as a document check. The copied sections are harness output |

Nothing above is RUNTIME-VERIFIED: no capture exists and no live run took place. The Source verification finding stays OPEN. Overall acceptance is PENDING (below).

### Status per case and acceptance

| Case | Status (`test-results/20261108T100819Z/06-test-equivalence.stdout`) | Predicted Rust stop on the capture's fixture store | Once captures arrive |
|---|---|---|---|
| GC-01 | NOT COMPARED: 7 capture files and `cases.zwr` missing | `UnsupportedBranch("PO-12.b1")` | FAIL on `CONTEXT MISMATCH IOF: expected UNRESOLVED` until O-01 closes; also reaches the BLOCKED PO-12.b1 |
| GC-02 | NOT COMPARED: same | `UnsupportedBranch("PO-12.b1")` | Same as GC-01 |
| GC-03 | NOT COMPARED: same | `UnsupportedBranch("PO-09.b14")` | FAIL on `CONTEXT MISMATCH IOF: expected UNRESOLVED` until O-01 closes; reaches the BLOCKED PO-09.b14 while O-03 is open |
| GC-04 | NOT COMPARED: same | `UnsupportedBranch("PO-09.b14")` | Same as GC-03 |
| GC-05 | NOT COMPARED: same | `UnsupportedBranch("PO-12.b1")` | Same as GC-01 |

The "Predicted Rust stop" column is `SPEC.md`'s capture mapping ("Source verification") for a store that holds `cases.zwr` within the fixture profile. The unit tests `found_order_stops_at_device_gate` and `absent_identifiers_follow_no_match_path` assert the same stops on synthetic stores. It is not a comparison result.

- **Captures:** Rainman Kemp owns them (the five captures, `cases.zwr` and its `MANIFEST.txt` line), due November 13, 2026, 5:00 p.m. Eastern. Status: assigned, captures pending.
- **B1:** NOT COMPARED. `tests/equivalence.rs` turns each case into PASS or FAIL once the captures load.
- **B2:** overall acceptance is the team's decision, and it is PENDING.
- **B3:** each case run from the screen in MUMPS, RUST and COMPARE modes. NOT RUN.
- **B4:** the "Front-end checklist" in current Chrome and Firefox. NOT RUN.
- **Live GC-01:** NOT RUN, because no VEHU container was available to the run. Live comparison stays NOT COMPARED while O-06 is open.

**Live GC-01 records.** The team records the seven items below for each live GC-01 run. This run has none, so each is NOT RUN. No live run through the bridge can produce them until O-01 closes. `live` in `src/bridge.rs` takes `context` and `docker_exec_args` from the `meta.txt` of the first `GC-0N` directory that has one (`live_meta`; with none, 502 `MALFORMED META <path>`) and checks `context` with `validate_context` before anything runs. `STUB_CONTEXT` holds `IOF` and `IOSL` as `None`, which no value matches, so every live call that reaches the check answers 502 `CONTEXT MISMATCH` and `docker` never runs. With `DUZ` `.5` the cause is `CONTEXT MISMATCH IOF: expected UNRESOLVED, actual <value>` (`synthetic_502_context_mismatch` in `tests/bridge.rs`). The bridge's live 200 body carries only the transcript and its 502 body only the cause, so the team records each item in full itself.

| Record | What the team records | This run |
|---|---|---|
| Exact argument vector | The program `docker` and its arguments as `live` builds them: `exec`, `-i`, the container named by `VEHU_CONTAINER` (read once by `main` in `src/bin/bridge.rs`), then every element of `docker_exec_args`, verbatim. `run_command` starts it directly, never through a shell. Standard input is each input followed by one `\n`, the form of GC-01's `input.txt` | NOT RUN |
| Context | The `meta.txt` `context` object, key by key, and the `validate_context` result against `STUB_CONTEXT` (`DUZ` `.5`, `IOF` and `IOSL` UNRESOLVED, `STATION` `500`) | NOT RUN |
| Duration | How long the `docker` run took. The bridge does not report it; `run_command` kills the child at `LIVE_TIMEOUT`, 10 s, and the cause is then `TIMEOUT` | NOT RUN |
| Exit status | The `docker` exit code, or `NONE` when the child was killed or ended by a signal. Unless a timeout or the output limit came first, a code other than 0, or none, gives 502 `EXIT <code or NONE>: <stderr_head>`. When `docker` cannot start, there is no exit status and the cause is the OS error text | NOT RUN |
| Stdout | The complete standard output. It becomes the transcript, unchanged, only when the run exits 0 within both limits and is UTF-8; otherwise the bridge discards it. Past `LIVE_OUTPUT_LIMIT`, 1 MiB, the child is killed and the cause is `OUTPUT TOO LARGE`; output that is not UTF-8 gives `NON-UTF8` | NOT RUN |
| Stderr | The complete standard error. The bridge reports it only on a non-zero or missing exit code, as `<stderr_head>`: its first 200 bytes, decoded lossily | NOT RUN |
| Comparison with the replay answer | The live transcript against GC-01's replay answer (`MumpsMode::Replay`: GC-01's `transcript.txt` and `returned.txt`), under GC-01's `normalisation.txt`. The bridge never makes this comparison: in live mode, compare pairs the live side with the Rust side, and its `match` is `null` because live `returned` is `null` (O-06) | NOT RUN. NOT COMPARED while O-06 is open; transcript-only agreement never becomes MATCH |

**Overall acceptance: PENDING.** It stays PENDING until all five cases are PASS and the manual and live checks are complete.
