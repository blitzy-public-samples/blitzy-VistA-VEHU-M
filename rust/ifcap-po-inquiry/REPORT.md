# REPORT: PRCH DISPLAY PO (EN3^PRCHRPT) parity

- **Covered:** one read-only VistA workflow, OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the repository at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`, with the fixed context `DUZ` `.5` and station 500.

This file is written in full by `tests/equivalence.rs` on every `cargo test` and is never edited by hand. Exit 0 of `cargo test` is not acceptance, which needs five PASS.

## Part 1: Run evidence

Each command below ran in this checkout from `rust/ifcap-po-inquiry/`, with `RUSTUP_AUTO_INSTALL=0` set so that a missing toolchain fails instead of downloading. Each entry gives the command, its exit status and its output, standard error included. An output is complete unless its entry says it is trimmed.

### Toolchain

`rustc -Vv`, exit status 0:

```text
rustc 1.99.1 (58cf8f9c3 2026-10-13)
binary: rustc
commit-hash: 58cf8f9c35b355199727627d07ae34a4e2846d67
commit-date: 2026-10-13
host: x86_64-unknown-linux-gnu
release: 1.99.1
LLVM version: 23.1.1
```

`cargo -V`, exit status 0:

```text
cargo 1.99.1 (5f94df478 2026-08-27)
```

`cc --version`, exit status 0:

```text
cc (Ubuntu 15.2.0-16ubuntu1) 15.2.0
Copyright (C) 2025 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
```

### Capture availability

`ls -la tests/golden/ifcap/ tests/fixtures/ifcap/`, exit status 0:

```text
tests/fixtures/ifcap/:
total 1208
drwxr-sr-x 2 root root   4096 Nov 15 08:14 .
drwxr-sr-x 3 root root   4096 Nov 15 08:14 ..
-rw-r--r-- 1 root root   1250 Nov 15 08:14 411.2.zwr
-rw-r--r-- 1 root root  10664 Nov 15 08:14 411.zwr
-rw-r--r-- 1 root root   8684 Nov 15 08:14 420.5.zwr
-rw-r--r-- 1 root root   1091 Nov 15 08:14 420.8.zwr
-rw-r--r-- 1 root root 783607 Nov 15 08:14 440.zwr
-rw-r--r-- 1 root root   8686 Nov 15 08:14 442.3.zwr
-rw-r--r-- 1 root root   1484 Nov 15 08:14 442.4.zwr
-rw-r--r-- 1 root root   5465 Nov 15 08:14 442.5.zwr
-rw-r--r-- 1 root root    171 Nov 15 08:14 442.8.zwr
-rw-r--r-- 1 root root   8998 Nov 15 08:14 49.zwr
-rw-r--r-- 1 root root 366907 Nov 15 08:14 5.zwr

tests/golden/ifcap/:
total 48
drwxr-sr-x 2 root root  4096 Nov 15 08:14 .
drwxr-sr-x 3 root root  4096 Nov 15 08:14 ..
-rw-r--r-- 1 root root 38572 Nov 15 08:14 README.md
```

`ls tests/fixtures/ifcap/cases.zwr`, exit status 2:

```text
ls: cannot access 'tests/fixtures/ifcap/cases.zwr': No such file or directory
```

`tests/golden/ifcap/` holds only `README.md` and no `GC-0N` folder, and `tests/fixtures/ifcap/cases.zwr` does not exist, so every case is ABSENT.

### Fixture-to-source equality

This `bash` script compares the sha256 of each of the 11 fixtures with that of its `Packages/` source, prints `EQUAL` or `DIFFERENT` per pair, and exits non-zero when a pair differs or the fixture count is not 11. Exit status 0:

```sh
status=0
for pair in \
  "411.zwr|IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr" \
  "411.2.zwr|IFCAP/Globals/411.2+FACILITY TYPE TEMPORARY.zwr" \
  "420.5.zwr|IFCAP/Globals/420.5+UNIT OF ISSUE.zwr" \
  "420.8.zwr|IFCAP/Globals/420.8+SOURCE CODE.zwr" \
  "440.zwr|IFCAP/Globals/440+VENDOR.zwr" \
  "442.3.zwr|IFCAP/Globals/442.3+PURCHASE ORDER STATUS.zwr" \
  "442.4.zwr|IFCAP/Globals/442.4+PURCHASE AUTHORITY.zwr" \
  "442.5.zwr|IFCAP/Globals/442.5+PAT TYPE.zwr" \
  "442.8.zwr|IFCAP/Globals/442.8+DELIVERY SCHEDULE ORDER.zwr" \
  "5.zwr|Kernel/Globals/5+STATE.zwr" \
  "49.zwr|Kernel/Globals/49+SERVICE-SECTION.zwr"; do
  fixture=${pair%%|*}; source=${pair#*|}
  f=$(sha256sum < "tests/fixtures/ifcap/$fixture" | cut -d' ' -f1)
  s=$(sha256sum < "../../Packages/$source" | cut -d' ' -f1)
  if [ "$f" = "$s" ]; then echo "EQUAL $f $fixture Packages/$source"
  else echo "DIFFERENT $fixture $f Packages/$source $s"; status=1; fi
done
[ "$(ls tests/fixtures/ifcap/*.zwr | wc -l)" -eq 11 ] || status=1
[ "$status" -eq 0 ]
```

Output:

```text
EQUAL 71dd66f898e2e2ae013202d8c45ba1aaca393576c415032c34a6aacf8479c2dc 411.zwr Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr
EQUAL f0f80e9ad3215b9da57d194f22f5134f6485186b996ed72eb42f9815a197a558 411.2.zwr Packages/IFCAP/Globals/411.2+FACILITY TYPE TEMPORARY.zwr
EQUAL 49806f54ffd8796c560f1c60ee167cc4c7963f44a7bb5fc1b9fc52180466c9c9 420.5.zwr Packages/IFCAP/Globals/420.5+UNIT OF ISSUE.zwr
EQUAL 5744e3a5ec5c5ebfa3eb35e5cd76a3aba3c14a79533cebe0792a8ab8f4f0e2ad 420.8.zwr Packages/IFCAP/Globals/420.8+SOURCE CODE.zwr
EQUAL 561cc2c490dd8b2a294341508d634c85dcfccc2423065c492e17f22b1a5cefd5 440.zwr Packages/IFCAP/Globals/440+VENDOR.zwr
EQUAL a17a790e82a4cfa62f170f20243b80926e357d9d440994390dc22b1c2c42bd34 442.3.zwr Packages/IFCAP/Globals/442.3+PURCHASE ORDER STATUS.zwr
EQUAL 0c21733a35b544fe12e5871905c677ad4470f50c0e376915de88f721dea704e0 442.4.zwr Packages/IFCAP/Globals/442.4+PURCHASE AUTHORITY.zwr
EQUAL bc93b9a1b77360c0667ad8564ca43a08f862c3127061d3a0995e4b95228f1899 442.5.zwr Packages/IFCAP/Globals/442.5+PAT TYPE.zwr
EQUAL 8fc831d8f068e8d092244c4e639345f629f96fe543d3ad02a094f8944a8bd187 442.8.zwr Packages/IFCAP/Globals/442.8+DELIVERY SCHEDULE ORDER.zwr
EQUAL 85fba36abf70a2c5e19783b05cf7be102a0100c04e5c2e5b8e72f212d8cf07c3 5.zwr Packages/Kernel/Globals/5+STATE.zwr
EQUAL de7af10fbf737fdc948c1c64d50316f22b108515074075e63507c510a70acb0a 49.zwr Packages/Kernel/Globals/49+SERVICE-SECTION.zwr
```

All 11 fixtures are byte-identical to their sources.

### Build

`cargo build --offline --locked`, exit status 0, with no warning. Output, with its `in <seconds>` timing trimmed:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

### Tests

`cargo test --offline --locked`, exit status 0. Output trimmed to the `Running` or `Doc-tests` line and the `test result:` line of each test binary; the compile lines, the per-test lines and each `finished in <seconds>` timing are omitted:

```text
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)
test result: ok. 131 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   Doc-tests ifcap_po_inquiry
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

148 tests pass: 131 unit tests in `src/`, 15 in `tests/bridge.rs` and 2 in `tests/equivalence.rs`. Its `equivalence_report` test prints one `GC-0N <verdict>` line per case on standard output, which the test runner captures and does not show for a passing test, so the verdicts appear in the next command's output.

`cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`, exit status 0. Output with the compile lines and the `finished in <seconds>` timing trimmed:

```text
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests
test equivalence_report ... GC-01 NOT COMPARED
GC-02 NOT COMPARED
GC-03 NOT COMPARED
GC-04 NOT COMPARED
GC-05 NOT COMPARED
ok
test spec_has_report_sections ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All five cases are NOT COMPARED, because none has a capture. Exit 0 is not acceptance, which needs five PASS.

### Harness checks in this run

- fixtures loaded: PASS — 29820 nodes
- per-case readiness evaluated: PASS — 5 of 5 cases
- store equal to its pre-call clone: PASS — sessions run: 0
- SPEC.md sections found: PASS — 10 of 10

## Part 2: Case results

### GC-01

- Verdict: NOT COMPARED
- Normalisation rules: none (no capture)

### GC-02

- Verdict: NOT COMPARED
- Normalisation rules: none (no capture)

### GC-03

- Verdict: NOT COMPARED
- Normalisation rules: none (no capture)

### GC-04

- Verdict: NOT COMPARED
- Normalisation rules: none (no capture)

### GC-05

- Verdict: NOT COMPARED
- Normalisation rules: none (no capture)

## Part 3: Capture readiness

| Case | Readiness |
|---|---|
| GC-01 | ABSENT |
| GC-02 | ABSENT |
| GC-03 | ABSENT |
| GC-04 | ABSENT |
| GC-05 | ABSENT |

| Check | Definition |
|---|---|
| `seven_files_present` | Each of the seven capture files (`input.txt`, `transcript.txt`, `returned.txt`, `db-before.zwr`, `db-after.zwr`, `meta.txt`, `normalisation.txt`) exists as a regular file in the case folder. |
| `meta_keys_present` | `meta.txt` is UTF-8, parses as `key=value` lines and holds all eight keys `image_digest`, `source_revision`, `snapshot_id`, `record_ids`, `entry_point`, `context`, `setup` and `docker_exec_args`. |
| `input_ends_with_exit` | `input.txt` is UTF-8 and its last line is the exit input `^`. |
| `normalisation_parses` | `normalisation.txt` is UTF-8 and every non-blank line is a non-empty open delimiter, one tab and a non-empty close delimiter (an empty file holds no rules). |
| `db_before_equals_after` | `db-before.zwr` and `db-after.zwr` are both readable and byte-identical. |
| `no_carriage_return` | Every capture file that is present is readable and contains no carriage return. |

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

## Unexercised branches

These branches are outside equivalence: a branch no case exercises is not implemented, no capture judges it, and no verdict in `REPORT.md` covers it. A session that reaches a branch that is not implemented, whether exercised by no case or left unresolved by an open item, ends after the last prompt written. The contract function stops with the private `Halt::Unexercised` carrying the branch ID, and `inquire` returns `Ok`, with the transcript cut at that prompt by `Session::into_transcript_at_prompt` and `returned` `None`, so no output is invented after it. There is no error variant for it: `InquiryError` has only `CorruptStore`.

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
| PO-09.b15 | [DIC0.m:21, 83-87] [DILIBF.m:141, 147] [DIC.m:14] [DIC1.m:41-49] | `^PRC(442,0)` absent, empty or holding only descendants, or its piece 2 not `442I`: no prompt, or other identifiers | not exercised, not implemented |
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

## Normalisation

The rules of a capture's `normalisation.txt`, as `golden::parse_normalisation` parses them into `golden::Rule` values and `golden::normalise` applies them (`src/golden.rs`). Lines are split on `\n` and not trimmed, so a blank line is a zero-length line.

- Each non-blank line of `normalisation.txt` is an open delimiter, one tab, and a close delimiter. Both delimiters are literal and non-empty. Blank lines are ignored.
- Rules apply in file order to both sides. Scanning left to right, the text from each open delimiter through the next close delimiter, both included, becomes `<NORM>`. An open delimiter with no later close delimiter leaves the rest unchanged.
- No other transformation exists. COMPARE uses the same function.
- Rules may cover only run-time timestamps, internal record numbers and other variable fields documented in that capture's `meta.txt`. `REPORT.md` part 2 lists every rule.

## Places the original looks wrong

Each line was re-read at the baseline commit. Each is reproduced where it lies on an implemented path, and recorded otherwise. None is fixed.

- **The non-specifiable status still displays the order.** `I X<10 W $C(7)," >>> Status makes this record non-specifiable here."` [PRCHRPT.m:73] is followed unconditionally by `S D0=PRCHPO D ^PRCHDP1,K` [PRCHRPT.m:74]. Reproduced in `en30_prchrpt` (PO-06.b2).
- **A read-only inquiry writes `^DISV`** [DIC2.m:65]. Recorded, not reproduced.
- **`LAST PARTIAL RECD.` uses a two-digit year:** `S:X>0 X=X_"  "_$E(Y,4,5)_"/"_$E(Y,6,7)_"/"_$E(Y,2,3)` [PRCHDP1.m:17]. The date is written only when node 11 exists (PO-10.b5, n/e), so it is recorded. The label itself is written by the implemented `prchdp1`.
- **The label `M.O.P.` reads a file named `PAT TYPE`.** `W !,"M.O.P.: ",$P($G(^PRCD(442.5,+$P(PRCHD0,U,2),0)),U,1)` [PRCHDP1.m:16] reads file 442.5, whose header is `^PRCD(442.5,0)="PAT TYPE^442.5^26^13"` [442.5+PAT TYPE.zwr:3]. Reproduced in `prchdp1`, which runs only past PO-12.b1 (not implemented, O-01).
- **`^DD(411,0,"ID",.1)` is exported twice,** with identical text [DD.zwr:488724, 488748]. Recorded. The identifier is evaluated once, into the R1 constant.
- **Two index nodes of file 442 point to a record that does not exist.** `^PRC(442,"AH",31,99999949)=""` and `^PRC(442,"AI",31,99999949)=""` [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:4-5] sit under a header that counts 0 entries [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3], and the export has no `^PRC(442,99999949,…)` node. Recorded. The 442 export is not a fixture.

## Existing items outside the layout

Each existing item outside the request's layout is kept only when the layout or the interface names it, a kept item needs it, or it reproduces a source-cited rule on the five cases' path; otherwise it is removed.

| Item | Disposition | Reason |
|---|---|---|
| `test-results/` (33 run folders from `20261108T035854Z` to `20261109T025920Z`, 1002 files) | Deleted | Not in the layout and not required. The evidence the run needs moves into "Run evidence" |
| `tests/fixtures/ifcap/MANIFEST.txt` (11 sha256 lines) | Deleted | Not in the layout (`tests/fixtures/ifcap/*.zwr` only). Fixture-to-source equality is recorded once in "Run evidence" |
| `Cargo.lock` | Kept | `cargo build --offline --locked` requires an existing, current lock file |
| `blitzy/documentation/Project Guide - d3cd5e06-5ae1-42c7-8b11-7bb911fc681f.md` (repository root, outside the crate) | Untouched | Outside the crate and the request |

## Open items

| ID | Open item | Evidence | Effect and current status |
|---|---|---|---|
| O-01 | HOME device context: the bytes `^%ZIS` writes for `IOP="HOME"`, the `IOF` value and the bytes `W @IOF` emits, `IOSL`, and `$Y` after a form feed | [PRCHDP1.m:12] [PRCHDP2.m:31, 42] [ZIS.m:28-37] [ZIS1.m:103-113] | OPEN. PO-12.b1, PO-10.b4, PO-26.b1 and PO-30.b1 are BLOCKED, so GC-01, GC-02 and GC-05 are BLOCKED; `STUB_CONTEXT` holds `IOF` and `IOSL` as `None`. Closure needs the team to fix the capture's HOME device, the run to record its terminal-type values from `3.2+TERMINAL TYPE.zwr` and `3.5+DEVICE.zwr`, and the first capture to confirm the form-feed bytes |
| O-02 | Exact FileMan echo bytes for station default acceptance, the unique order match, and the no-match sequence | [DIC.m:25, 63-65, 80-81] [DIC2.m:8-98] [DIC5.m:21-31] [DIC1.m:46-60] [DICQ1.m:86-88] [DD.zwr:488724, 514202-514204] | **Closed by R1, R2 and R3** of section "Resolution decisions binding src/inquiry.rs"; PO-03, PO-05.b1 and PO-09.b8 are SOURCE-RESOLVED. A capture that disagrees reopens it |
| O-03 | Transform behaviour for the GC-03 and GC-04 values | [DICM.m:13-36] [DICM1.m:13-40] [DICM0.m:8-60] [DIDT.m:8-25] | **Narrowed, still OPEN (R4).** Settled: file 442's own indexes after `C` carry no date, set or variable-pointer field, so PO-09.b12 is unreachable; nested pointer misses write nothing of their own; the soundex echo cannot arise. Open, two questions: the nested lookups keep `M` and run the pointed-to files' own transforms, where a `%DT` conversion writes a date echo [DICM1.m:15-22] and a set-of-codes match writes its code echo or a `CHOOSE 1-n: ` prompt that reads an input [DICM1.m:41-52, 84]; the GC-03 value is not yet chosen. PO-09.b14 is BLOCKED |
| O-04 | Which `%RCR` and `%DT` are installed at runtime: `DIRCR.m` and `DIDT.m` (FileMan 22.2) or `ZTRCR.m` and `ZTDT.m` (FileMan 19.0 copies under Kernel) | [DIRCR.m:1-3, 24-30] [ZTRCR.m:1-3, 21-27] [DIDT.m:1-2, 11-12] [ZTDT.m:1-4] | OPEN, not blocking: each pair behaves the same on this path (subtree copy; no output without `A` or `E`) |
| O-05 | Image digest, and whether the runtime database is commit `dd3fd0f3…` (`snapshot_id`) | `meta.txt` (team) | OPEN; baseline identity is checked by the harness |
| O-06 | How live mode obtains the original's return value | The request | OPEN; `returned` and `match` are `null` in live mode |
| O-07 | `docker_exec_args` and the capture command's context set-up (`DUZ^XUP(.5)`, `U`, `DT`, `DTIME`, HOME device; `DIC`, `DO`, `DST` undefined at option entry) | `DUZ^XUP` sets `DUZ` and `DUZ(…)` but no `DTIME` or `IO*` [XUP.m:54-64] | OPEN; team confirms |
| O-08 | Pinned toolchain presence on the build host | Planning host: `rustc: command not found` | OPEN for this document; `REPORT.md` records the build host's `rustc -Vv` and `cargo -V` from `test-results/<run-id>/` |

## Scope statement

- **Covered:** one read-only VistA workflow, OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the repository at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`, with the fixed context `DUZ` `.5` and station 500.
- **Not claimed:** no VA-wide equivalence; equivalence with any other image, snapshot, site, user, device or option; or any behaviour the five cases do not exercise. The five cases (GC-01 to GC-05) show only their exercised paths.
- **Read-only:** the port has no write path. `PRCHQ1` (OPTION 10295, New RFQ, `EN2^PRCHQ2`) was named in the request but creates RFQs, so `PRCH DISPLAY PO` was substituted. The original's own non-business writes (`^DISV`, word-wrap and delivery scratch under `^UTILITY` and `^TMP`) are documented and not reproduced.
- **Out of scope:** every write path (creating, editing, receiving, amending or cancelling orders); every other IFCAP function and option; VistA GUI clients, terminal devices and terminal emulators; a Java port; every other VistA package; the earlier GMV vitals slice; and the request's "must not be built" list: a write-guard framework, a CLI or exit-code scheme, git or provenance modules, deterministic-report machinery, Windows or macOS branches, an async runtime, any network service other than the loopback bridge, FFI or C bindings, procedural macros, a second crate or workspace, more than two integration test files, a front-end framework or build tooling, authentication or TLS.
- **Unexercised branches:** every branch marked "not exercised, not implemented", and every BLOCKED branch, returns `UnsupportedBranch` with its branch ID and writes no further output.

## Acceptance record

Not accepted: no case has a capture.
