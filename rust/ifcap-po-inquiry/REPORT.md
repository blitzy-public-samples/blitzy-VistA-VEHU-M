# REPORT: PRCH DISPLAY PO (EN3^PRCHRPT) parity

Parity covers OPTION `PRCH DISPLAY PO` (IEN 1901, `EN3^PRCHRPT`), the five cases GC-01 to GC-05 and `worldvista/vehu:202609` only, with no claim of VA-wide equivalence.

This file is written in full by `tests/equivalence.rs` on every `cargo test` and is never edited by hand. Exit 0 of `cargo test` is not acceptance, which needs five PASS.

## Part 1: Run evidence

Each command below ran on 2026-11-15 in this checkout from `rust/ifcap-po-inquiry/` unless its entry says otherwise (the subsections headed "Round one" ran on 2026-11-17, in round one), with `RUSTUP_AUTO_INSTALL=0` set so that a missing toolchain fails instead of downloading. Each entry gives the command, its exit status and its output, standard error included. An output is complete unless its entry says it is trimmed. Round one's records are history. Each "Round one" heading names the commit its record was observed on, or, for a record taken over uncommitted edits, the commit that committed them; when recorded, these subsections were headed "Refine round", and their own cross-references use that name. Round two's baseline and completion records close this section; its fixture-to-source check is in subsection "Fixture-to-source equality". The read-only searches S5 to S9 are in `SPEC.md` section "Read-only enforcement results", the A7 commands in each round's completion evidence below and in section "Source-unchanged verification", and the capture listings in section "Capture hashes".

### Round one: baseline before the first edit (commit `0b123807c`)

These commands ran on 2026-11-17 from `rust/ifcap-po-inquiry/` at commit `0b123807c`, with the five capture folders and `tests/fixtures/ifcap/cases.zwr` present, before any edit of this round other than this record. The capture listing is the "Before" row of section "Capture hashes".

`cargo test --offline --locked`, started 2026-11-17T01:16:44Z, exit status 101. Output trimmed: the lines `test <name> ... ok` are omitted. Cargo stops at the first failing test target, so `tests/equivalence.rs` did not run in this command:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.00s
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)

running 158 tests

test result: ok. 158 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.45s

     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)

running 19 tests
test end_to_end_real_captures ... FAILED

failures:

---- end_to_end_real_captures stdout ----

thread 'end_to_end_real_captures' (6281) panicked at tests/bridge.rs:1722:9:
GC-01: COMPARE answered match false
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    end_to_end_real_captures

test result: FAILED. 18 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.12s

error: test failed, to rerun pass `--test bridge`
```

`cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`, run next to read the five verdicts, exit status 101. Output trimmed as above:

```text
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests
test equivalence_report ... GC-01 FAIL
GC-02 FAIL
GC-03 FAIL
GC-04 FAIL
GC-05 FAIL

thread 'equivalence_report' (6375) panicked at tests/equivalence.rs:773:5:
GC-01 FAIL: transcript differs at line 7: expected `"P.O./REQ.NO.:   500-GC1001  10-01-26  ST   Ordered and Obligated  "`, actual `"P.O./REQ.NO.: "`, attributed to EN3^PRCHPAT, ASK^DIC, PR1^DIC11 (text from PO^PRCHRPT)
GC-02 FAIL: transcript differs at line 7: expected `"P.O./REQ.NO.:   500-GC2002  10-02-26  ST   Ordered and Obligated  "`, actual `"P.O./REQ.NO.: "`, attributed to EN3^PRCHPAT, ASK^DIC, PR1^DIC11 (text from PO^PRCHRPT)
GC-03 FAIL: transcript differs at line 10: expected `""`, actual `<absent>`, not attributed
GC-04 FAIL: transcript differs at line 10: expected `""`, actual `<absent>`, not attributed
GC-05 FAIL: transcript differs at line 7: expected `"P.O./REQ.NO.:   500-GC5005  10-05-26  ST   Cancelled Order  "`, actual `"P.O./REQ.NO.: "`, attributed to EN3^PRCHPAT, ASK^DIC, PR1^DIC11 (text from PO^PRCHRPT)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    equivalence_report

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.85s

error: test failed, to rerun pass `--test equivalence`
```

The baseline is the one the team expected. The exit status is non-zero. `REPORT.md` part 2 gave FAIL for all five cases, and part 3 gave all five READY:

- `GC-01`, `GC-02` and `GC-05` FAIL at transcript line 7. The found order reached the device open `D ^%ZIS` [PRCHDP1.m:12], then not implemented (PO-12.b1), so the session ended after `P.O./REQ.NO.: ` and dropped the echo.
- `GC-03` and `GC-04` FAIL at transcript line 10, expected `""`, actual `<absent>`. The capture ends with one LF after the last `P.O./REQ.NO.: `, and the Rust transcript ended without it.

That run's `REPORT.md` was restored with `git checkout -- REPORT.md`. The committed `REPORT.md` is the one written by the final `cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1` of section "Refine round: completion evidence".

### Round one: `cases.zwr` preflight (commit `68a7a64a9`)

`tests/golden/ifcap/README.md` Part B step 6, conditional path step 4, applied to the five READY cases' `db-before.zwr`. The extraction rule is the first bullet of step 6, with the record numbers of each `meta.txt` `record_ids` (`442:990001`, `442:990002` and `442:990005`, vendor `440:990001`): every `^PRC(442,…)` node of those orders, the `^PRC(442,0)` header, every `^PRC(442,"<index>",…)` node with one of the orders' record numbers as a subscript, the vendor's `^PRC(440,990001,…)` nodes and every `^PRC(440,"<index>",…)` node with subscript `990001`. A line is known when it is in `cases.zwr` or in one of the 11 supporting fixtures `tests/fixtures/ifcap/*.zwr`. It is a text comparison and executes no MUMPS. Exit status 0:

```sh
export LC_ALL=C TMPDIR=/tmp/ifcap-clone-7 && P=$TMPDIR/preflight && mkdir -p $P && for c in GC-01 GC-02 GC-03 GC-04 GC-05; do f=tests/golden/ifcap/$c/db-before.zwr; { grep -E '^\^PRC\(442,0\)=' $f; grep -E '^\^PRC\(442,(990001|990002|990005),' $f; grep -E '^\^PRC\(442,"[^"]*",.*[,(](990001|990002|990005)[,)]' $f; grep -E '^\^PRC\(440,990001,' $f; grep -E '^\^PRC\(440,"[^"]*",.*[,(]990001[,)]' $f; } | sort -u > $P/$c.ext; done; ls tests/fixtures/ifcap/*.zwr | grep -v '/cases.zwr$' | xargs cat | sort -u > $P/supporting; sort -u tests/fixtures/ifcap/cases.zwr > $P/cases; sort -u $P/supporting $P/cases > $P/known; for c in GC-01 GC-02 GC-03 GC-04 GC-05; do echo "$c extracted=$(wc -l < $P/$c.ext) missing=$(comm -23 $P/$c.ext $P/known | wc -l)"; done; echo "in supporting fixtures: $(comm -12 $P/GC-01.ext $P/supporting | wc -l)"; ls tests/fixtures/ifcap/*.zwr | grep -v '/cases.zwr$' | wc -l
```

```text
GC-01 extracted=51 missing=0
GC-02 extracted=51 missing=0
GC-03 extracted=51 missing=0
GC-04 extracted=51 missing=0
GC-05 extracted=51 missing=0
in supporting fixtures: 0
11
```

- The five `db-before.zwr` are byte-identical (`c229942c…c476`), so each case extracts the same 51 lines: the header, 42 order nodes, the six `B` and `C` index nodes, the vendor's zero node and its `B` index node.
- Missing lines: none. Every extracted line is in `cases.zwr`, and none is in a supporting fixture. No open item is added, and `cases.zwr` is not edited.
- `cases.zwr` holds 53 nodes, the 51 lines and `^PRC(442,"AH",31,99999949)=""` and `^PRC(442,"AI",31,99999949)=""`, which are every other `^PRC(442)` line of `db-before.zwr` and are the dangling index nodes of section "Places the original looks wrong".


### Round one: YottaDB references (commit `00981dfbb`)

The `$X`, `$Y`, form-feed and session-end behaviour that `src/mumps.rs` and `src/inquiry.rs` model, with the YottaDB documentation each cites. Fetched 2026-11-17 from `docs.yottadb.com`.

| Behaviour modelled | YottaDB statement | Page and anchor |
|---|---|---|
| Every character written, control characters included, advances `$X` (`Session::write`), so the 26 device-open bytes move `$X` while written | "Generally, YottaDB increments $X for every character written to and read from the current device." "By default, YottaDB does not screen output for escape sequences." | Programmer's Guide, Intrinsic Special Variables, `isv.html#x`; Input/Output Processing, `ioproc.html#filter` |
| `SET $X`, `SET $Y` write nothing (`Session::reset_position`), as `U IO S $X=0,$Y=0` [ZIS4GTM.m:35] and the TRM reset [ZIS1.m:25] do, so `$X` and `$Y` are 0 after the device open | "SET $X does not automatically issue device commands or escape sequences to reposition the physical cursor." The same for `$Y` | `isv.html#x`, `isv.html#y` |
| `WRITE *27` writes the one character ESC | `WRITE *` writes the character whose code is its argument | `ioproc.html#write-star` |
| `W #` ends the record in progress and leaves `$X` and `$Y` 0 (`Session::form_feed`) | "# Sets $X and $Y to zero (0), and terminates the logical record in progress." "WRITE #: Clears $X and $Y and terminates the logical record in progress." | `ioproc.html#maintenance-of-x-and-y`, `ioproc.html#write-command` |
| Documented, not followed (captures judge): `WRITE #` bytes on sequential files (SD) only; neither citation covers a terminal device | READ/WRITE Operations table, which covers STREAM, VARIABLE and FIXED format sequential files, for STREAM and VARIABLE format: "Write <FF>,<LF>, set $X to 0, increment $Y"; the FFLF deviceparameter, "Applies to: SD", has default `<FF><LF>` | `ioproc.html#read-write-operations`; section "Places the original looks wrong" |
| Normal completion: one `\n` when `$X>0` (K14) | CLOSE: "If a partial record has been output, a WRITE ! is done to complete it. To suppress this action, set $X to zero before the CLOSE." Sequential-file CLOSE: "After a WRITE, if $X > 0, Write <LF>". Also: "YottaDB ignores CLOSE $PRINCIPAL." No statement covers HALT or process exit, so O-10 asks the team to confirm | `ioproc.html#close`, `ioproc.html#read-write-operations` |

### Round one: correction cycle (commit `68a7a64a9`)

Command, from `rust/ifcap-po-inquiry/`: `cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`. After each run `REPORT.md` was restored with `git checkout -- REPORT.md`, until the final run of section "Refine round: completion evidence".

**Attempt limit.** The team raised the AAP 0.7.2 limit of three correction attempts per case to five, for this round only. An attempt is one edit to `src/inquiry.rs` or `src/mumps.rs` followed by one run; the first implementation of `STUB_CONTEXT`, PO-12.b1, PO-10.b4, PO-26.b1, PO-30.b1 and the process-exit `\n` is not one.

**Run 0 (first implementation, not an attempt), 2026-11-17T01:35:09Z, exit status 101.** Output trimmed to the verdict lines, the failure message and the summary:

```text
running 2 tests
test equivalence_report ... GC-01 FAIL
GC-02 FAIL
GC-03 PASS
GC-04 PASS
GC-05 FAIL

thread 'equivalence_report' panicked at tests/equivalence.rs:773:5:
GC-01 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
GC-02 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
GC-05 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
test spec_has_report_sections ... ok

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

GC-03 and GC-04 now PASS: the baseline difference at their line 10 was the process-exit `\n`. GC-01, GC-02 and GC-05 match through line 24, including the 26 device-open bytes on line 8, the page-1 header from line 9, and nothing written for `W:$Y>0 @IOF` at PRCHDP1.m:12; they first differ at line 25.

**Run 1, 2026-11-17T01:44:12Z, exit status 101, the same output.** It followed the rest of the first implementation: the unit tests and doc comments, with no rule changed. `cargo test --offline --locked --test bridge -- --nocapture --test-threads=1` in the same state exited 101: 18 passed, and `end_to_end_real_captures` failed with `GC-01: COMPARE answered match false` (tests/bridge.rs:1722).

**Attempt log.** No attempt was made, for any case: no cited source line supports a change to the rule that writes line 25.

| Case | Attempts | First differing line | Expected | Actual | `TAG^ROUTINE` |
|---|---|---|---|---|---|
| GC-01 | 0 of 5 | 25 | `DELIVER ON/BEFORE 10/15/2026 \|CONTRACT:                  \|   POSTMASTER` | `DELIVER ON/BEFORE 10/15/2026 \|CONTRACT:                  \|   ` | `DIS^PRCHDP1` [PRCHDP1.m:59] |
| GC-02 | 0 of 5 | 25 | the same | the same | `DIS^PRCHDP1` [PRCHDP1.m:59] |
| GC-03 | 0 of 5 | none: PASS | | | |
| GC-04 | 0 of 5 | none: PASS | | | |
| GC-05 | 0 of 5 | 25 | the same | the same | `DIS^PRCHDP1` [PRCHDP1.m:59] |

**Review fixes.** The later edits of this round to `src/inquiry.rs`, made after the first review, change only doc comments and the test module, and the later edit to `src/mumps.rs`, made after the second review, changes only the `Session` doc comment (section "Refine round: completion evidence"). They change no rule and no output, so none is a correction attempt, and the counts stay 0 of 5 for every case. The final run of that section gives the same verdicts and the same first differing line as Run 0.

**Isolation of the line-25 difference.** `DIS^PRCHDP1` writes the agent name with `S Y=+$P(PRCHD1,U,10),Y=$P($G(^VA(200,Y,0)),U,1) W ?57,"|  ",$P(Y,",",2)," ",$P(Y,",",1)` [PRCHDP1.m:59], which `dis_prchdp1` reproduces unchanged. Each of the three orders' node 1 piece 10 (field 16 BUYER [DD.zwr:514742]) is `.5`. `^VA(200,.5,0)="POSTMASTER^.5^…"` is in every `db-before.zwr` and in [200+NEW PERSON.zwr:4], but `grep '^\^VA(200' tests/fixtures/ifcap/*.zwr` prints nothing: no fixture holds file 200, and README Part B step 6 extracts only the `^PRC(442)` and `^PRC(440)` nodes, so `cases.zwr` omits the node. `tests/equivalence.rs` loads the store only from `tests/fixtures/ifcap/`. The Rust side therefore reads `""` through `$G` and writes `|   `, while the original writes `|   POSTMASTER`. A check outside the repository, with a scratch copy of the twelve fixtures plus one file holding only that node, gave transcripts byte-equal to all five captures (GC-01 1664, GC-02 1852, GC-03 101, GC-04 101, GC-05 1745 bytes); over the repository fixtures GC-01, GC-02 and GC-05 are each 10 bytes short. The scratch copy is not part of the crate.

**Result.** Closing the difference needs the node in the fixture store: an edit under `tests/fixtures/ifcap/` (a rebuilt `cases.zwr` or a new supporting fixture) together with a revised README Part B step 6. Both are outside the files this refine round may change, and changing `DIS^PRCHDP1`'s rule would special-case the captures. GC-01, GC-02 and GC-05 stay FAIL at line 25 (O-09), GC-03 and GC-04 PASS. This is the refine round's escape valve: the round stops here, and a new request with a revised scope must add the node.

### Round one: completion evidence (commit `00981dfbb` and the fixes committed as `4aae4dba6`)

Run from `rust/ifcap-po-inquiry/` on 2026-11-17, from 05:37:09Z to 05:38:47Z, after this round's last edit to `src/` and `tests/`, both sets of review fixes included, at `HEAD` `00981dfbb4c0bf92c5cf3e5686b0661fc176d1a1`, which holds the first review fixes, with this review round's fixes uncommitted. The `git` commands ran from the repository root. Timings and thread ids are trimmed and the checkout path is shown as `<crate>`.

| Command | Exit status | Result |
|---|---|---|
| `cargo build --offline --locked` | 0 | `Finished` only: the crate was already built from the current sources; no warning |
| `cargo clippy --offline --locked --all-targets` | 0 | `Finished` only: every target was already checked from the current sources, and cargo replays the stored warnings of a target it does not recheck; no warning |
| `cargo fmt --check` | 0 | no output |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked --no-deps --document-private-items` | 0 | `Finished`, `Generated <crate>/target/doc/ifcap_po_inquiry/index.html and 1 other file`, with no `Documenting` line: the documentation was already generated from the current sources with the same flags; no warning |

`cargo test --offline --locked --no-fail-fast`, started 2026-11-17T05:37:39Z, exit status 101. Output trimmed to cargo's `Finished` line, each target's `Running` or `Doc-tests` line and its `test result:` line, every failure message, and cargo's `error:` lines:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s)
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)
test result: ok. 164 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)
test end_to_end_real_captures ... FAILED
thread 'end_to_end_real_captures' panicked at tests/bridge.rs:1722:9:
GC-01: COMPARE answered match false
test result: FAILED. 18 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `--test bridge`
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)
test spec_has_report_sections ... ok
test equivalence_report ... FAILED
GC-01 FAIL
GC-02 FAIL
GC-03 PASS
GC-04 PASS
GC-05 FAIL
thread 'equivalence_report' panicked at tests/equivalence.rs:773:5:
GC-01 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
GC-02 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
GC-05 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `--test equivalence`
   Doc-tests ifcap_po_inquiry
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
error: 2 targets failed:
    `--test bridge`
    `--test equivalence`
```

185 tests: 183 pass and 2 fail, both on the line-25 difference of O-09. The 164 unit tests include the new exact-value tests of section "Contracts", "Test names". The completion target of five PASS and exit status 0 is not reached: GC-03 and GC-04 PASS, GC-01, GC-02 and GC-05 FAIL, and `end_to_end_real_captures` stops at GC-01's `match` false. `--no-fail-fast` makes cargo run `tests/equivalence.rs` after `tests/bridge.rs` fails. That run rewrote `REPORT.md`, which was then restored with `git checkout -- REPORT.md`; the committed `REPORT.md` is the one of the final run below.

`cargo test --offline --locked`, the plain fail-fast form, started 2026-11-17T05:37:19Z, exit status 101. Output trimmed to cargo's `Compiling` and `Finished` lines, each target's `Running` line and its `test result:` line, the failure message and cargo's `error:` line:

```text
   Compiling ifcap-po-inquiry v0.1.0 (<crate>)
    Finished `test` profile [unoptimized + debuginfo] target(s)
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)
test result: ok. 164 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)
test end_to_end_real_captures ... FAILED
thread 'end_to_end_real_captures' panicked at tests/bridge.rs:1722:9:
GC-01: COMPARE answered match false
test result: FAILED. 18 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `--test bridge`
```

Cargo stops at the first failing test target, so `tests/equivalence.rs` did not run: the output has no `Running tests/equivalence.rs` line and no `GC-0N` line. `sha256sum REPORT.md` and `stat -c %y REPORT.md`, taken before and after the command, were the same, `e0faa0c746e81a29afc22ee1281f2956ac68d862534fca98fa3dc3419a96ea93` and `2026-11-17 04:47:33.008079731 +0000`: this command did not write `REPORT.md`.

Audits, run 2026-11-17T05:38:04Z, each exit status as shown; S5 to S9 are in section "Read-only enforcement results":

```text
$ grep -rnE 'unsafe|\.unwrap\(|\.expect\(|panic!' src/          # S1, exit 0
src/lib.rs:9:#![forbid(unsafe_code)]
$ grep -rnE '[A-Za-z_)]\[[^]]+\]' src/                          # S2, exit 0
src/inquiry.rs:786:        // W:DIC(0)["E" ! ends the line before G ASK [DIC.m:63], whose W ! and
$ grep -n 'golden' src/inquiry.rs src/store.rs src/mumps.rs     # S3, exit 1, no output
$ grep -rlE 'input\.txt|transcript\.txt|returned\.txt|meta\.txt|normalisation\.txt|db-before|db-after|tests/golden' src/   # S4, exit 0
src/golden.rs
$ grep -rcE '<same pattern>' src/golden.rs
82
```

The S2 hit is the comment quoting the M code `DIC(0)["E"` [DIC.m:63], not Rust indexing. It was at line 745 before this round and moved with the lines this round added above it.

`git diff --name-only 0b123807c2042e55d006c4812e7fa61b0224ab42`, from the repository root, exit status 0. It compares the working tree with the commit this round started from, so it covers the whole round: the commits `68a7a64a9`, `2476fb27b` and `00981dfbb` and this review round's uncommitted fixes:

```text
rust/ifcap-po-inquiry/REPORT.md
rust/ifcap-po-inquiry/RUNNING.md
rust/ifcap-po-inquiry/SPEC.md
rust/ifcap-po-inquiry/runPlaybook.md
rust/ifcap-po-inquiry/src/bridge.rs
rust/ifcap-po-inquiry/src/golden.rs
rust/ifcap-po-inquiry/src/inquiry.rs
rust/ifcap-po-inquiry/src/mumps.rs
rust/ifcap-po-inquiry/tests/bridge.rs
```

The first review fixes, committed as `00981dfbb`, change `src/inquiry.rs`, `SPEC.md`, `RUNNING.md`, `runPlaybook.md` and `REPORT.md`. In `src/inquiry.rs` they change only doc comments, those of `InquiryError`, its `CorruptStore` variant and `inquire`, and the test module: the doc comment of the helper `no_match_transcript`, and `paging_test_writes_nothing_for_one_and_three_items`, whose direct checks now also run at column 74. In `RUNNING.md` they change lines 29, 31, 55 and 108 (base lines 29, 31, 53 and 106), and in `runPlaybook.md` lines 35 and 36: lines 29 and 31 and the phase 4 rows say that, with the five captures present, plain `cargo test` stops at `tests/bridge.rs` and that the `--test equivalence` command prints the verdict lines and rewrites `REPORT.md`; line 55 says which step 4 items also hold with the captures; line 108 gives the COMPARE outcome per case.

This review round's fixes, uncommitted, change `src/mumps.rs`, `SPEC.md` and, through the final run below, `REPORT.md`. In `src/mumps.rs` they change only the `Session` doc comment, which now quotes YottaDB's `$X` sentence in full ("for every character written to and read from the current device") and states why `Session::read` moves neither `$X` nor `$Y`; no code and no test changes, so no rule and no output changes. In `SPEC.md` they change section "Workflow selection" (the reason the stop condition does not apply), PO-26's outputs in section "Contracts" and section "Places the original looks wrong" (the halt prompt `ENTER '^' TO HALT: ` with its 8 leading spaces, `?8` [PRCHDP2.m:10]), section "Mapping table" (the built-ins gain `WRITE #` to `Session::form_feed` and `SET $X=0,$Y=0` to `Session::reset_position`, and the device I/O row gains `W #`, `W @IOF` with `IOF` `#` and `S $X=0,$Y=0`, with those two functions), the READ and WRITE rule of section "Port rules", O-11's `src/mumps.rs` lines in section "Open items", the `#[cfg(test)]` lines of `src/mumps.rs` and the re-run statement in section "Read-only enforcement results", subsection "Fixture-to-source equality" of this section, and the re-taken measurements of this subsection, of section "Source-unchanged verification" and of section "Capture hashes". A separate fix of this review round, commit `cabfc1ec6`, changes only the "If not" cells of the two phase 4 rows of `runPlaybook.md` (lines 35 and 36), so that each covers both the no-captures state and the five-captures state; it changes no command, `runPlaybook.md` stays 116 lines, and no measurement of this subsection depends on it.

In `src/golden.rs`, `src/bridge.rs` and `tests/bridge.rs` the diff changes only test code: each `IOSL` `"24"` becomes `"60"`, the context with `IOF` `#,$C(27,91,50,74,27,91,72)` moves from accepted to rejected with `CONTEXT MISMATCH IOF: expected #, actual #,$C(27,91,50,74,27,91,72)`, and `CONTEXT MISMATCH IOF: expected UNRESOLVED (O-01), actual ABSENT` becomes `CONTEXT MISMATCH IOF: expected #, actual ABSENT`. The `DUZ` `1` contexts still fail on `DUZ` alone. `RUNNING.md` (lines 29, 31, 39, 51-56 and 106 of the base) and `runPlaybook.md` (lines 8, 35, 36, 42, 43 and 57) keep their no-captures outcome and gain, beside it, the outcome with the five captures present as this run shows it: `GC-03` and `GC-04` PASS, `GC-01`, `GC-02` and `GC-05` FAIL at transcript line 25 (O-09), exit status 101, `RECORDED 5 OF 5 READY 5 OF 5` and `MUMPS BACKEND: REPLAY RECORDED 5 OF 5 READY 5 OF 5`. The outcome the refine round asked them to state, `GC-01 PASS` to `GC-05 PASS` with exit status 0, holds only once O-09 is closed. No command changed. `runPlaybook.md` stays 116 lines. `testingMethods.md` and `tests/golden/ifcap/README.md` are unchanged.

**A7, source unchanged.** Both commands ran from the repository root at 2026-11-17T05:38:47Z, at `HEAD` `00981dfbb4c0bf92c5cf3e5686b0661fc176d1a1` with this review round's fixes uncommitted; each exit status 0, and neither printed anything (no output). The same measurement is the last two rows of section "Source-unchanged verification":

```text
$ git diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION; echo "exit=$?"
exit=0
$ git status --porcelain -- . ':!rust/ifcap-po-inquiry'; echo "exit=$?"
exit=0
```

`Packages/` and the root files `Packages.csv`, `README.rst`, `LICENSE`, `NOTICE` and `ATTESTATION` are byte-identical to `HEAD`, and no file outside `rust/ifcap-po-inquiry/` is modified or untracked. The `git diff --name-only` output above lists only files under `rust/ifcap-po-inquiry/`, so `Packages/` and the root files are also unchanged since the commit this round started from.

**Runtime check.** The bridge ran in this checkout's namespace `pf-18081`, started at 2026-11-17T05:34:25Z by `netns-port-forward 18081 8080 -- ./target/debug/bridge` after `cargo build --offline --locked`, which built it from the current sources at 05:34:13Z, after the last edit to `src/` at 05:33:41Z. Its standard output:

```text
http://127.0.0.1:8080/
MUMPS MODE: REPLAY
RECORDED 5 OF 5 READY 5 OF 5
```

`curl -s http://127.0.0.1:18081/api/status`, run 2026-11-17T05:38:24Z, exit status 0:

```text
{"mumps_mode":"replay","recorded":["GC-01","GC-02","GC-03","GC-04","GC-05"],"ready":["GC-01","GC-02","GC-03","GC-04","GC-05"],"incomplete":{}}
```

`POST /api/compare/session` to `http://127.0.0.1:18081/`, run 2026-11-17T05:38:39Z, with each case's `input.txt` sequence, `Content-Type: application/json` and `Origin: http://127.0.0.1:18081`; the byte counts are the lengths of the two transcripts in the answer:

| Case | Inputs | HTTP | `match` | MUMPS bytes | RUST bytes |
|---|---|---|---|---|---|
| GC-01 | `""`, `GC1001`, `""`, `""`, `^` | 200 | false | 1664 | 1654 |
| GC-02 | `""`, `GC2002`, `""`, `""`, `^` | 200 | false | 1852 | 1842 |
| GC-03 | `""`, `GC3003`, `^` | 200 | true | 101 | 101 |
| GC-04 | `""`, `INVALID!`, `^` | 200 | true | 101 | 101 |
| GC-05 | `""`, `GC5005`, `""`, `""`, `^` | 200 | false | 1745 | 1735 |

For GC-01, GC-02 and GC-05 the two transcripts differ only at line 25, `…|   POSTMASTER` against `…|   `. Each of the five MUMPS transcripts is byte-equal to the case's `transcript.txt`, and so are the RUST transcripts of GC-03 and GC-04. A second instance, `timeout 10 nsenter --net=/run/netns/pf-18081 ./target/debug/bridge`, run from `rust/ifcap-po-inquiry/` at 2026-11-17T05:38:44Z, exited 1, with nothing on standard output and `bridge: port 8080 on 127.0.0.1 is already in use` on standard error. The first instance kept serving: `GET /api/status` answered 200 afterwards.

`REPORT.md` is written by `cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`, run after this document's last edit and committed as that run wrote it. Plain `cargo test --offline --locked` cannot write it in this state, because it stops at `tests/bridge.rs` before `tests/equivalence.rs` runs (its record above). That command, run after every other edit of this document, exited with status 101. Output complete except the thread id and the timings, with the checkout path shown as `<crate>`; it recompiles the test because the test includes this document:

```text
   Compiling ifcap-po-inquiry v0.1.0 (<crate>)
    Finished `test` profile [unoptimized + debuginfo] target(s)
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests
test equivalence_report ... GC-01 FAIL
GC-02 FAIL
GC-03 PASS
GC-04 PASS
GC-05 FAIL

thread 'equivalence_report' panicked at tests/equivalence.rs:773:5:
GC-01 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
GC-02 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
GC-05 FAIL: transcript differs at line 25: expected `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   POSTMASTER"`, actual `"DELIVER ON/BEFORE 10/15/2026 |CONTRACT:                  |   "`, attributed to DIS^PRCHDP1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED
test spec_has_report_sections ... ok

failures:

failures:
    equivalence_report

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out

error: test failed, to rerun pass `--test equivalence`
```

The console output does not depend on this document's text: the verdicts and causes come from the captures and the port, and `spec_has_report_sections` checks only that the report's sections exist. The run that writes the committed `REPORT.md` therefore prints these lines too.

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

History: round one's listing, recorded in commit `68a7a64a9`, before the team committed `200.zwr` in `a78321486`. Since round two `tests/fixtures/ifcap/` also holds `200.zwr` (101 bytes), 13 files in all (subsection "Fixture-to-source equality", round two's run).

`ls -la --time-style=+ tests/golden/ifcap/ tests/fixtures/ifcap/`, run 2026-11-17 in this refine round, exit status 0. The empty time style leaves the modification-time field empty, because Git sets those times when it writes each file into a checkout:

```text
tests/fixtures/ifcap/:
total 1212
drwxr-sr-x 2 root root   4096  .
drwxr-sr-x 3 root root   4096  ..
-rw-r--r-- 1 root root   1250  411.2.zwr
-rw-r--r-- 1 root root  10664  411.zwr
-rw-r--r-- 1 root root   8684  420.5.zwr
-rw-r--r-- 1 root root   1091  420.8.zwr
-rw-r--r-- 1 root root 783607  440.zwr
-rw-r--r-- 1 root root   8686  442.3.zwr
-rw-r--r-- 1 root root   1484  442.4.zwr
-rw-r--r-- 1 root root   5465  442.5.zwr
-rw-r--r-- 1 root root    171  442.8.zwr
-rw-r--r-- 1 root root   8998  49.zwr
-rw-r--r-- 1 root root 366907  5.zwr
-rw-r--r-- 1 root root   2250  cases.zwr

tests/golden/ifcap/:
total 72
drwxr-sr-x 8 root root  4096  .
drwxr-sr-x 3 root root  4096  ..
drwxr-sr-x 2 root root  4096  GC-01
drwxr-sr-x 2 root root  4096  GC-02
drwxr-sr-x 2 root root  4096  GC-03
drwxr-sr-x 2 root root  4096  GC-04
drwxr-sr-x 2 root root  4096  GC-05
-rw-r--r-- 1 root root 37947  README.md
drwxr-sr-x 2 root root  4096  state
```

Each `GC-0N` folder holds the seven files: `db-after.zwr` and `db-before.zwr` (1200849 bytes each), `input.txt` (12 bytes; 10 for GC-03), `meta.txt` (654 bytes; 641 for GC-03 and GC-04), `normalisation.txt` and `returned.txt` (0 bytes each), and `transcript.txt` (GC-01 1664, GC-02 1852, GC-03 101, GC-04 101, GC-05 1745 bytes). All five are READY in `REPORT.md` part 3, and `tests/fixtures/ifcap/cases.zwr` exists, so no file is built. `state/` is not a case (section "Existing items outside the layout"). Before this round `tests/golden/ifcap/` held only `README.md`, `cases.zwr` did not exist, and every case was ABSENT.

### Fixture-to-source equality

History: round one's run, recorded in commit `4aae4dba6`, before the team committed `200.zwr` in `a78321486`. Since round two its count guard finds 12 files other than `cases.zwr` in `tests/fixtures/ifcap/`, so the script exits 1; round two's run, after it, covers all 12 supporting fixtures.

This `bash` script compares the sha256 of each of the 11 supporting fixtures with that of its `Packages/` source, prints `EQUAL` or `DIFFERENT` per pair, and exits non-zero when a pair differs or the count of `tests/fixtures/ifcap/*.zwr` files other than `cases.zwr` is not 11. Run 2026-11-17T05:26:57Z in this refine round, from `rust/ifcap-po-inquiry/`, exit status 0:

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
[ "$(ls tests/fixtures/ifcap/*.zwr | grep -v '/cases.zwr$' | wc -l)" -eq 11 ] || status=1
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

History: before this refine round the count guard counted every `.zwr` file in `tests/fixtures/ifcap/`, which was 11 before `cases.zwr` existed; with `cases.zwr` present that guard counts 12, so it now excludes `cases.zwr`, which has no `Packages/` source.

Round two: the 12 supporting fixtures are the 11 whole-file copies above and `200.zwr`, which is not a whole-file copy: its two ZWR header lines are lines 1 and 2 of `Packages/Kernel/Globals/200+NEW PERSON.zwr`, and its one node is line 4 of that export. This `bash` script runs the 11 comparisons above unchanged, then compares `200.zwr` line by line with those three source lines and requires exactly three lines, and exits non-zero when a comparison differs or the count of `tests/fixtures/ifcap/*.zwr` files other than `cases.zwr` is not 12. Run 2026-11-17T10:25:01Z in round two, at commit `64482eef8`, from `rust/ifcap-po-inquiry/`, exit status 0:

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
f=tests/fixtures/ifcap/200.zwr; s="../../Packages/Kernel/Globals/200+NEW PERSON.zwr"
if [ "$(wc -l < "$f")" -eq 3 ] && [ "$(sed -n 1,2p "$f")" = "$(sed -n 1,2p "$s")" ] && [ "$(sed -n 3p "$f")" = "$(sed -n 4p "$s")" ]
then echo "EQUAL LINES 200.zwr 1-3 Packages/Kernel/Globals/200+NEW PERSON.zwr 1, 2, 4"
else echo "DIFFERENT 200.zwr Packages/Kernel/Globals/200+NEW PERSON.zwr"; status=1; fi
supporting=$(ls tests/fixtures/ifcap/*.zwr | grep -v '/cases.zwr$' | wc -l)
echo "supporting=$supporting all=$(ls tests/fixtures/ifcap/*.zwr | wc -l)"
[ "$supporting" -eq 12 ] || status=1
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
EQUAL LINES 200.zwr 1-3 Packages/Kernel/Globals/200+NEW PERSON.zwr 1, 2, 4
supporting=12 all=13
```

The 11 export copies are byte-identical to their sources, and `200.zwr` is three verbatim lines of its source, so the 12 supporting fixtures all agree with `Packages/`; with `cases.zwr`, `tests/fixtures/ifcap/` holds 13 files.

### Build

History: the earlier run's result; round one's is in section "Round one: completion evidence", and round two's closes this section.

`cargo build --offline --locked`, exit status 0, with no warning. Output, with its `in <seconds>` timing trimmed:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

### Tests

History: the earlier run's results, before round one, with no capture in the checkout. Round one's results are in sections "Round one: correction cycle" and "Round one: completion evidence", and round two's close this section.

`cargo test --offline --locked`, exit status 0. Output trimmed to the `Running` or `Doc-tests` line and the `test result:` line of each test binary; the compile lines, the per-test lines and each `finished in <seconds>` timing are omitted:

```text
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)
test result: ok. 158 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   Doc-tests ifcap_po_inquiry
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

179 tests pass: 158 unit tests in `src/`, 19 in `tests/bridge.rs` and 2 in `tests/equivalence.rs`. Its `equivalence_report` test prints one `GC-0N <verdict>` line per case on standard output, which the test runner captures and does not show for a passing test, so the verdicts appear in the next command's output.

`cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`, exit status 0. Output with the `finished in <seconds>` timings trimmed:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s)
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

All five cases were NOT COMPARED, because none had a capture. Exit 0 was not acceptance, which needs five PASS. `spec_has_report_sections` confirms that every section `REPORT.md` includes exists in this document.

`cargo test --offline --locked --test bridge` (criterion A5), exit status 0. Output with the `finished in <seconds>` timings trimmed; the per-test lines are in completion order:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s)
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)

running 19 tests
test end_to_end_real_captures ... ok
test compare_match_false_for_incomplete_case ... ok
test compare_match_true_for_ready_equal_case ... ok
test compare_match_false_when_difference_is_outside_the_rule ... ok
test compare_match_true_when_rule_covers_the_difference ... ok
test mumps_replay_502_not_recorded ... ok
test mumps_replay_200_when_recorded ... ok
test rust_session_200 ... ok
test not_found_404_texts ... ok
test foreign_host_and_cross_origin_post_400 ... ok
test compare_match_false_for_differing_ready_case ... ok
test bad_request_400_on_every_backend ... ok
test compare_match_false_for_context_duz_1 ... ok
test status_empty_folder ... ok
test startup_with_empty_folder ... ok
test status_ready_case ... ok
test static_paths_exact_match ... ok
test status_incomplete_case ... ok
test status_and_compare_incomplete_on_cr_or_malformed_rule ... ok

test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Format and lint

History: the earlier run's results; round one's are in section "Round one: completion evidence", and round two's close this section.

`cargo fmt --check`, exit status 0, no output. `cargo clippy --offline --locked --all-targets`, exit status 0, with no warning. Output, with its `in <seconds>` timing trimmed and the absolute checkout path shown as `<crate>`:

```text
    Checking ifcap-po-inquiry v0.1.0 (<crate>)
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

### Audit searches S1 to S4

History: the earlier run's results; round one's are in section "Round one: completion evidence", and round two's close this section.

S1, `grep -rnE 'unsafe|\.unwrap\(|\.expect\(|panic!' src/`, exit status 0. The one hit is the allowlisted crate attribute:

```text
src/lib.rs:9:#![forbid(unsafe_code)]
```

S2, `grep -rnE '[A-Za-z_)]\[[^]]+\]' src/`, exit status 0. The one hit, reviewed, is a comment quoting the M code `DIC(0)["E"` [DIC.m:63], not Rust indexing:

```text
src/inquiry.rs:745:        // W:DIC(0)["E" ! ends the line before G ASK [DIC.m:63], whose W ! and
```

S3, `grep -n 'golden' src/inquiry.rs src/store.rs src/mumps.rs`, exit status 1, no output.

S4, `grep -rnE 'input\.txt|transcript\.txt|returned\.txt|meta\.txt|normalisation\.txt|db-before|db-after|tests/golden' src/`, exit status 0, 82 hits, every one in `src/golden.rs`. Output trimmed to the per-file summary of `grep -rlE` and `grep -cE` with the same pattern:

```text
$ grep -rlE '<same pattern>' src/
src/golden.rs
$ grep -rcE '<same pattern>' src/golden.rs
82
```

### Specification counts

History: round one's counts, recorded in commit `00981dfbb`.

Run on this document in this refine round, after its last edit outside this section and section "Reconciliation search"; each exit status 0, except the `grep -c` that counts 0 matches, which exits 1. Commands and outputs:

```text
$ grep -c '^| PO-[0-9]*\.b' SPEC.md
108
$ grep -c '^| PO-' SPEC.md
108
$ grep -c '^### PO-' SPEC.md
41
$ awk -F'|' '/^\| PO-[0-9]+\.b/{print NF}' SPEC.md | sort -u
8
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -cE '\| exercised by GC-0[1-5][^;|]* \|$'
47
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -c '| exercised by GC-01, GC-02, GC-05; not implemented (O-01) |$'
0
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -c '| not exercised, not implemented'
61
```

- `108` is the branch inventory rows (A1), and `8` fields per row means each has six cells.
- `grep -c '^| PO-'` also gives `108`, the inventory rows only, because the IDs in the first cells of the 41 "Mapping table" rows and the 11 "Data reads" rows are quoted in backticks. Before that quoting, in this refine round, it gave `160` (108 + 41 + 11).
- `41` is the contract headings.
- `47`, `0` and `61` are the coverage groups of section "Unexercised branches": exercised and implemented; exercised, not implemented pending O-01; not exercised, not implemented. Before this refine round they were `43`, `4` and `61`.

### Document limits

`wc -l -w testingMethods.md`, exit status 0: `52  600 testingMethods.md` (at most 60 lines and 600 words). `wc -l runPlaybook.md`, exit status 0: `116 runPlaybook.md` (at most 150 lines).

### HL7 domain

From `Packages/IFCAP/Routines/`, `grep -il "HLCS\|HL7\|DOMAIN" PRCHRPT.m PRCFSITE.m PRCHPAT.m PRCHDP1.m PRCHFPNT.m PRCUTL.m PRCHDP2.m PRCHUTL.m PRCHDP6.m PRCHDP4.m`, exit status 1, no output: no routine on the call tree references an HL7 or domain node (section "Stubs").

### Bridge runtime (A12)

History: the earlier run's check, with no capture in the checkout; round one's runtime check is in section "Round one: completion evidence".

Several checkouts share this host, so the bridge, which always binds `127.0.0.1:8080`, ran inside this checkout's own network namespace, `pf-18087`, whose loopback is up, never in the host's. The command `nsenter --net=/run/netns/pf-18087 bash a12.sh` ran this `a12.sh`, exit status 0. Its last command checks that no socket is left listening on port 8080, and its status is the script's:

```sh
out=$(mktemp -d)
cargo run --offline --locked --bin bridge > "$out/out1" 2> "$out/err1" &
cpid=$!
for i in $(seq 1 120); do curl -s -o /dev/null http://127.0.0.1:8080/ && break; sleep 0.5; done
curl -s http://127.0.0.1:8080/api/status; echo " curl=$?"
timeout 10 ./target/debug/bridge; echo "second=$?"
timeout 20 cargo run --offline --locked --bin bridge; echo "cargo-second=$?"
bpid=$(ss -ltnpH "sport = :8080" | sed -n "s/.*pid=\([0-9]*\).*/\1/p" | head -1)
kill "$bpid"; wait "$cpid"; echo "wait=$?"
echo "--- first instance stdout"; cat "$out/out1"
echo "--- first instance stderr"; cat "$out/err1"
! ss -ltnH "sport = :8080" | grep -q .
```

Output, standard error included, with cargo's `in <seconds>` timings trimmed:

```text
{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}} curl=0
bridge: port 8080 on 127.0.0.1 is already in use
second=1
    Finished `dev` profile [unoptimized + debuginfo] target(s)
     Running `target/debug/bridge`
bridge: port 8080 on 127.0.0.1 is already in use
cargo-second=1
wait=143
--- first instance stdout
http://127.0.0.1:8080/
MUMPS MODE: REPLAY
RECORDED 0 OF 5 READY 0 OF 5
--- first instance stderr
    Finished `dev` profile [unoptimized + debuginfo] target(s)
     Running `target/debug/bridge`
```

- The first instance, run by `cargo run` in the background, wrote the three startup lines to standard output, and to standard error only cargo's two status lines, `Finished …` and `` Running `target/debug/bridge` ``.
- `curl -s http://127.0.0.1:8080/api/status` exited 0 and printed `{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}}`.
- The second instance, run as `./target/debug/bridge`, exited with status 1, wrote nothing to standard output, and wrote `bridge: port 8080 on 127.0.0.1 is already in use` to standard error. Run again as `cargo run --offline --locked --bin bridge`, it exited with status 1 and the same message after cargo's `Finished` and `Running` lines.
- The first instance was then stopped by `kill` on the pid owning port 8080 in that namespace. `wait` on the background `cargo run` job returned 143 (128 + 15: it ended on signal 15, SIGTERM). The final check found no socket listening, so the script exited 0.

### Reconciliation search

History: round one's searches, recorded in commit `4aae4dba6`. Their counts and residual hits describe this document as round one left it.

The whole-document search for removed APIs, closed items and superseded wording, run on this document in this refine round after its last edit outside section "Run evidence", which it leaves out, because that section quotes the search and the run outputs. Command and output, exit status 0:

```sh
awk '/^## Run evidence$/{s=1;next} /^## /{s=0} !s' SPEC.md | grep -oE 'UnsupportedBranch|Halt::Error|BLOCKED|422|serve_until|MANIFEST|O-03|test-results|UNSUPPORTED BRANCH|StoreError::Io|absorb|NOT COMPARED|split_input_lines|parse_normalisation|parse_meta|inquiry_error_to_response|decide_live_result|run_command|predicted' | sort | uniq -c
```

```text
      3 422
      3 BLOCKED
      6 MANIFEST
      4 NOT COMPARED
     54 O-03
      1 UNSUPPORTED BRANCH
      5 UnsupportedBranch
      1 decide_live_result
      2 inquiry_error_to_response
      1 parse_meta
      2 parse_normalisation
      1 predicted
      2 run_command
      1 serve_until
      1 split_input_lines
      4 test-results
```

Every residual hit, in the `SPEC.md` sections named, is current or marked as history:

- `O-03`: 30 are the substring of the contract ID `PO-03`; the other 24 name O-03 as closed (R4, "Open items", the request's statement in "Existing-state verification", and the gate that section "Resolution decisions binding src/inquiry.rs" closes) or name the O-03 guard and its fixture conditions.
- `422`: two are `DD.zwr` line numbers (514222, 514228); one, in "Source corrections and interface departures", C12, states that the 422 status is removed.
- `UnsupportedBranch`, `UNSUPPORTED BRANCH`, `BLOCKED`, `serve_until`, `decide_live_result`, `run_command`, `inquiry_error_to_response`: in sentences marked as history ("Before this run", "now"), in the request's statements of "Existing-state verification", in "Port rules", which states that there is no `UnsupportedBranch`, or naming the private functions of `src/bridge.rs` and their unit tests.
- `split_input_lines`, `parse_normalisation`, `parse_meta`: the items of `src/golden.rs`, in "Mapping table" and "Normalisation".
- `MANIFEST`, `test-results`: the deleted items, in "Existing items outside the layout", "Existing-state verification" and "Source-unchanged verification"; and the kept `tests/golden/ifcap/state/MANIFEST.txt`, in "Existing items outside the layout" and the before and after listings of "Capture hashes".
- `NOT COMPARED`: the verdict, never a front-end `match` display.
- `predicted`: "the 2026-11-08 plan predicted" station echo, history, in "Source corrections and interface departures".

The refine round's search for the wording of the zero-capture state and of open items O-01 and O-05, over the same text, exit status 0:

```sh
awk '/^## Run evidence$/{s=1;next} /^## /{s=0} !s' SPEC.md | grep -oE 'O-01|not implemented \(O-01\)|UNRESOLVED|no capture|NOT COMPARED|0 OF 5|ABSENT|RUNTIME-VERIFIED|Predicted from source|OPEN until runtime|image digest|O-05' | sort | uniq -c
```

```text
      2 0 OF 5
      7 ABSENT
      4 NOT COMPARED
     57 O-01
     28 O-05
      3 RUNTIME-VERIFIED
      1 UNRESOLVED
      1 image digest
      5 no capture
      4 not implemented (O-01)
```

Each residual hit is current, closed or marked as history:

- `O-01`: 11 are the substring of the contract ID `PO-01`; the other 46 name O-01 itself: as closed, or as the team's answer that fixes the HOME device context (`IOF` `#`, `IOSL` `60`), in "Source verification", "Workflow selection", "Contracts", "Mapping table", "Stubs", "Flagged hard-MUMPS features", "Unexercised branches", "Source corrections and interface departures" and "Conflict register"; as that answer which, with this refine round's instruction, places the device-open bytes outside the stop condition ("Workflow selection", "Stop condition"); in the request's statement "O-01 blocks PO-12.b1" ("Existing-state verification", "Confirmed at the time"); in its own closed row of "Open items" and in O-07's reference to the HOME device; in the empty coverage group "Exercised, not implemented pending O-01" (0 rows, "Coverage counts"); in the `UNRESOLVED (O-01)` report, stated as no longer arising ("Stubs"); and in sentences marked as history ("Before this refine round", "Before the answer", "while O-01 was open").
- `O-05`: 25 are the substring of `PO-05`; the other 3 name O-05 as closed with the recorded digest and snapshot ("Source verification" baseline row and "Closure criteria", "Open items").
- `not implemented (O-01)`: the former coverage label, in sentences marked as history (the status vocabulary, "Branch granularity", "Scope statement", K9).
- `RUNTIME-VERIFIED`: the status vocabulary, which limits it to GC-03 and GC-04, and the "Capture mapping" row, which applies it to those two cases only.
- `UNRESOLVED`: the `validate_context` report for a `None` constant, stated as no longer arising ("Stubs").
- `image digest`: the "Source verification" baseline row, which records the digest.
- `no capture`: "no capture value overrides" a constant ("Port rules"), "no capture judges" an unexercised branch, "generates no capture data" (R5), K12's B2 rule for a case with no capture, and the equality rule's history sentence.
- `NOT COMPARED`: the verdict for a case with no capture (C2, K12) and the "Existing-state verification" row marked "at the time".
- `0 OF 5`: the zero-captures check of section "Front-end checklist", item 14, a check the team runs on such a checkout.
- `ABSENT`: the `CONTEXT MISMATCH` message format, C2, and the history rows of "Capture hashes" with the listing command's `ABSENT` branch.

### Round two: baseline before the first edit

These commands ran on 2026-11-17 at commit `64482eef86c9d7bfbc3e141576f054bd944dee3b`, before any edit of round two other than this record, with `RUSTUP_AUTO_INSTALL=0` set and the five capture folders, `tests/fixtures/ifcap/cases.zwr` and `tests/fixtures/ifcap/200.zwr` present. They ran from `rust/ifcap-po-inquiry/`, except `git rev-parse HEAD` and the two A7 commands, which ran from the repository root. The A7 results are also the two "Before round two's first edit" rows of section "Source-unchanged verification", and the `200.zwr` hash and the capture listing, taken at 2026-11-17T10:17:14Z, are the "Before (round two)" row of section "Capture hashes".

`git rev-parse HEAD`, 2026-11-17T10:16:27Z, exit status 0:

```text
64482eef86c9d7bfbc3e141576f054bd944dee3b
```

The two A7 commands, at the same time, each followed by `echo "exit=$?"`. Neither command printed anything:

```sh
git diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION; echo "exit=$?"
git status --porcelain -- . ':!rust/ifcap-po-inquiry'; echo "exit=$?"
```

```text
exit=0
exit=0
```

`cargo test --offline --locked --no-fail-fast`, started 2026-11-17T10:16:32Z and finished 2026-11-17T10:16:50Z, exit status 0. Output trimmed: the 185 lines `test <name> ... ok` are omitted; every other line, standard error included, is shown:

```text
   Compiling ifcap-po-inquiry v0.1.0 (/tmp/blitzy/blitzy-VistA-VEHU-M/blitzy-cba44d3a-fb41-4a16-9c11-e135b780df6e-w-003_0d7014/rust/ifcap-po-inquiry)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.09s
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)

running 164 tests

test result: ok. 164 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.61s

     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)

running 19 tests

test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.07s

     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.80s

   Doc-tests ifcap_po_inquiry

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Total over the five test targets: 185 passed (unit tests of `src/lib.rs` 164, `src/bin/bridge.rs` 0, `tests/bridge.rs` 19, `tests/equivalence.rs` 2, doc-tests 0), 0 failed, 0 ignored.

`grep -n 'Verdict' REPORT.md`, on the `REPORT.md` that command wrote, exit status 0:

```text
739:- Verdict: PASS
746:- Verdict: PASS
753:- Verdict: PASS
760:- Verdict: PASS
767:- Verdict: PASS
```

The five lines are the part 2 blocks of `GC-01`, `GC-02`, `GC-03`, `GC-04` and `GC-05`, in that order, headed at lines 737, 744, 751, 758 and 765. Part 3 gave all five READY. The file that run wrote has sha256 `df579b85d883fb2380de8747423bb84d29911e3debcbc12ec168a1527203da6d`, byte-identical to the `REPORT.md` of commit `64482eef8`.

`cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`, run next to read the per-case lines, exit status 0. Output complete:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.00s
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests
test equivalence_report ... GC-01 PASS
GC-02 PASS
GC-03 PASS
GC-04 PASS
GC-05 PASS
ok
test spec_has_report_sections ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.67s
```

The baseline is the one the team expected: exit status 0, 185 passed, 0 failed, and `Verdict: PASS` for all five cases, so the round-two stop condition does not apply and no correction cycle runs. The `REPORT.md` these runs wrote was restored with `git checkout -- REPORT.md`; the committed `REPORT.md` is the one written by the last `cargo test` after this document's final edit.

### Round two: completion evidence

These commands ran on 2026-11-17, from 15:35:49Z to 15:39:11Z, on the Linux build host at commit `5534d6cd22ed5587da1885f5ca776de074ed283c`, the observed commit: it holds every round-two change to `src/`, `tests/`, `RUNNING.md`, `runPlaybook.md` and the golden README, and the tree had no uncommitted change. Only this document and `REPORT.md` change after it. They ran from `rust/ifcap-po-inquiry/` with `RUSTUP_AUTO_INSTALL=0` set, except `git rev-parse HEAD`, the A7 commands and the commit-order check, which ran from the repository root. The checkout path is shown as `<crate>`. `cargo clean` ran first, so the build, the tests, clippy and the documentation were compiled from the committed sources in this run.

**Observed commit.** `git rev-parse HEAD`, 2026-11-17T15:35:49Z, exit status 0, then `git status --porcelain -- rust/ifcap-po-inquiry/src rust/ifcap-po-inquiry/tests` and `git status --porcelain`, each exit status 0 with no output:

```text
5534d6cd22ed5587da1885f5ca776de074ed283c
```

**Build, lint, format and documentation.** The build ran at 15:35:57Z, and the other three ran in sequence from 15:37:34Z to 15:37:36Z. Each exited 0 with no warning. Output complete:

| Command | Exit status | Output |
|---|---|---|
| `cargo build --offline --locked` | 0 | `Compiling ifcap-po-inquiry v0.1.0 (<crate>)`, then ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.81s`` |
| `cargo clippy --offline --locked --all-targets` | 0 | `Checking ifcap-po-inquiry v0.1.0 (<crate>)`, then ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.45s`` |
| `cargo fmt --check` | 0 | none |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked --no-deps --document-private-items` | 0 | `Checking ifcap-po-inquiry v0.1.0 (<crate>)`, `Documenting ifcap-po-inquiry v0.1.0 (<crate>)`, ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.68s``, `Generated <crate>/target/doc/ifcap_po_inquiry/index.html and 1 other file` |

**Tests.** `cargo test --offline --locked`, started 2026-11-17T15:36:17Z and finished 15:37:07Z, exit status 0. Output trimmed: the 189 lines `test <name> ... ok` are omitted; every other line, standard error included, is shown:

```text
   Compiling ifcap-po-inquiry v0.1.0 (<crate>)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 24.37s
     Running unittests src/lib.rs (target/debug/deps/ifcap_po_inquiry-3a953f325e76a084)

running 167 tests

test result: ok. 167 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.27s

     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)

running 20 tests

test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.46s

     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.00s

   Doc-tests ifcap_po_inquiry

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Total over the five test targets: 189 passed (unit tests of `src/lib.rs` 167, `src/bin/bridge.rs` 0, `tests/bridge.rs` 20, `tests/equivalence.rs` 2, doc-tests 0), 0 failed, 0 ignored. That is the baseline's 185 plus the four O-11 tests: `mumps::tests::output_limit_refuses_the_write_that_would_pass_it`, `inquiry::tests::output_limit_ends_session_at_last_prompt`, `inquiry::tests::repeated_found_order_ends_at_the_output_limit` and `rust_session_stops_at_the_output_limit` in `tests/bridge.rs`, each `... ok` in this run.

`grep -n 'Verdict' REPORT.md`, on the `REPORT.md` that command wrote, exit status 0:

```text
891:- Verdict: PASS
898:- Verdict: PASS
905:- Verdict: PASS
912:- Verdict: PASS
919:- Verdict: PASS
```

The five lines are the part 2 blocks of `GC-01`, `GC-02`, `GC-03`, `GC-04` and `GC-05`, in that order, headed at lines 889, 896, 903, 910 and 917 under `## Part 2: Case results` at line 887. Part 1 reads `SPEC.md sections found: PASS — 10 of 10`, and part 3 gives all five READY. The line numbers belong to the file that run wrote; the committed `REPORT.md`, written after this subsection was added, carries it in part 1, so its line numbers are later.

`cargo test --offline --locked --test bridge end_to_end_real_captures -- --nocapture`, 2026-11-17T15:37:21Z, exit status 0. Output complete. The test prints `GC-0N MATCH` only after its assertion that COMPARE answered `match` true for that READY case:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.00s
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)

running 1 test
GC-01 MATCH
GC-02 MATCH
GC-03 MATCH
GC-04 MATCH
GC-05 MATCH
test end_to_end_real_captures ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 19 filtered out; finished in 1.60s
```

`cargo test --offline --locked --test equivalence -- --nocapture --test-threads=1`, 2026-11-17T15:37:22Z, exit status 0. Output complete:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.00s
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)

running 2 tests
test equivalence_report ... GC-01 PASS
GC-02 PASS
GC-03 PASS
GC-04 PASS
GC-05 PASS
ok
test spec_has_report_sections ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.13s
```

**Audits**, run 2026-11-17T15:37:47Z, each exit status as shown. S6 to S9 gave the outputs recorded in section "Read-only enforcement results":

```text
$ grep -rnE 'unsafe|\.unwrap\(|\.expect\(|panic!' src/          # S1, exit 0
src/lib.rs:9:#![forbid(unsafe_code)]
$ grep -rnE '[A-Za-z_)]\[[^]]+\]' src/                          # S2, exit 0
src/inquiry.rs:827:        // W:DIC(0)["E" ! ends the line before G ASK [DIC.m:63], whose W ! and
$ grep -n 'golden' src/inquiry.rs src/store.rs src/mumps.rs     # S3, exit 1, no output
$ grep -rlE 'input\.txt|transcript\.txt|returned\.txt|meta\.txt|normalisation\.txt|db-before|db-after|tests/golden' src/   # S4, exit 0
src/golden.rs
$ grep -rnE '<same pattern>' src/ | wc -l
82
$ grep -rcE '<same pattern>' src/golden.rs
82
$ grep -rn '&mut Store' src/                                    # S5, exit 1, no output
$ grep -n '&mut self' src/store.rs                              # S5, exit 1, no output
```

S1 has exactly one hit, the allowlisted crate attribute. The one S2 hit, reviewed, is the comment quoting the M code `DIC(0)["E"` [DIC.m:63], not Rust indexing; it was at line 786 at commit `64482eef8` and moved with the lines the O-11 change added above it. S3 and S5 print nothing. All 82 S4 hits are in `src/golden.rs`.

**A7, source unchanged.** Both commands ran from the repository root at 2026-11-17T15:38:29Z, at `HEAD` `5534d6cd22ed5587da1885f5ca776de074ed283c` with no uncommitted change; each exit status 0, and neither printed anything. The same measurement is the two "After round two's last edit" rows of section "Source-unchanged verification":

```text
$ git diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION; echo "exit=$?"
exit=0
$ git status --porcelain -- . ':!rust/ifcap-po-inquiry'; echo "exit=$?"
exit=0
```

**Capture and fixture hashes.** The `200.zwr` hash and the capture listing, taken at 2026-11-17T15:38:14Z, are the "After (round two)" row of section "Capture hashes", with the listing verbatim there. It is identical to round two's before listing (`diff` exit status 0), and all 18 stated hashes hold: `200.zwr` `836c6a9b1ecb94d8246a8c7b34212ca82361fa11c3db918b6e0cf1ffd09ac9da`, `cases.zwr` `9dd56f96bf29c5bb8e0e373cbaf2fb3f912daab4b41bf65fec1df76485da32a0`, the ten `db-before.zwr` and `db-after.zwr` `c229942c5a7b9f817ab06bfee1eb0f96bbc63d87e4be48478ccc62f249f5c476`, the five transcripts and `state/state.zwr`.

**Runtime check.** The bridge ran in this checkout's own network namespace, `pf-18080`, never in the host's, started at 2026-11-17T15:38:37Z by `netns-port-forward 18080 8080 -- ./target/debug/bridge` after `cargo build --offline --locked` (`Finished` only, the build above being current). Its standard output:

```text
http://127.0.0.1:8080/
MUMPS MODE: REPLAY
RECORDED 5 OF 5 READY 5 OF 5
```

`curl -s http://127.0.0.1:18080/api/status`, 2026-11-17T15:38:48Z, exit status 0:

```text
{"mumps_mode":"replay","recorded":["GC-01","GC-02","GC-03","GC-04","GC-05"],"ready":["GC-01","GC-02","GC-03","GC-04","GC-05"],"incomplete":{}}
```

`POST /api/compare/session` to `http://127.0.0.1:18080/`, 2026-11-17T15:38:48Z, with each case's `input.txt` sequence, `Content-Type: application/json` and `Origin: http://127.0.0.1:18080`. The byte counts are the lengths of the two transcripts in the answer, read with `jq -j`, and each transcript was compared with the case's `transcript.txt` by `cmp`:

| Case | Inputs | HTTP | `match` | MUMPS bytes | RUST bytes | Both equal to `transcript.txt` |
|---|---|---|---|---|---|---|
| GC-01 | `""`, `GC1001`, `""`, `""`, `^` | 200 | true | 1664 | 1664 | yes |
| GC-02 | `""`, `GC2002`, `""`, `""`, `^` | 200 | true | 1852 | 1852 | yes |
| GC-03 | `""`, `GC3003`, `^` | 200 | true | 101 | 101 | yes |
| GC-04 | `""`, `INVALID!`, `^` | 200 | true | 101 | 101 | yes |
| GC-05 | `""`, `GC5005`, `""`, `""`, `^` | 200 | true | 1745 | 1745 | yes |

At 2026-11-17T15:38:58Z, `POST /api/rust/session` with `{"inputs":[""]}` answered 200 with an 80-byte transcript ending `P.O./REQ.NO.: `, the order prompt over `cases.zwr`, and `POST /api/mumps/session` with the same body answered 502 `{"error":"NOT RECORDED"}`. At 15:39:11Z, `POST /api/rust/session` with a 900,004-byte body, `""`, then 59,999 × `"GC1001","",""`, then `"^"`, answered 200 in 1.194 s with a 1,048,026-byte transcript, within `SESSION_OUTPUT_LIMIT` (1,048,576), ending `P.O./REQ.NO.: ` with no `\n`. A second instance, `timeout 10 nsenter --net=/run/netns/pf-18080 ./target/debug/bridge`, run from `rust/ifcap-po-inquiry/`, exited 1 with `bridge: port 8080 on 127.0.0.1 is already in use`, and `GET /api/status` still answered 200 afterwards. The bridge was then stopped, and no socket was left listening on port 18080.

**Fence verification**, run at `HEAD` `5534d6cd22ed5587da1885f5ca776de074ed283c` with no uncommitted change, against round two's base commit `64482eef86c9d7bfbc3e141576f054bd944dee3b`. From `rust/ifcap-po-inquiry/`, each exit status 0 unless shown:

```text
$ git diff --name-status 64482eef86c9d7bfbc3e141576f054bd944dee3b -- .
M	rust/ifcap-po-inquiry/RUNNING.md
M	rust/ifcap-po-inquiry/SPEC.md
M	rust/ifcap-po-inquiry/runPlaybook.md
M	rust/ifcap-po-inquiry/src/inquiry.rs
M	rust/ifcap-po-inquiry/src/mumps.rs
M	rust/ifcap-po-inquiry/tests/bridge.rs
M	rust/ifcap-po-inquiry/tests/golden/ifcap/README.md
$ git diff --quiet 64482eef8 -- src/bridge.rs src/lib.rs src/golden.rs src/store.rs src/bin/bridge.rs tests/equivalence.rs frontend testingMethods.md Cargo.toml Cargo.lock rust-toolchain.toml; echo $?
0
$ git diff --quiet 64482eef8 -- tests/fixtures tests/golden/ifcap/GC-01 tests/golden/ifcap/GC-02 tests/golden/ifcap/GC-03 tests/golden/ifcap/GC-04 tests/golden/ifcap/GC-05 tests/golden/ifcap/state; echo $?
0
$ git diff -U0 64482eef8 -- tests/golden/ifcap/README.md | grep '^@@'
@@ -370,0 +371,2 @@ Run each case a second time from the restored snapshot, and compare the two runs
$ git diff 64482eef8 -- SPEC.md | grep -E '^[-+]\| (O-07|O-10|K14) '; echo $?
1
$ grep -c '^| PO-' SPEC.md
108
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -cE '\| exercised by GC-0[1-5][^;|]* \|$'
47
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -c '| exercised by GC-01, GC-02, GC-05; not implemented (O-01) |$'
0
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -c '| not exercised, not implemented'
61
```

- Every changed path is in round two's file list; `REPORT.md`, also in it, changes only through `cargo test`. `src/bridge.rs` (its test module included), `src/lib.rs`, `src/golden.rs`, `src/store.rs`, `src/bin/bridge.rs`, `tests/equivalence.rs`, `frontend/`, `testingMethods.md`, `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` are byte-identical to the base, and so are the fixtures and the capture folders. From the repository root, `git diff --quiet 64482eef8 -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION` exits 0 and `git diff --name-only 64482eef8 -- . ':!rust/ifcap-po-inquiry'` prints nothing.
- `Session::into_transcript_at_prompt` is unchanged: its body and doc comment, with 12 lines before and 10 after, extracted from `src/mumps.rs` at the base (line 713) and now (line 776), compare equal (`diff` exit status 0).
- The golden README diff removes no line and adds two, one paragraph and its blank line, at the end of Part B step 6, before `### Step 7. Checks`. The paragraph gives the command `grep -F '^VA(200,.5,0)=' tests/golden/ifcap/GC-01/db-before.zwr`.
- The O-07, O-10 and K14 rows are unchanged. The branch inventory has 108 rows, with coverage 47/0/61, and every heading `tests/equivalence.rs` needs is present once: "Run evidence", "Scope statement" and the nine static sections, "Acceptance record" and "Open items" among them.
- `Halt` has four variants, `InputExhausted`, `OutputLimit`, `Unexercised(&'static str)` and `Corrupt(String)`, and no derive or visibility line of it changed. `rust-toolchain.toml` pins `1.99.1`, `Cargo.toml` gives edition `2024` and an empty `[dependencies]`, and `src/lib.rs:9` is `#![forbid(unsafe_code)]`.
- `git log --reverse --format='%h %s' 64482eef8..5534d6cd2`, from the repository root, gives the order of work: the baseline, then the documents and O-09, then O-11:

```text
109a4af2c ifcap-po-inquiry: record the round-two baseline, A7 before-check and the 200.zwr hash
f4f3a07e9 ifcap-po-inquiry: state the five-PASS outcome in the run guides; buyer-node note; open items O-12 to O-14
f2cc2811d ifcap-po-inquiry: close O-09, list 200.zwr, reconcile SPEC.md present tense
5534d6cd2 ifcap-po-inquiry: bound one Rust session's output (O-11, K15)
```

**Document limits.** `wc -l -w testingMethods.md` gives `52  600 testingMethods.md`, and `wc -l runPlaybook.md` gives `116 runPlaybook.md`, each exit status 0 (at most 60 lines and 600 words, and at most 150 lines).

**Result.** Every completion condition of round two holds on the Linux build host at commit `5534d6cd2`: `cargo test --offline --locked` exits 0 with 189 passed and 0 failed; `REPORT.md` part 2 reads `Verdict: PASS` for all five cases; `end_to_end_real_captures` gets `match` true for each READY case; build, clippy, format and documentation exit 0 with no warning; S1 has one hit, S3 and S5 none, and S4 hits only `src/golden.rs`; A7 holds; every stated hash holds. This round records no acceptance (section "Acceptance record"). The `REPORT.md` these runs wrote was restored with `git checkout -- REPORT.md`. The committed `REPORT.md` is the one `cargo test --offline --locked` wrote after this document's final edit; a second run left its sha256 unchanged, which shows it current.

### Harness checks in this run

- fixtures loaded: PASS — 29874 nodes
- per-case readiness evaluated: PASS — 5 of 5 cases
- store equal to its pre-call clone: PASS — sessions run: 5
- SPEC.md sections found: PASS — 10 of 10

## Part 2: Case results

### GC-01

- Verdict: PASS
- Normalisation rules: none
- image_digest: `"worldvista/vehu@sha256:e0710df4ab487831ad84d0383637488da2baa37168ce8ac8e71e8cd039590622"`
- snapshot_id: `"sha256:a8089ad06fcca92d40659c2a1503cd6f4ef829f407df547d939275e6892bace4"`

### GC-02

- Verdict: PASS
- Normalisation rules: none
- image_digest: `"worldvista/vehu@sha256:e0710df4ab487831ad84d0383637488da2baa37168ce8ac8e71e8cd039590622"`
- snapshot_id: `"sha256:a8089ad06fcca92d40659c2a1503cd6f4ef829f407df547d939275e6892bace4"`

### GC-03

- Verdict: PASS
- Normalisation rules: none
- image_digest: `"worldvista/vehu@sha256:e0710df4ab487831ad84d0383637488da2baa37168ce8ac8e71e8cd039590622"`
- snapshot_id: `"sha256:a8089ad06fcca92d40659c2a1503cd6f4ef829f407df547d939275e6892bace4"`

### GC-04

- Verdict: PASS
- Normalisation rules: none
- image_digest: `"worldvista/vehu@sha256:e0710df4ab487831ad84d0383637488da2baa37168ce8ac8e71e8cd039590622"`
- snapshot_id: `"sha256:a8089ad06fcca92d40659c2a1503cd6f4ef829f407df547d939275e6892bace4"`

### GC-05

- Verdict: PASS
- Normalisation rules: none
- image_digest: `"worldvista/vehu@sha256:e0710df4ab487831ad84d0383637488da2baa37168ce8ac8e71e8cd039590622"`
- snapshot_id: `"sha256:a8089ad06fcca92d40659c2a1503cd6f4ef829f407df547d939275e6892bace4"`

## Part 3: Capture readiness

| Case | Readiness |
|---|---|
| GC-01 | READY |
| GC-02 | READY |
| GC-03 | READY |
| GC-04 | READY |
| GC-05 | READY |

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

`STUB_CONTEXT` in `src/inquiry.rs` is exactly `[("DUZ", Some(".5")), ("IOF", Some("#")), ("IOSL", Some("60")), ("STATION", Some("500"))]`. `IOF` and `IOSL` are the team's answer to O-01, as each capture's `meta.txt` `context` and `setup` record it: an input to the port, not a rule derived from golden content. Before this refine round they were `None`. Every capture's `meta.txt` `context` must be `{"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}`, and `golden::validate_context` checks it against `STUB_CONTEXT` key by key:

- every `STUB_CONTEXT` key must be present;
- a resolved constant must be equal;
- a key whose constant is `None` would accept any value and be reported `UNRESOLVED (O-01)`; since O-01 closed no constant is `None`, so a context whose `IOF` is not `#` or whose `IOSL` is not `60` fails, for example `CONTEXT MISMATCH IOSL: expected 60, actual 24`;
- an extra key fails;
- a failure reads `CONTEXT MISMATCH <key>: expected <value or ABSENT>, actual <value or ABSENT>`, for example `CONTEXT MISMATCH DUZ: expected .5, actual 1`.

No capture value overrides a constant. Diagnostic values such as `IOM` and the HOME device's name go in `meta.txt` `setup`, never in `context` (O-01). This replaces the rule, before this run, that failed every capture on `IOF` while O-01 was open.

| Stub | Original source | Fixed value or behaviour | Status |
|---|---|---|---|
| `DUZ` | Test code `D DUZ^XUP(.5)` [ZOSVGUT1.m:13]; POSTMASTER `^VA(200,.5,0)` [200+NEW PERSON.zwr:4]; node 20 piece 2 `POSTMASTER` [200+NEW PERSON.zwr:12], so `D A` never writes [PRCFSITE.m:38]. The request cites tech-spec section 9.1 for this test code; the code is in `ZOSVGUT1.m`, and 9.1 records only station 500 (conflict C10) | `".5"` | SOURCE-RESOLVED |
| `STATION` | Default `$O(^PRC(411,"AC","Y",0))` [PRCFSITE.m:14] = 500 [411 export:290]; accepted by RETURN [PRCFSITE.m:18] | `"500"` | SOURCE-RESOLVED |
| `IOF` | `W:$Y>0 @IOF` [PRCHDP1.m:12] [PRCHDP2.m:42]; set by `^%ZIS` from the HOME device's terminal type [ZIS1.m:103, 106, 113]: device 267 `PMT290`, `$I` `0` [3.5+DEVICE.zwr:14-18, 679], terminal type 168 `PK-VTMT` node 1 `132^#^60` [3.2+TERMINAL TYPE.zwr:816-817] | `"#"`: `W @IOF` is `W #`, the form feed of `form_feed` (PO-10.b4, PO-26.b1) | SOURCE-RESOLVED by the team's answer to O-01 (closed) |
| `IOSL` | `$Y+5>IOSL` [PRCHDP2.m:31, 54]; set by `^%ZIS` [ZIS1.m:103-104] from the same node `132^#^60` [3.2+TERMINAL TYPE.zwr:817] | `"60"`: `ask_prchdp2` decides `$Y+5>IOSL` with it (PO-30.b1) | SOURCE-RESOLVED by the team's answer to O-01 (closed) |
| `prcfsite` + `dic` output (PO-03 to PO-05) | `W !` [PRCFSITE.m:8]; `W !` [PRCFSITE.m:18]; `W !` [DIC.m:25]; prompt and default [DIC11.m:15-17]; echo [DIC2.m:10, 32] through `^DD(411,0,"ID",.1)` [DD.zwr:488724] and `^DIC(4,500,0)="CAMP MASTER^…"` [4+INSTITUTION.zwr:494] | Writes `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// `; consumes one input line; an empty line writes `      CAMP MASTER` (six spaces, no newline) and sets `PRC("SITE")=500`; a non-empty line ends the session after the station prompt (PO-05.b2, not implemented); no line left is `Halt::InputExhausted`, which ends it there too. Reads nothing from `Store` | SOURCE-RESOLVED (byte trace R1 in SPEC.md) |
| `duz_prcfsite` (PO-04) | [PRCFSITE.m:35-40] | No output; no `Store` read; `PRC("PER")` not modelled because no output reads it | SOURCE-RESOLVED |
| `ix_dic` lookup (PO-09) | `IX^DIC` [DIC.m:84-86] and the lookup routines | Lookup rules over `Store` data (R5, R6), never a list of known identifiers. The unique-match echo is resolved; the search-exhausted outcome PO-09.b14 is implemented: the R3 sequence, under the O-03 guard of R5 step 6, then the prompt again | b3, b8, b14 SOURCE-RESOLVED (O-03 closed for the five cases' values) |
| `pct_zis` (PO-12) | `S IOP="HOME",%ZIS="" ... D ^%ZIS` [PRCHDP1.m:12]; `D L2^%ZIS2` [ZIS1.m:19]; `TRM^%ZIS3` [ZIS3.m:12, 21, 24-25, 50-51]; `U IO S $X=0,$Y=0` [ZIS4GTM.m:35], `OXECUTE^%ZIS6` [ZIS4GTM.m:38] [ZIS6.m:6, 21] [ZISX.m:15, 21-30]; open execute `^%ZIS(2,168,2)` [3.2+TERMINAL TYPE.zwr:818]; TRM reset [ZIS1.m:25] with `IO("S")` set [ZIS2.m:44] | Fixed-context rule equivalent of the HOME device open (PO-12.b1): writes exactly the 26 bytes `ESC[5i ESC[6w ESC[4i ESC[?5i ESC[?3h ESC[6w` (`1B 5B 35 69 1B 5B 36 77 1B 5B 34 69 1B 5B 3F 35 69 1B 5B 3F 33 68 1B 5B 36 77`), with no line terminator, and returns with `$X` and `$Y` 0; evaluates no M code; reads nothing from `Store` | SOURCE-RESOLVED (O-01 closed); flagged "requires runtime shim, not converted" |
| `pct_dt` (PO-41) | `D ^%DT` with `%DT=""`, `X="T"` [PRCFSITE.m:15]; `%DT` writes only under the `A` or `E` flag [DIDT.m:11-12, 63, 67, 92] [ZTDT.m:3-4, 43, 46, 55] | No output and no value. The date feeds only `PRC("FY")`, `PRC("QTR")`, `A` and `B` [PRCFSITE.m:15]; `OUT` kills `A`, `B`, `X`, `Y` and leaves `PRC("FY")`, `PRC("QTR")` defined [PRCFSITE.m:29]; their only readers on the path are the copy into `X` at `FY`, which `SE1` overwrites [PRCFSITE.m:21, 26], and the year and quarter prompts and budget line that `PRCF("X")="SP"` skips [PRCFSITE.m:20, 23, 28] [PRCHRPT.m:5]; `EN3` runs `ST` once [PRCHRPT.m:70-75] and no display routine reads them. So they are omitted | SOURCE-RESOLVED (runtime identity O-04) |
| Control point | No control-point read or prompt anywhere on the `EN3` path (conflict C11) | None needed | SOURCE-RESOLVED |
| Current date and time | Not stubbed: the one date the path computes, through `^%DT`, reaches no output | Handled only through `normalisation.txt`, expected empty | Team confirms with the captures |
| `DTIME`, `U`, `DT` | `R X:DTIME` [PRCHDP2.m:10, 41]; `DT` [PRCFSITE.m:7]; `U` [PRCFSITE.m:9] | Preconditions set by the capture start command (O-07), with `DT=$$DT^XLFDT` [XLFDT.m:159]. Because `DT` is defined, the `NOW^%DTC` branch PO-03.b2 [PRCFSITE.m:7] stays not exercised. The Rust READ never times out | README states them (O-07) |
| HL7 domain `HL7.VEHU.DOMAIN.GOV` | No routine on the call tree references an HL7 or domain node: `grep -il "HLCS\|HL7\|DOMAIN"` over the ten call-tree routines finds none | Not stubbed and not in `context` | Recorded as instance identity only |

## Flagged hard-MUMPS features

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`.

Rule (conflict K10, superseding C8): every indirection, XECUTE, LOCK or device-control feature on the call tree is flagged "requires runtime shim, not converted" here, and `REPORT.md` copies this section. No XECUTE, indirection or LOCK evaluator and no device shim exists in the crate. Separately, where the operand is a literal fixed in the pinned source or data dictionary on an exercised path, `src/inquiry.rs` holds a rule equivalent: ordinary Rust code reproducing the observable effect of that one literal, cited to the literal's line. It is not a conversion of the language feature: it evaluates no M code and would not follow a change to the literal. Without the rule equivalents for the lookup screens and identifiers no case could reach its output, while the flag keeps every such site visible. Where the operand depends on runtime state there is no rule equivalent: the branch is not implemented, and a session that reaches it ends after the last prompt written. The HOME device's open execute, `IOF` and `IOSL` are fixed for this context by the team's answer to O-01 (section "Stubs"), so the device-open, form-feed and paging operands below have rule equivalents.

| Feature | Routine and line | Flag | Rule equivalent in inquiry.rs |
|---|---|---|---|
| `D ^%ZIS` device open, then `W:$Y>0 @IOF` | [PRCHDP1.m:12] | requires runtime shim, not converted | Fixed-context rule for the HOME device, device 267 `PMT290`, subtype 168 `PK-VTMT` [3.5+DEVICE.zwr:14-18, 679] (`pct_zis`, PO-12.b1): the open XECUTEs node `^%ZIS(2,168,2)` [ZIS4GTM.m:38] [ZIS6.m:6, 21] [ZISX.m:15, 21-30], whose literal `W:$S($D(PGM):$S(PGM="XMR":0,1:1),1:1) *27,"[5i",*27,"[6w",*27,"[4i",*27,"[?5i",*27,"[?3h",*27,"[6w"` [3.2+TERMINAL TYPE.zwr:818] writes the 26 bytes `ESC[5i ESC[6w ESC[4i ESC[?5i ESC[?3h ESC[6w` with no line terminator, and `$X` and `$Y` are 0 on return [ZIS4GTM.m:35] [ZIS1.m:25]. `W:$Y>0 @IOF`, with `IOF` `#`, then writes nothing, because `$Y` is 0 (`form_feed`, PO-10.b4) |
| `W:$Y>0 @IOF,!!…` column header | [PRCHDP2.m:42] [PRCHDP4.m:35] | requires runtime shim, not converted | `IOF` `#`, so `W @IOF` is `W #` (`form_feed`, `Session::form_feed`): the postcondition governs every argument of the WRITE, and with `$Y>0` after the halt prompt it writes `\n` to end the record in progress and the form feed, leaving `$X` and `$Y` 0; `hdr_prchdp2` writes the rest, `!!?55,"UNIT",?70,"TOTAL",!,"ITEM",?15,"DESCRIPTION",?42,"QTY",?46,"UNIT",?55,"COST",?70,"COST",!`, then 80 dashes (PO-26.b1): `0A 0C 0A 0A`, 55 spaces, `UNIT`, … The `PRCHDP4` occurrence is n/e |
| `$Y+5>IOSL` paging | [PRCHDP2.m:31, 54] | requires runtime shim, not converted | `IOSL` `60` from `STUB_CONTEXT` (`ask_prchdp2`): `$Y` counts from the item page's form feed; false writes nothing (PO-30.b1, false through the 17th item without detail lines), true ends the session after the last prompt written as the n/e PO-30.b2. The [PRCHDP2.m:54] occurrence is n/e |
| `X DIC("S")` order screen | [DIC3.m:124] [PRCHRPT.m:9] [PRCHPAT.m:81] | requires runtime shim, not converted | Fixed predicate for the literal screen `I +$P(^(0),U,1)=PRC("SITE"),$P(^(0),U,2)<10!($P(^(0),U,2)=25)`, with the naked reference on `^PRC(442,ien,0)` [DIC3.m:122] (`screen_passes`) |
| `X DIC("S")` station screen `I +Y<1000000` | [PRCFSITE.m:18] [DIC3.m:124] | requires runtime shim, not converted | Fixed predicate for the literal screen inside the `dic` stub; 500 passes |
| Identifier XECUTE: `DIC("W")` built from `^DD(file,0,"ID",…)`; for 442 it overflows 224 characters and becomes `D WOV^DICQ1`, which XECUTEs each identifier after `W "  ",$E(^PRC(442,Y,0),0)` resets the naked reference | [DIC1.m:46-60] [DICQ1.m:86-88] [DD.zwr:514202-514204] | requires runtime shim, not converted | Fixed output for the literal identifiers (R2), never by executing the code (`write_order_echo`) |
| Station identifier `^DD(411,0,"ID",.1)` with `@(…)` and `D Y^DIQ` | [DD.zwr:488724] (exported again at 488748) | requires runtime shim, not converted | Fixed station stub output: evaluated once for the fixed context into the `dic` constant `STATION_ECHO` (R1) |
| Pre-lookup node `X ^DD(411,.01,7.5)` | [DIC.m:36] [DD.zwr:488792] | requires runtime shim, not converted | Fixed station stub output: the literal `S PRCRI=$P(^PRC(411,0),U,3)` writes nothing, so the `dic` stub writes nothing for it |
| Pointer-transform screen: `P^DICM0` builds `DIC("S")` from literal index and file names and XECUTEs it inside a nested lookup; `G @Y` dispatches on the literal field type | [DICM0.m:8-41] [DICM1.m:8-11] [DICM.m:30-34] | requires runtime shim, not converted | R6 (`pointer_candidate`) and the O-03 guard of R5 step 6 (`no_match_guard`); PO-09.b11 is n/e |
| Name indirection `@(DIC_…)` | [DIC1.m:41] [DIC3.m:42, 63, 83, 109-125] | requires runtime shim, not converted | Fixed roots `^PRC(442,` [PRCHPAT.m:79] and `^PRC(411,` [PRCFSITE.m:18] |
| `%XY^%RCR` indirection `@%Z`, `$Q(@%Z)` | [DIRCR.m:29] (the Kernel copy uses `$N` [ZTRCR.m:24-27]) | requires runtime shim, not converted | A copy of the literal scratch lines: `%X` and `%Y` are literal [PRCHDP1.m:54] [PRCHDP2.m:48] (`pct_rcr`) |
| `RCR^DIWW` `\|` windows | [DIWP.m:40] | requires runtime shim, not converted | None: PO-15.b2 is n/e, and a session that reaches it ends after the last prompt written |
| `D EN^DIQ`; `D Y^DIQ` | [PRCHDP1.m:9] [PRCHUTL.m:76-77] | requires runtime shim, not converted | None: PO-10.b2 and PO-29.b3 are n/e |
| `G @DIPGM`; `G @^("LOOK")` | [DIC.m:21, 38] [DICM.m:10] | requires runtime shim, not converted | None. Not reached on the `EN3` path: the target nodes are absent from the DD (section "Workflow selection", "Dynamic targets") |
| `D @PRCHQ` | [PRCHRPT.m:43] | requires runtime shim, not converted | None. Not reached on the `EN3` path: `EN0` path only |
| `LOCK`: `L +^PRC(442.6,…)` in `NUM`, reached from `EN`, not `EN3` | [PRCHPAT.m:29] | requires runtime shim, not converted | None. Not reached on the `EN3` path |

Not hard features, so not flagged: naked references `^(…)`, throughout (for example [PRCHDP1.m:4] [PRCHDP2.m:11, 49-50] [PRCHDP1.m:61-62]), each resolved to the full global reference it denotes at that point; and `R X:DTIME`, `W !`, `W ?n`, `$C(7)`, `$X` and `$Y`, throughout, modelled in `src/mumps.rs`.

## Unexercised branches

These branches are outside equivalence: a branch no case exercises is not implemented, no capture judges it, and no verdict in `REPORT.md` covers it. A session that reaches a branch that is not implemented, whether exercised by no case or left unresolved by an open item, ends after the last prompt written. The contract function stops with the private `Halt::Unexercised` carrying the branch ID, and `inquire` returns `Ok`, with the transcript cut at that prompt by `Session::into_transcript_at_prompt` and `returned` `None`, so no output is invented after it. There is no error variant for it: `InquiryError` has only `CorruptStore`.

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`; the 411 export is `Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr`.

### Coverage counts

| Coverage | Count | Branches |
|---|---|---|
| Exercised and implemented | 47 | The 42 rows exercised before this run; PO-09.b14, exercised by GC-03 and GC-04 since the O-03 closure (R4) made its sequence source-determined for those inputs; and PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1, exercised by GC-01, GC-02, GC-05 and implemented in this refine round with the HOME device context of the O-01 answer [PRCHDP1.m:12] [PRCHDP2.m:31, 42] |
| Exercised, not implemented pending O-01 | 0 | None. Before this refine round: PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1, 43/4/61 in all |
| Not exercised, not implemented | 61 | Every other row. PO-09.b7 and PO-09.b12 are unreachable inside this group. PO-03.b2 (`DT` undefined, `NOW^%DTC` [PRCFSITE.m:7]) stays in it, because the capture start command defines `DT` (O-07) |

### Branch granularity

- **Decisions:** an `IF`, with its `ELSE`, that governs a `DO` or extrinsic call, a `GOTO`, `QUIT` or `READ`, a loop or a block; a postcondition on a `DO`, `GOTO` or `QUIT`; a loop over stored entries whose body runs a `DO`, `GOTO`, `QUIT` or block, with the outcomes "at least one entry" and "no entry".
- **Not decisions** (they belong to the enclosing branch and are implemented with it): an `IF` or postcondition governing only `SET`, `WRITE` or `KILL` commands; a `$SELECT` arm; a loop whose body only formats the entries it visits; a fixed-count loop.
- **Branch IDs:** each outcome of a decision is a branch, numbered `PO-NN.bM` in source order within its contract. PO-09.b15 and PO-09.b16 were found before this run, after the 2026-11-08 plan fixed b1 to b14, and are appended so existing IDs stay stable. An exercised outcome may be described inside its enclosing branch's row; every outcome no case exercises has its own ID, because `Halt::Unexercised` carries it, so a unit test can assert the branch that ends a session, through the private `run` or on the contract function's own result; decisions inside an n/e branch are covered by that branch's ID.
- **Coverage words:** "exercised by GC-0N", for an exercised and implemented branch; "not exercised, not implemented" (n/e). A session that reaches an n/e branch ends after the last prompt written and writes nothing for the branch. Before this refine round a third label, "exercised by GC-01, GC-02, GC-05; not implemented (O-01)", marked the four branches left unresolved while O-01 was open (conflict K9, history). "Unreachable", inside the n/e group, marks an outcome that a fixed constant on this path rules out; it needs no code. The coverage of exercised branches follows from source and the fixture profile; the captures confirm it for all five cases, which PASS. Before round two they confirmed it for GC-03 and GC-04, and GC-01, GC-02 and GC-05 agreed with it up to their line-25 difference (O-09).
- **Columns:** `Tag` is the owning `TAG^ROUTINE`, the contract's tag in the call tree of section "Original structure". `FileMan file/field` is the field the decision reads, as `file,field NAME` with its `^DD(file,field,0)` line, or `none` when the decision tests only a local, an input or a fixed constant, or the row is a single outcome with no decision; a node or file header read whole is named as such.

### Branch inventory

| Branch | Tag | Source | FileMan file/field | Outcome | Coverage |
|---|---|---|---|---|---|
| PO-01.b1 | `EN3^PRCHRPT` | [PRCHRPT.m:70-72] | none (no decision) | `D ST`, then fall into `EN30` | exercised by GC-01 to GC-05 |
| PO-02.b1 | `ST^PRCHRPT` | [PRCHRPT.m:5-7] | none (no decision) | `PRCF("X")="SP"`, `D ^PRCFSITE` | exercised by GC-01 to GC-05 |
| PO-03.b1 | `^PRCFSITE` | [PRCFSITE.m:5] | none (local `%`) | `D DUZ` leaves `%=0`: `G Q` | not exercised, not implemented |
| PO-03.b2 | `^PRCFSITE` | [PRCFSITE.m:7] | none (local `DT`) | `DT` undefined: `NOW^%DTC` | not exercised, not implemented |
| PO-03.b3 | `^PRCFSITE` | [PRCFSITE.m:8-9, 13, 17-18] | 411 file header `^PRC(411,0)` piece 4, the entry count [411 export:3] | Site file missing or empty, or holding one site so that no station prompt is written; the snapshot holds six [411 export:3] | not exercised, not implemented (the stub fixes the snapshot's site file) |
| PO-03.b4 | `^PRCFSITE` | [PRCFSITE.m:15, 21, 29] | none (local `PRC("FY")`) | `PRC("FY")` undefined: `D ^%DT` (PO-41); fiscal year and quarter computed with no output. The defined outcome is unreachable: `PRC` is undefined when `EN3` starts | exercised by GC-01 to GC-05 |
| PO-03.b5 | `^PRCFSITE` | [PRCFSITE.m:16] | none (local `PRCF("X")`) | `AFU^PRCFSI1` (flag `A`) | not exercised, not implemented (unreachable: `PRCF("X")="SP"`) |
| PO-03.b6 | `^PRCFSITE` | [PRCFSITE.m:18] | 411,.01 STATION/SUBSTATION CODE, index `B` [DD.zwr:488769] (`+Y` of the lookup) | The lookup returns station 500: `PRC("SITE")=500` | exercised by GC-01 to GC-05 |
| PO-03.b7 | `^PRCFSITE` | [PRCFSITE.m:18] | 411,.01 STATION/SUBSTATION CODE, index `B` [DD.zwr:488769] (`+Y` of the lookup) | The lookup returns no station (`+Y<0`): `G Q`. A non-empty station answer ends the session as PO-05.b2 | not exercised, not implemented |
| PO-03.b8 | `^PRCFSITE` | [PRCFSITE.m:19-25] | none (local `PRCF("X")`) | `AFU1`, fiscal-year and quarter prompts (flags `A`, `F`, `Q`) | not exercised, not implemented (unreachable: `PRCF("X")="SP"`) |
| PO-03.b9 | `^PRCFSITE` | [PRCFSITE.m:27] | none (local `PRCFASYS`) | ISMS flag (`PRCFASYS` defined) | not exercised, not implemented |
| PO-04.b1 | `DUZ^PRCFSITE` | [PRCFSITE.m:35, 37, 39] | 200,20.2 SIGNATURE BLOCK PRINTED NAME `20;2` [DD.zwr:350238] | `DUZ>0`; node 20 piece 2 present | exercised by GC-01 to GC-05 |
| PO-04.b2 | `DUZ^PRCFSITE` | [PRCFSITE.m:36] | none (local `DUZ`) | "YOU ARE NOT IN THE 'NEW PERSON' FILE" | not exercised, not implemented |
| PO-04.b3 | `DUZ^PRCFSITE` | [PRCFSITE.m:34, 38] | 200,20.2 SIGNATURE BLOCK PRINTED NAME `20;2` [DD.zwr:350238] | Node 20 piece 2 empty: `D A` writes `^VA(200,DUZ,20)` | not exercised, not implemented |
| PO-05.b1 | `^DIC` (411) | [DIC11.m:15-17, 29-32] [DIC.m:36, 48] [DIC5.m:21-31] [DIC3.m:20-31] [DIC2.m:8-32, 49-66] | 411,.01 STATION/SUBSTATION CODE, index `B` [DD.zwr:488769] | RETURN accepts the default 500, selected by the exact `B` match and echoed `      CAMP MASTER` | exercised by GC-01 to GC-05 |
| PO-05.b2 | `^DIC` (411) | [DIC11.m:17-33] | none (the answer `X`) | Any non-empty answer | not exercised, not implemented |
| PO-06.b1 | `EN30^PRCHRPT` | [PRCHRPT.m:72] | none (local `PRCHPO`) | `Q:'$D(PRCHPO)`: the lookup ended without a selection, so `EN3` returns | exercised by GC-01 to GC-05 |
| PO-06.b2 | `EN30^PRCHRPT` | [PRCHRPT.m:73-75] | none (local `PRCHPO`); the `X<10` message tests 442.3,.02 SUPPLY STATUS ORDER [DD.zwr:518035] | Selection: the bell and ` >>> Status makes this record non-specifiable here.` when `X<10`, then the display, `K` and `G EN30` | exercised by GC-01, GC-02, GC-05 |
| PO-07.b1 | `PO^PRCHRPT` | [PRCHRPT.m:9-15] | none (no decision; the MOP-8 SET tests 442,.02 METHOD OF PROCESSING `0;2` [DD.zwr:514331]) | Default screen, prompt text, `D EN3^PRCHPAT`; conditional SETs for `PRCHPC`, `PRCHDELV` and MOP 8 | exercised by GC-01 to GC-05 |
| PO-08.b1 | `EN3^PRCHPAT` | [PRCHPAT.m:79] | none (local `PRC("SITE")`) | `PRC("SITE")` undefined: quit | not exercised, not implemented |
| PO-08.b2 | `EN3^PRCHPAT` | [PRCHPAT.m:80-83] | none (`+Y` of the lookup) | Index `C` lookup; `Q:+Y<0` with no selection | exercised by GC-01 to GC-05 |
| PO-08.b3 | `EN3^PRCHPAT` | [PRCHPAT.m:83] | none (`+Y` of the lookup); then reads 442,.5 SUPPLY STATUS `7;1` [DD.zwr:514433] and 442.3,.02 SUPPLY STATUS ORDER [DD.zwr:518035] | Selection sets `PRCHPO`, `DA`, and `X` to the supply status order (`0` becomes `""`) | exercised by GC-01, GC-02, GC-05 |
| PO-09.b1 | `IX^DIC` | [DIC11.m:18] | none (the READ) | Timeout at the read | not exercised, not implemented (unreachable: the Rust READ never times out) |
| PO-09.b2 | `IX^DIC` | [DIC11.m:19-22, 24-25] | none (the answer `X`) | A character outside `.ANP`, an over-long number, or over 250 characters | not exercised, not implemented |
| PO-09.b3 | `IX^DIC` | [DIC11.m:23, 33] [DIC.m:45] | none (the answer `X`) | `^`, or any value beginning with `^`: `Y=-1`, no output | exercised by GC-01 to GC-05 |
| PO-09.b4 | `IX^DIC` | [DIC11.m:26] [DIC.m:42-44] | none (the answer `X`) | `?` help | not exercised, not implemented |
| PO-09.b5 | `IX^DIC` | [DIC11.m:27] [DIC.m:47, 49] | none (the answer `X`) | A backtick value or a single space | not exercised, not implemented |
| PO-09.b6 | `IX^DIC` | [DIC11.m:29-31] [DIC.m:45] | none (the answer `X`) | Empty input: `Y=-1`, no output | not exercised, not implemented |
| PO-09.b7 | `IX^DIC` | [DIC.m:48] [DIC5.m:21-32] | none (local `DF`) | Selection by IEN | not exercised, not implemented (unreachable: `IX^DIC` defines `DF` [DIC.m:84-85], so `BYIEN2` quits [DIC5.m:22]) |
| PO-09.b8 | `IX^DIC` | [DIC3.m:15-32, 59-71] [DIC2.m:8-16, 49-66] [DICQ1.m:86-88] | 442,.01 PURCHASE ORDER NUMBER, index `C` [DD.zwr:514312, 514318-514320]; screen on .01 and .02 METHOD OF PROCESSING [DD.zwr:514331] | After the whole search, exactly one candidate, an exact match on `C` that passes the screen; echoed and selected | exercised by GC-01, GC-02, GC-05 |
| PO-09.b9 | `IX^DIC` | [DIC3.m:34-71] | 442,.01, index `C`, and the later indexes of `^DD(442,0,"IX")` [DD.zwr:514205-514231]; screen on .01 and .02 METHOD OF PROCESSING [DD.zwr:514331] | Exactly one candidate that is a partial match or comes from a later index | not exercised, not implemented |
| PO-09.b10 | `IX^DIC` | [DIC3.m:72, 155] [DIC1.m:74-107] | 442,.01, index `C`, and the later indexes [DD.zwr:514205-514231]; screen on .01 and .02 METHOD OF PROCESSING [DD.zwr:514331] | Several candidates: the choose list | not exercised, not implemented |
| PO-09.b11 | `IX^DIC` | [DICM.m:32-34] [DICM1.m:10-11] [DICM0.m:8-60] | 442 pointer indexes `D` (5 VENDOR [DD.zwr:514589]), `F` (.02 [DD.zwr:514331]), `G` (442.14,11 [DD.zwr:517723]), `M` (46 [DD.zwr:514943]), `MCH` (61 [DD.zwr:515047]), `ST` (.5 [DD.zwr:514433]) | A candidate through a pointer transform | not exercised, not implemented |
| PO-09.b12 | `IX^DIC` | [DICM.m:33-34] [DICM1.m:13-40] | 442 indexes after `C` [DD.zwr:514219-514231], none on a date, set or variable-pointer field | A candidate through a date or set-of-codes transform on a 442 index | not exercised, not implemented (unreachable: no index of 442 after `C` is on a date, set or variable-pointer field [DD.zwr:514219-514231]) |
| PO-09.b13 | `IX^DIC` | [DICM.m:42-66] | 442,.01 PURCHASE ORDER NUMBER, index `C` [DD.zwr:514312, 514318-514320] (uppercase retry) | A candidate through the uppercase, comma or long-input retry | not exercised, not implemented |
| PO-09.b14 | `IX^DIC` | [DICM.m:71, 85-87] [DIC1.m:157-159] [DIC.m:63-65, 80-81, 25] | 442,.01 PURCHASE ORDER NUMBER, index `C` [DD.zwr:514312, 514318-514320]; the O-03 guard reads the indexes of 440, 442.5 and 442.3 [DD.zwr:509908-509917, 518127-518128, 517989-517991] and the 440 set codes of .05 and 31.5 [DD.zwr:510071, 510441] | Search exhausted, and the O-03 guard of R5 step 6 holds: output `<BEL> ??`, `\n`, `\n` and the re-asked prompt, then the lookup reads the next answer (R3). Screen-rejected entries [DIC3.m:94, 124] and pointed-to entries without a qualifying parent count as none | exercised by GC-03, GC-04 |
| PO-09.b15 | `IX^DIC` | [DIC0.m:21, 83-87] [DILIBF.m:141, 147] [DIC.m:14] [DIC1.m:41-49] | 442 file header `^PRC(442,0)` piece 2 [442 export:3] | `^PRC(442,0)` absent, empty or holding only descendants, or its piece 2 not `442I`: no prompt, or other identifiers; the session ends after the station prompt | not exercised, not implemented |
| PO-09.b16 | `IX^DIC` | [DICM.m:71-81] | 442 index subscripts after `"A["` (no field) | No `^PRC(442)` subscript collates after `"A["`: the whole file is scanned for an equal `.01`, writing `..` every 20 entries | not exercised, not implemented |
| PO-10.b1 | `^PRCHDP1` | [PRCHDP1.m:8] | 442 node 0, whole (its first field .01 PURCHASE ORDER NUMBER `0;1` [DD.zwr:514312]) | Node 0 empty: quit | not exercised, not implemented |
| PO-10.b2 | `^PRCHDP1` | [PRCHDP1.m:9] | 442 node 1, whole (its first field 5 VENDOR `1;1` [DD.zwr:514589]); 442.04 COMMENTS, node 4, field 20 [DD.zwr:514819] | Node 1 empty with comments present: `EN^DIQ` | not exercised, not implemented |
| PO-10.b3 | `^PRCHDP1` | [PRCHDP1.m:10] | 442 node 1, whole (its first field 5 VENDOR `1;1` [DD.zwr:514589]) | Node 1 empty: quit | not exercised, not implemented |
| PO-10.b4 | `^PRCHDP1` | [PRCHDP1.m:10-44] | 442 node 1, whole (its first field 5 VENDOR `1;1` [DD.zwr:514589]) | The page-1 header (ship-to, `^%ZIS`, `@IOF`, labels, vendor and ship-to rows, underscores) | exercised by GC-01, GC-02, GC-05 |
| PO-10.b5 | `^PRCHDP1` | [PRCHDP1.m:16-17] | 442.11 PARTIAL, node 11, field .6 [DD.zwr:514467] | Node 11 present: the `LAST PARTIAL RECD.` loop | not exercised, not implemented |
| PO-10.b6 | `^PRCHDP1` | [PRCHDP1.m:32-34] | 440,.2 EDI VENDOR? `3;2` [DD.zwr:510094]; 442,116 DO YOU WANT TO SEND THIS EDI? `12;16` [DD.zwr:515424] | EDI vendor (440 field .2 `Y`) and 442 field 116 not `n`: the EDI block | not exercised, not implemented |
| PO-10.b7 | `^PRCHDP1` | [PRCHDP1.m:45-52] | 442,65 INTERFACE PACKAGE PREFIX `24;3` [DD.zwr:515073] | Field 65 equal to `RMPR`: the prosthetics block | not exercised, not implemented |
| PO-11.b1 | `FTYP^PRCHFPNT` | [PRCHFPNT.m:44-45, 49, 52-53] | 442,31 SUBSTATION `23;7` [DD.zwr:514881], empty; then 411,.5 FACILITY TYPE `0;7` [DD.zwr:488809] | Facility type from site piece 7 through `^PRC(411.2)` piece 2 | exercised by GC-01, GC-02, GC-05 |
| PO-11.b2 | `FTYP^PRCHFPNT` | [PRCHFPNT.m:46-47] | 442,31 SUBSTATION `23;7` [DD.zwr:514881] | Substation, node 23 piece 7 | not exercised, not implemented |
| PO-12.b1 | `^%ZIS` | [ZIS.m:28-37] [ZIS1.m:19, 25, 103-113] [ZIS4GTM.m:35, 38] [ZIS6.m:6, 21] | none (the HOME device's 3.5 and 3.2 entries, fixed by the O-01 answer: device 267, subtype 168 [3.5+DEVICE.zwr:14-18, 679] [3.2+TERMINAL TYPE.zwr:816-818]) | `IOP="HOME"` device open: the open execute's 26 bytes, `$X` and `$Y` 0 | exercised by GC-01, GC-02, GC-05 |
| PO-13.b1 | `FOB^PRCHDP1` | [PRCHDP1.m:53-57] | none (no decision) | FOB, proposal wrap at width 16, authority loop, cost center, type, buyer or agent label | exercised by GC-01, GC-02, GC-05 |
| PO-14.b1 | `DIWP^PRCUTL` | [PRCUTL.m:33-35] | none (no decision) | `D ^DIWP` with `DA` saved | exercised by GC-01, GC-02, GC-05 |
| PO-15.b1 | `^DIWP` | [DIWP.m:28-77] | none (locals `X`, `DIWF`) | Plain text, `DIWF` empty, no pipe character (`$C(124)`) and no `TAB` | exercised by GC-01, GC-02, GC-05 |
| PO-15.b2 | `^DIWP` | [DIWP.m:40] | none (local `X`) | Pipe (`$C(124)`) window evaluation (`RCR^DIWW`) | not exercised, not implemented |
| PO-15.b3 | `^DIWP` | [DIWP.m:30-38, 47, 55-57] | none (locals `X`, `DIWF`) | Other `DIWF` letters and `TAB` | not exercised, not implemented |
| PO-16.b1 | `%XY^%RCR` | [DIRCR.m:24-30] | none (no decision) | Copy of the `^UTILITY($J,"W")` subtree to `^TMP($J,"W")` | exercised by GC-01, GC-02, GC-05 |
| PO-17.b1 | `TY^PRCHDP1` | [PRCHDP1.m:71-72] | none (no decision) | Order type from the source code through `$SELECT` | exercised by GC-01, GC-02, GC-05 |
| PO-18.b1 | `DIS^PRCHDP1` | [PRCHDP1.m:58-60] | 442.06 PROMPT PAYMENT TERMS, node 5, field 9.2 [DD.zwr:514680] | Delivery date; buyer or agent name; no node 5 | exercised by GC-01, GC-02, GC-05 |
| PO-18.b2 | `DIS^PRCHDP1` | [PRCHDP1.m:60] | 442.06 PROMPT PAYMENT TERMS, node 5, field 9.2 [DD.zwr:514680] | Node 5 present: discount terms | not exercised, not implemented |
| PO-19.b1 | `DT^PRCHDP1` | [PRCHDP1.m:69-70] | none (local `Y`) | `M/D/YYYY` when `Y` is non-zero (a conditional WRITE) | exercised by GC-01, GC-02, GC-05 |
| PO-20.b1 | `CON^PRCHDP1` | [PRCHDP1.m:61-62] | 442.01,4 CONTRACT/BOA #, index `AC` [DD.zwr:516132, 516134] | A contract number in the item index `AC`: its count and number on the row | not exercised, not implemented |
| PO-20.b2 | `CON^PRCHDP1` | [PRCHDP1.m:61, 63-64] | 442.01,4 CONTRACT/BOA #, index `AC` [DD.zwr:516132, 516134] | No further contract number, in each of the three rows; then dashes and `G ^PRCHDP2` | exercised by GC-01, GC-02, GC-05 |
| PO-21.b1 | `APP^PRCHDP1` | [PRCHDP1.m:65] | none (no decision) | `APP: ` from fields 1.4 and 1 | exercised by GC-01, GC-02, GC-05 |
| PO-22.b1 | `DAT^PRCHDP1` | [PRCHDP1.m:66] | none (no decision) | `DATE: ` from field .1 through `DT` | exercised by GC-01, GC-02, GC-05 |
| PO-23.b1 | `EST^PRCHDP1` | [PRCHDP1.m:68] | 442,.08 ESTIMATED ORDER? `7;3` [DD.zwr:514379] | Field .08 not `Y`: quit, nothing written | exercised by GC-01, GC-02, GC-05 |
| PO-23.b2 | `EST^PRCHDP1` | [PRCHDP1.m:68] | 442,.08 ESTIMATED ORDER? `7;3` [DD.zwr:514379] | Field .08 equal to `Y`: `ESTIMATED` | not exercised, not implemented |
| PO-24.b1 | `TOT^PRCHDP1` | [PRCHDP1.m:67] | none (no decision) | `TOTAL: ` as `$J(field 91,8,2)` | exercised by GC-01, GC-02, GC-05 |
| PO-25.b1 | `^PRCHDP2` | [PRCHDP2.m:10] | none (the answer `X`) | An answer containing `^` at `ENTER '^' TO HALT`: `G ASK2` | not exercised, not implemented |
| PO-25.b2 | `^PRCHDP2` | [PRCHDP2.m:10-11] | none (the answer `X`) | Otherwise: `D HDR`, then `ITEM` once per item | exercised by GC-01, GC-02, GC-05 |
| PO-25.b3 | `^PRCHDP2` | [PRCHDP2.m:11] | 442.01 ITEM, node 2, field 40 [DD.zwr:514888] | No line item | not exercised, not implemented |
| PO-25.b4 | `^PRCHDP2` | [PRCHDP2.m:11] | none (local `PRCHDQ`) | `PRCHDQ` set after an item, reachable only through PO-30.b2: `G ASK2` | not exercised, not implemented |
| PO-25.b5 | `^PRCHDP2` | [PRCHDP2.m:12, 67] | 442.03 DISCOUNT, node 3, field 14 [DD.zwr:514730] | Discount entries in node 3: `DIS^PRCHDP2` | not exercised, not implemented |
| PO-25.b6 | `^PRCHDP2` | [PRCHDP2.m:13-14] | 442.15 ADMINISTRATIVE CERTIFICATIONS, node 15, field 24 [DD.zwr:514843]; the conditional WRITE tests 442,13 EST. SHIPPING AND/OR HANDLING `0;13` [DD.zwr:514683] | The estimated-shipping line when field 13 > 0 (a conditional WRITE); no node 15: `G COM` | exercised by GC-01, GC-02, GC-05 |
| PO-25.b7 | `^PRCHDP2` | [PRCHDP2.m:14-18] | 442.15 ADMINISTRATIVE CERTIFICATIONS, node 15, field 24 [DD.zwr:514843] | Certifications in node 15 | not exercised, not implemented |
| PO-26.b1 | `HDR^PRCHDP2` | [PRCHDP2.m:42] | none (`$Y`, and `IOF` `#` of the O-01 answer) | `W:$Y>0 @IOF,…` column header, then dashes | exercised by GC-01, GC-02, GC-05 |
| PO-27.b1 | `ITEM^PRCHDP2` | [PRCHDP2.m:44-53, 55-66] | 442.01,1 DESCRIPTION [DD.zwr:516024]; 442.01,20 DATE RECEIVED, item node 3 [DD.zwr:516311] | A regular item and its detail lines | exercised by GC-01, GC-02, GC-05 |
| PO-27.b2 | `ITEM^PRCHDP2` | [PRCHDP2.m:47] | 442.01,1 DESCRIPTION, subfile 442.05 [DD.zwr:516024] | An item with no description line | not exercised, not implemented |
| PO-27.b3 | `ITEM^PRCHDP2` | [PRCHDP2.m:54] | none (local `PRCHDCNT`, the wrapped line count of 442.01,1 [DD.zwr:516024]) | Continuation description lines (`PRCHDCNT>1`) | not exercised, not implemented |
| PO-27.b4 | `ITEM^PRCHDP2` | [PRCHDP2.m:55] | none (local `PRCHDQ`) | `PRCHDQ` set, reachable only through PO-30.b2: quit | not exercised, not implemented |
| PO-27.b5 | `ITEM^PRCHDP2` | [PRCHDP2.m:57] | 442.01,20 DATE RECEIVED, item node 3, index `AC` [DD.zwr:516311] | Partial receipts in item node 3 | not exercised, not implemented |
| PO-28.b1 | `PIPECK^PRCHDP2` | [PRCHDP2.m:96-102] | 442.05,.01 DESCRIPTION text [DD.zwr:516742] | Every item's description lines scanned; none contains the pipe-IN command, `$C(124)_" IN "` [PRCHDP2.m:100] | exercised by GC-01, GC-02, GC-05 |
| PO-28.b2 | `PIPECK^PRCHDP2` | [PRCHDP2.m:98-100] | 442.05,.01 DESCRIPTION text [DD.zwr:516742] | One does: `Q:PURPIPE` | not exercised, not implemented |
| PO-28.b3 | `PIPECK^PRCHDP2` | [PRCHDP2.m:99] | 442.01,1 DESCRIPTION [DD.zwr:516024] | An item with no description line | not exercised, not implemented |
| PO-29.b1 | `EDISTAT^PRCHUTL` | [PRCHUTL.m:73] | 442.01 item node 2, whole (its first field 15 TOTAL COST `2;1` [DD.zwr:516287]) | Item node 2 absent | not exercised, not implemented |
| PO-29.b2 | `EDISTAT^PRCHUTL` | [PRCHUTL.m:73-74, 80] | 442.01,12 EDI STATUS CODE 1 `2;9` [DD.zwr:516265]; 442.01,13 EDI STATUS CODE 2 `2;11` [DD.zwr:516276] | Node 2 present, EDI status codes 1 and 2 empty: quit | exercised by GC-01, GC-02, GC-05 |
| PO-29.b3 | `EDISTAT^PRCHUTL` | [PRCHUTL.m:75-78] | 442.01,12 EDI STATUS CODE 1 `2;9` [DD.zwr:516265]; 442.01,13 EDI STATUS CODE 2 `2;11` [DD.zwr:516276] | EDI status lines through `Y^DIQ` | not exercised, not implemented |
| PO-30.b1 | `ASK^PRCHDP2` | [PRCHDP2.m:31] | none (`$Y`, and `IOSL` `60` of the O-01 answer) | `$Y+5'>IOSL`: no prompt | exercised by GC-01, GC-02, GC-05 |
| PO-30.b2 | `ASK^PRCHDP2` | [PRCHDP2.m:31] | none (`$Y`, and `IOSL` `60` of the O-01 answer) | Paging prompt and `HDR` | not exercised, not implemented |
| PO-31.b1 | `COM^PRCHDP2` | [PRCHDP2.m:19] | 442.04 COMMENTS, node 4, field 20 [DD.zwr:514819] | No node 4: `G PT` | exercised by GC-01, GC-02, GC-05 |
| PO-31.b2 | `COM^PRCHDP2` | [PRCHDP2.m:19-21] | 442.04 COMMENTS, node 4, field 20 [DD.zwr:514819] | Comments | not exercised, not implemented |
| PO-32.b1 | `PT^PRCHDP2` | [PRCHDP2.m:22-25] | 442.14 2237 REFERENCE NUMBER, node 13, field .05 [DD.zwr:514364]; 442.07 AMENDMENT, node 6, field 50 [DD.zwr:514981] | No node 13; `D AMENDS`; no node 6; `K ^TMP($J,"PRCHDP6")` | exercised by GC-01, GC-02, GC-05 |
| PO-32.b2 | `PT^PRCHDP2` | [PRCHDP2.m:22] | 442.14 2237 REFERENCE NUMBER, node 13, field .05 [DD.zwr:514364] | Transaction numbers | not exercised, not implemented |
| PO-32.b3 | `PT^PRCHDP2` | [PRCHDP2.m:24] | 442.07 AMENDMENT, node 6, field 50 [DD.zwr:514981] | Amendment lines | not exercised, not implemented |
| PO-33.b1 | `AMENDS^PRCHDP6` | [PRCHDP6.m:127] | 442.07 AMENDMENT, node 6, field 50 [DD.zwr:514981] | No node 6: quit | exercised by GC-01, GC-02, GC-05 |
| PO-33.b2 | `AMENDS^PRCHDP6` | [PRCHDP6.m:128-152] | 442.07 AMENDMENT, node 6, field 50 [DD.zwr:514981] | Amendment history | not exercised, not implemented |
| PO-34.b1 | `ASK2^PRCHDP2` | [PRCHDP2.m:26] | 442.11 PARTIAL, node 11, field .6 [DD.zwr:514467]; local `PRCHDQ` | `PRCHDQ` clear: `EN^PRCHDP4`; no node 11: `G ASK1` | exercised by GC-01, GC-02, GC-05 |
| PO-34.b2 | `ASK2^PRCHDP2` | [PRCHDP2.m:26-30] | 442.11 PARTIAL, node 11, field .6 [DD.zwr:514467] | Receiving-report review | not exercised, not implemented |
| PO-34.b3 | `ASK2^PRCHDP2` | [PRCHDP2.m:26] | none (local `PRCHDQ`) | `PRCHDQ` set: `EN^PRCHDP4` skipped | not exercised, not implemented |
| PO-35.b1 | `EN^PRCHDP4` | [PRCHDP4.m:4-5] | none (no decision) | Sets `PRCHONL`, `PRCHQUIT` and falls into `EN2` | exercised by GC-01, GC-02, GC-05 |
| PO-36.b1 | `EN2^PRCHDP4` | [PRCHDP4.m:8] | none (local `D0`) | `D0` undefined: quit | not exercised, not implemented (unreachable: `D0` is set at [PRCHRPT.m:74]) |
| PO-36.b2 | `EN2^PRCHDP4` | [PRCHDP4.m:8] | 442,.01 PURCHASE ORDER NUMBER `0;1` [DD.zwr:514312] | Empty PO number: `G EXDELV` | not exercised, not implemented |
| PO-36.b3 | `EN2^PRCHDP4` | [PRCHDP4.m:9-11, 16] | 442.8,.01 ORDER NUMBER (PAT), index `B` [DD.zwr:518518] | Delivery-schedule entries in `^PRC(442.8,"B",PO)`: `BLD`, then print | not exercised, not implemented |
| PO-36.b4 | `EN2^PRCHDP4` | [PRCHDP4.m:8-10] | 442.8,.01 ORDER NUMBER (PAT), index `B` [DD.zwr:518518] | No `B` entry, so no `^TMP($J,"MDEL")`: `G EXDELV` | exercised by GC-01, GC-02, GC-05 |
| PO-37.b1 | `EXDELV^PRCHDP4` | [PRCHDP4.m:13-14] | none (no decision) | `W !`; kills | exercised by GC-01, GC-02, GC-05 |
| PO-38.b1 | `ASK1^PRCHDP2` | [PRCHDP2.m:33-40] | none (local `PRCHAMNT`) | `PRCHAMNT=2`: the amendment warning | not exercised, not implemented |
| PO-38.b2 | `ASK1^PRCHDP2` | [PRCHDP2.m:41] | none (local `PRCHDQ`) | `PRCHDQ` set: `G Q` | not exercised, not implemented |
| PO-38.b3 | `ASK1^PRCHDP2` | [PRCHDP2.m:41] | none (local `PRCHDQ`) | `END OF DISPLAY` read, then `G Q` | exercised by GC-01, GC-02, GC-05 |
| PO-39.b1 | `Q^PRCHDP2` | [PRCHDP2.m:94-95] | none (no decision) | Kills locals and scratch; no output | exercised by GC-01, GC-02, GC-05 |
| PO-40.b1 | `K^PRCHRPT` | [PRCHRPT.m:106-108] | none (no decision) | Kills locals and returns | exercised by GC-01, GC-02, GC-05 |
| PO-41.b1 | `^%DT` | [DIDT.m:11-24] | none (local `%DT`) | `%DT` has no `A` or `E` flag: no prompt, no output; `Y` gets today's date, which no output uses | exercised by GC-01 to GC-05 |

### Tags without a contract

Reachable only through an n/e branch, so given no contract and no Rust function. IFCAP: `PT1`, `ADJCHK`, `AMD`, `DIS` and `DT^PRCHDP2`; `^PRCHDP3`; `^PRCFYN`; `START^PRCHDP5`; `BLD`, `PRT1`, `PRT2`, `H` and `PAUSE^PRCHDP4`; `A` and `HELP1^PRCFSITE`; `AFU` and `AFU1^PRCFSI1`; `$$ISMSFLAG^PRCPUX2`. Kernel and FileMan: `NOW^%DTC`; `DD^%DT`; `EN^DIQ`; `Y^DIQ`; `DT^DIQ`; `RCR^DIWW`; FileMan help, list and LAYGO code (`DSPHLP^DICQ`, `Y^DIC1`, `L^DICM`).

### Exhausted searches versus unimplemented outcomes

In PO-09.b11, b12 and b13 an alternative route finds a candidate. When no route does, the search continues and ends in PO-09.b14, never in the branch of a route that found nothing. Routes that find nothing: a numeric value, which is never tried as an IEN [DIC5.m:22]; a lowercase value whose uppercase form matches nothing [DICM.m:44-47]; candidates that all fail the screen [DIC3.m:124]; pointed-to entries without a qualifying parent [DICM0.m:25-37], which the nested lookup rejects without output of its own [DICM.m:107]. Whether an exhausted search writes the no-match sequence is decided by the O-03 guard alone (R5 step 6): a canonical number or a lowercase value fails its condition (d), and a value that a nested index subscript equals or begins with fails (c), so each of those ends the session as the n/e PO-09.b11; screen-rejected candidates pass it (`candidates_all_rejected_by_screen_follow_no_match_path`). No identifier is treated specially.

### Fixture profile

The coverage of the exercised branches assumes the fixture profile stated in `tests/golden/ifcap/README.md` Part B step 1 and in section "Source corrections and interface departures". It includes the O-03 conditions: fields 46 PURCHASE CARD NUMBER [DD.zwr:514943] and 61 PURCHASE CARD HOLDER [DD.zwr:515047] are empty and no item carries 442.14 field 11 INVENTORY/DISTRIBUTION POINT [DD.zwr:517723], so indexes `M`, `MCH` and `G` hold no entry; and no subscript of any index of 440, 442.5 or 442.3 equals or begins with `GC3003` or `INVALID!`, which the repository fixtures satisfy (`no_match_guard_holds_for_case_values_over_repository_fixtures`). A capture whose records leave that profile reaches an n/e branch, where the Rust session ends; that case is FAIL and its contract reopens.

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
- **The label `M.O.P.` reads a file named `PAT TYPE`.** `W !,"M.O.P.: ",$P($G(^PRCD(442.5,+$P(PRCHD0,U,2),0)),U,1)` [PRCHDP1.m:16] reads file 442.5, whose header is `^PRCD(442.5,0)="PAT TYPE^442.5^26^13"` [442.5+PAT TYPE.zwr:3]. Reproduced in `prchdp1`, which runs past PO-12.b1 for a found order (GC-01, GC-02, GC-05).
- **`^DD(411,0,"ID",.1)` is exported twice,** with identical text [DD.zwr:488724, 488748]. Recorded. The identifier is evaluated once, into the R1 constant.
- **Two index nodes of file 442 point to a record that does not exist.** `^PRC(442,"AH",31,99999949)=""` and `^PRC(442,"AI",31,99999949)=""` [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:4-5] sit under a header that counts 0 entries [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3], and the export has no `^PRC(442,99999949,…)` node. Recorded. The 442 export is not a fixture.
- **The documented `WRITE #` differs from what the captures show.** The column header `HDR W:$Y>0 @IOF,!!?55,"UNIT",…` [PRCHDP2.m:42], with `IOF` `#`, is `W #,!!…`. The YottaDB Programmer's Guide, "Input/Output Processing", gives `#` as `<FF><LF>` only for sequential files (SD): its READ/WRITE Operations table, which covers STREAM, VARIABLE and FIXED format sequential files, gives "Write <FF>,<LF>, set $X to 0, increment $Y" for STREAM and VARIABLE format, and the FFLF deviceparameter, "Applies to: SD", has `<FF><LF>` as its default. This is the reading this run's plan took; neither citation covers a terminal device. In GC-01, GC-02 and GC-05, after `        ENTER '^' TO HALT: ` is read [PRCHDP2.m:10], the bytes are `0A 0C 0A 0A`, then 55 spaces and `UNIT`: the `!!` gives the last two `0A`, so `W #` contributes `0A 0C`, a line feed ending the record in progress and then the form feed, with no line feed after it. The same Guide's "Maintenance of $X and $Y" says `#` "Sets $X and $Y to zero (0), and terminates the logical record in progress", which agrees with the captures. The captures judge: `Session::form_feed` writes `\n` when `$X>0`, then `\f`, and leaves `$X` and `$Y` 0. At PRCHDP1.m:12 `W:$Y>0 @IOF` writes nothing, because `$Y` is 0 after the device open [ZIS4GTM.m:35] [ZIS1.m:25].

## Existing items outside the layout

Each existing item outside the request's layout is kept only when the layout or the interface names it, a kept item needs it, or it reproduces a source-cited rule on the five cases' path; otherwise it is removed.

| Item | Disposition | Reason |
|---|---|---|
| `test-results/` (33 run folders from `20261108T035854Z` to `20261109T025920Z`, 1002 files) | Deleted | Not in the layout and not required. The evidence the run needs moves into "Run evidence" |
| `tests/fixtures/ifcap/MANIFEST.txt` (11 sha256 lines) | Deleted | Not in the layout (`tests/fixtures/ifcap/*.zwr` only). Fixture-to-source equality is recorded once in "Run evidence" |
| `Cargo.lock` | Kept | `cargo build --offline --locked` requires an existing, current lock file |
| `blitzy/documentation/Project Guide - d3cd5e06-5ae1-42c7-8b11-7bb911fc681f.md` (repository root, outside the crate) | Untouched | Outside the crate and the request |
| `tests/golden/ifcap/state/`: `MANIFEST.txt`, `state.zwr` (sha256 `20ebce847e6359d430f01ee794572618cfe28a524509b6266bcb0485d28ee7ae`), `export.stderr` and `verify.stderr` | Kept, by instruction of this refine round, which overrides the legacy disposition rule above for this folder | The team's evidence of the frozen starting data. No code reads it, and `state` is not a case id. Its four files are in the capture listings of section "Capture hashes" and under their equality rule. The team also names `save_state_report.txt`, which is not in the checkout: the crate `.gitignore` excludes `*_report.txt`, so the file was never committed |

## Open items

| ID | Open item | Evidence | Effect and current status |
|---|---|---|---|
| O-01 | HOME device context: the bytes `^%ZIS` writes for `IOP="HOME"`, the `IOF` value and the bytes `W @IOF` emits, `IOSL`, and `$Y` after a form feed | [PRCHDP1.m:12] [PRCHDP2.m:31, 42] [ZIS.m:28-38, 46, 58-81] [ZIS1.m:103-113] [3.5+DEVICE.zwr:14-18, 679] [3.2+TERMINAL TYPE.zwr:816-817, 1153] | **CLOSED by the team's answer, recorded in each capture's `meta.txt` `context` `{"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}` and `setup` (HOME device `$I` `0`, `IOM` 132, `IOSL` 60).** The answer is an input to the port, not a rule derived from golden content. It selects device 267 `PMT290` (`$I` `"0"`, key `SYS..0`), subtype 168 `PK-VTMT`, whose node 1 is `132^#^60` [3.5+DEVICE.zwr:14-18, 679] [3.2+TERMINAL TYPE.zwr:816-817]. `IOF` `#` is a form feed, not cursor addressing, so the stop condition of section "Workflow selection" does not apply. `STUB_CONTEXT` holds `IOF` `#` and `IOSL` `60` (section "Stubs"), and PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1 are implemented: `pct_zis` writes the open execute `^%ZIS(2,168,2)` [3.2+TERMINAL TYPE.zwr:818], 26 bytes, and leaves `$X` and `$Y` 0 [ZIS1.m:25]; `W @IOF` is `W #`; `$Y+5>IOSL` is decided with 60. The bytes of `W #` follow the captures, where the YottaDB documentation differs (section "Places the original looks wrong"). Before the answer this item read: OPEN, narrowed; the candidates disagreed (device 267 `132^#^60`, device 595 `CONSOLE` `80^#,$C(27,91,50,74,27,91,72)^24` [3.2+TERMINAL TYPE.zwr:1153]); the team was asked which `IOF` and `IOSL` `^%ZIS` sets at PRCHDP1.m:12 under the capture start command |
| O-02 | Exact FileMan echo bytes for station default acceptance, the unique order match, and the no-match sequence | [DIC.m:25, 63-65, 80-81] [DIC2.m:8-98] [DIC5.m:21-31] [DIC1.m:46-60] [DICQ1.m:86-88] [DD.zwr:488724, 514202-514204] | **Closed by R1, R2 and R3** of section "Resolution decisions binding src/inquiry.rs"; PO-03, PO-05.b1, PO-09.b8 and PO-09.b14 are SOURCE-RESOLVED. A capture that disagrees reopens it |
| O-03 | Transform behaviour for the GC-03 and GC-04 values | [DICM.m:10, 13-36, 107] [DICM1.m:13-40] [DICM0.m:8-60] [DICF2.m:110-127] [DIDT.m:8-25] | **CLOSED for the five cases' values, under the fixture profile (R4).** A nested pointer lookup runs only for a 442 pointer index that holds entries [DICM0.m:26, 32]. With `G`, `M` and `MCH` empty, only 440 (`D`, field 5 [DD.zwr:514589]), 442.5 (`F`, .02 [DD.zwr:514331]) and 442.3 (`ST`, .5 [DD.zwr:514433]) are visited. None of the three has a date, pointer or variable-pointer index, a node-7 field or a new-style index [DD.zwr:509908-509917, 517989-517991, 518127-518128] [0.11+INDEX.zwr], so no `%DT` echo can occur. The only set-typed indexes are 440 `AC` (.05, `S:SUPPLY WAREHOUSE` [DD.zwr:510071]) and `AE` (31.5, `1:INACTIVATED` [DD.zwr:510441]); `S^DICM1` matches an internal code or an external prefix [DICM1.m:30-39], and neither `GC3003` nor `INVALID!` does either. Nested misses write nothing [DICM.m:107]. `ix_dic` writes PO-09.b14 only under the guard of R5 step 6, which checks these conditions; otherwise the session ends as the n/e PO-09.b11 |
| O-04 | Which `%RCR` and `%DT` are installed at runtime: `DIRCR.m` and `DIDT.m` (FileMan 22.2) or `ZTRCR.m` and `ZTDT.m` (FileMan 19.0 copies under Kernel) | [DIRCR.m:1-3, 24-30] [ZTRCR.m:1-3, 21-27] [DIDT.m:1-2, 11-12] [ZTDT.m:1-4] | OPEN, not blocking: each pair behaves the same on this path (subtree copy; no output without `A` or `E`) |
| O-05 | Image digest, and whether the runtime database is commit `dd3fd0f3…` (`snapshot_id`) | `meta.txt` (team) | **CLOSED.** All five `meta.txt` record `image_digest=worldvista/vehu@sha256:e0710df4ab487831ad84d0383637488da2baa37168ce8ac8e71e8cd039590622` and `snapshot_id=sha256:a8089ad06fcca92d40659c2a1503cd6f4ef829f407df547d939275e6892bace4`, the same in each; `REPORT.md` part 2 shows both per case. Every `db-before.zwr` and `db-after.zwr` has sha256 `c229942c5a7b9f817ab06bfee1eb0f96bbc63d87e4be48478ccc62f249f5c476` (section "Capture hashes") |
| O-06 | How live mode obtains the original's return value | [19+OPTION.zwr:14691] [PRCHRPT.m:70-75] | **CLOSED.** `EN3^PRCHRPT` is entered by `DO` [19+OPTION.zwr:14691] and quits without a value [PRCHRPT.m:70-75], so `returned` is `None` on both sides and `returned.txt` is zero bytes; live mode needs no channel for a return value |
| O-07 | The capture start command and `docker_exec_args`: the context set-up (`U`, `DTIME`, `DT` through `$$DT^XLFDT`, `DUZ` through `DUZ^XUP(.5)`; the HOME device is O-01) | [XLFDT.m:159] [XUP.m:54-64] [PRCFSITE.m:7] | **OPEN.** `DUZ^XUP` sets `DUZ` and its parts but not `DT`, `DTIME` or `IO*` [XUP.m:54-64]; defining `DT` keeps `^PRCFSITE` off `NOW^%DTC`, PO-03.b2 [PRCFSITE.m:7]. The README's capture command, run from inside the case folder in the team's shell and unverified, is `docker exec -i vehu su - vehu -c 'source /home/vehu/etc/env && yottadb -run %XCMD "S U=\$C(94),DTIME=300,DT=\$\$DT^XLFDT D DUZ^XUP(.5) D EN3^PRCHRPT"' < input.txt > transcript.txt`. `meta.txt` `docker_exec_args` is split on single spaces and cannot hold its `-c` argument (conflict K3). **Question for the team:** supply a start command whose arguments contain no space, or change the convention. **Live mode is delivered unverified; replay is the delivered MUMPS backend**. The 10 s live timeout kills only the `docker` process the bridge started, because stable std has no process-group signal and the crate forbids unsafe code, takes no dependency and runs one kind of command: a process `docker` started is not signalled, can outlive the run outside the 4 live slots, and one holding `docker`'s standard output or error makes the run `TIMEOUT` even when `docker` exited 0; whether the in-container command stops when `docker exec` is killed is unverified with the rest of live mode. **Information from the team (this refine round):** `su - vehu -c "source /home/vehu/etc/env && yottadb -run %XCMD ..."` produced all five captures; each `meta.txt` records it as `docker_exec_args`. The single-space convention, K3 and the live-mode status above are unchanged |
| O-08 | Pinned toolchain presence on the build host | Planning host: `rustc: command not found` | **CLOSED for this run's host.** Rust 1.99.1 and the C linker `cc` are verified: section "Run evidence", "Toolchain" |
| O-09 | GC-01, GC-02 and GC-05: node `^VA(200,.5,0)`, read by `DIS^PRCHDP1` for the agent name, was in neither `cases.zwr` nor a supporting fixture before round two | `S Y=+$P(PRCHD1,U,10),Y=$P($G(^VA(200,Y,0)),U,1) W ?57,"\|  ",$P(Y,",",2)," ",$P(Y,",",1)` [PRCHDP1.m:59]; each order's node 1 piece 10 (field 16 BUYER [DD.zwr:514742]) is `.5`; `^VA(200,.5,0)="POSTMASTER^.5^…"` is in every `db-before.zwr` and in [200+NEW PERSON.zwr:4]; `tests/equivalence.rs` loads only `tests/fixtures/ifcap/`, which since round two holds `200.zwr` (commit `a78321486`) | **CLOSED in round two.** The buyer node `^VA(200,.5,0)`, read at [PRCHDP1.m:59], is now in `tests/fixtures/ifcap/200.zwr`, copied by the team from the captures' `db-before.zwr`: the two ZWR header lines and that one node. It is a supporting fixture beside `cases.zwr`: `load_fixtures` loads it with the other supporting fixtures, before `cases.zwr`, and README Part B step 6 still builds `cases.zwr` only. **Comparison: verbatim.** The committed node is byte-identical to the node in `GC-01/db-before.zwr`, and does not differ in an empty piece 3 or anywhere else: `grep -F '^VA(200,.5,0)=' tests/fixtures/ifcap/200.zwr` and `grep -F '^VA(200,.5,0)=' tests/golden/ifcap/GC-01/db-before.zwr` each print `^VA(200,.5,0)="POSTMASTER^.5^K!GM58!:4:Z5JAwss^@^^^^1^26^^^^^^^.5"`, and `cmp <(grep -F '^VA(200,.5,0)=' tests/fixtures/ifcap/200.zwr) <(grep -F '^VA(200,.5,0)=' tests/golden/ifcap/GC-01/db-before.zwr); echo $?` prints `0`, as it does for the other four `db-before.zwr`. The same line is [200+NEW PERSON.zwr:4]. Piece 3 of the value, field 2 ACCESS CODE `0;3` [DD.zwr:349726], is `K!GM58!:4:Z5JAwss` in both files, not empty (`grep -F '^VA(200,.5,0)=' <file> \| sed 's/^[^=]*="//; s/"$//' \| cut -d^ -f3`; `cut` on the raw line would count the global's own leading `^` as a delimiter). No code reads piece 3: `dis_prchdp1` takes piece 1, field .01 NAME [DD.zwr:349371], as [PRCHDP1.m:59] does, and the only other read of file 200, the `MCH` route of the pointer-candidate rule (R6), tests only that an entry's zero node exists, and only when `^PRC(442,"MCH")` holds an entry, which no fixture has. The fixture is not edited. GC-01, GC-02 and GC-05 now write `\|   POSTMASTER` at column 57 of transcript line 25 and PASS, with GC-03 and GC-04: five PASS at round two's base commit `64482eef8` (section "Run evidence"). Before round two this item read: OPEN, blocking GC-01, GC-02 and GC-05. The Rust side read no value through `$G` and wrote `\|   `, so the three cases FAILed at line 25 and at no other line (section "Run evidence", "Round one: correction cycle"). README Part B step 6 extracts only `^PRC(442)` and the vendor's `^PRC(440)` nodes, so `cases.zwr` omits the node, and round one's preflight, which applies that step, found no missing line. No source line supported a code change, and round one froze `tests/fixtures/ifcap/`, so the team was asked to extend step 6 or add a supporting fixture holding the node |
| O-10 | Process exit after normal completion: the `\n` that ends every capture after the final `P.O./REQ.NO.: ` | The five transcripts end `P.O./REQ.NO.: \n` (GC-03 and GC-04 101 bytes, GC-01 1664, GC-02 1852, GC-05 1745); YottaDB documents that CLOSE completes a partial record with `WRITE !` and that it ignores CLOSE of `$PRINCIPAL` (Programmer's Guide, "Input/Output Processing", CLOSE; the READ/WRITE Operations table: "After a WRITE, if $X > 0, Write <LF>"), and states no such rule for HALT or process exit | **OPEN, asking the team to confirm.** The port implements the rule as the refine round states it (K14): when `EN3` returns after `^` at the final order prompt, `run` writes `\n` if `$X>0`; a session cut at `Halt::InputExhausted` or `Halt::Unexercised` gets none. The rule keys on `$X`, never on a transcript. **Question for the team:** confirm that the process exit of `yottadb -run %XCMD` on the piped principal device completes the record in progress as the documented CLOSE does |
| O-11 | Before round two, one Rust session's work and transcript were bounded only by the bridge's request admission: each repetition of a found order's inputs ran one more full display, and the session kept every byte it wrote (CWE-400) | History, at commit `64482eef8`, before the limit: `EN30 D PO Q:'$D(PRCHPO)` … `S D0=PRCHPO D ^PRCHDP1,K`, `G EN30` [PRCHRPT.m:72-75] is the `loop` of `en30_prchrpt` [src/inquiry.rs:592-609]; with PO-12.b1 implemented, `pct_zis` returns `Ok` [src/inquiry.rs:1637-1645], so each pass runs `prchdp1` [src/inquiry.rs:604] through both display pauses and back to the order prompt, and `Session::write` and `nl` append to one `String` with no budget [src/mumps.rs:610-620]. The bridge admits a body of up to `MAX_BODY`, 1 MiB [src/bridge.rs:71], and up to `MAX_HELD_INPUTS`, 524,288 decoded inputs across the sessions with more than `FREE_INPUTS` 1024 [src/bridge.rs:102, 109]; `REQUEST_TIME`, 5 s [src/bridge.rs:67], bounds reading the request only [src/bridge.rs:497], and `rust_session` runs `inquire` to its end with no deadline or cancellation [src/bridge.rs:1933-1941]. Measured on the replay bridge (2026-11-17), `POST /api/rust/session` with inputs `""`, N × `"GC1001","",""`, `"^"` (a 19+15N-byte body; N=1 is GC-01's `input.txt`): N=1, 200, 1,766 bytes in 0.009 s, 1 display; N=100, 200, 164,621 bytes in 0.17 s, 100 displays; N=1000, 200, 1,645,121 bytes in 1.59 s, 1,000 displays. Output and time grow linearly, about 1.6 KB and 1.6 ms per repetition; the largest admitted body, N=69,903 (1,048,564 bytes, 209,711 inputs), extends that to about 115 MB and 111 s for one request (linear extension, not run) | **CLOSED in round two, for CWE-400 (Uncontrolled Resource Consumption).** `SESSION_OUTPUT_LIMIT`, 1,048,576 bytes, the size of the bridge's live-mode `LIVE_OUTPUT_LIMIT` [src/bridge.rs:116], bounds one Rust session's transcript [src/mumps.rs:544]. Every append of `Session`, by `write`, `nl`, `tab` and `form_feed` (and `bell` through `write`), is admitted only while the transcript stays within the limit [src/mumps.rs:641-646, 658-747]; the first write that would pass it appends nothing and sets `output_limit_crossed`, nothing is appended after it, and `$X` and `$Y` still move, so the session decides as before until it ends. The next READ, through the private `read`, returns `Halt::OutputLimit` without issuing the READ [src/inquiry.rs:244-249], and `process_exit` returns it when `EN3` returns past the limit, the K14 `\n` included [src/inquiry.rs:256-264]; `run` maps it as it maps `Halt::InputExhausted`, to `Ok` with the transcript cut by `Session::into_transcript_at_prompt` after the last prompt written before that write, `returned` `None` and no exit `\n` [src/inquiry.rs:187-193]. So the transcript is never longer than the limit, and since the session stops at its first READ past it, its work is bounded too: one found order repeated gives at most 662 displays. The reasoning: without the bound, a body the bridge admits made one session write, hold and spend time on output in proportion to its repetitions, about 115 MB and 111 s for the largest body (history in the evidence), an uncontrolled consumption of memory and CPU on the bridge. Replay is bounded by the capture read budgets [src/golden.rs:550-563] and live mode by `LIVE_TIMEOUT` and `LIVE_OUTPUT_LIMIT` [src/bridge.rs:115-116], so the Rust backend was the only unbounded one. The original has no such limit (conflict K15). **Measured wall time:** `cargo test --offline --locked --test bridge rust_session_stops_at_the_output_limit -- --nocapture --test-threads=1`, run three times on the Linux build host (debug build, 4 CPUs), timed the `POST /api/rust/session` of a 900,019-byte body, `""`, then 60,000 × `"GC1001","",""`, then `"^"`, about 60,000 displays and 95 MB of output without the limit: each run printed `900019 body bytes, 1048026 transcript bytes`, with 1.249 s, 1.216 s and 1.180 s, and `test result: ok. 1 passed; 0 failed`; in the default parallel run of `tests/bridge.rs` it took 1.470 s. Unit tests: `src/mumps.rs` `output_limit_refuses_the_write_that_would_pass_it`; `src/inquiry.rs` `output_limit_ends_session_at_last_prompt` and `repeated_found_order_ends_at_the_output_limit`, whose 1,000 repetitions end `Ok` at 1,048,026 bytes, the order prompt after 662 whole displays, with no `\n`. The five transcripts are unchanged: each case stays far below the limit (at most 1,852 bytes), GC-01 to GC-05 PASS, and `end_to_end_real_captures` prints `MATCH` for each. Coverage stays 47/0/61, because the limit is not a branch of the original. Before round two this item read: OPEN, not blocking the five cases; each case runs at most one display; the original has the same unbounded `G EN30` loop [PRCHRPT.m:75], which the port reproduces; a bound needed a change to the bridge's admission, the private `Halt` type or the session model, which round one froze, so the port added no guard |
| O-12 | macOS test runs of the Rust suites: one `src/bridge.rs` unit test fails, and `tests/bridge.rs` fails when its tests run in parallel | The team ran `cargo test --offline --locked --no-fail-fast` on macOS with Rust 1.99.1 at commit `255c420ec` plus the committed `200.zwr`. Observed: `equivalence` 2 passed, and `REPORT.md` part 2 PASS for all five cases. `--lib` 163 passed and 1 failed: `bridge::tests::drain_discards_exactly_its_byte_budget` at `src/bridge.rs:5847`, `left` 1987797 and 2021449 against `right` 2097152 on two runs, failing when run alone. `tests/bridge.rs` 9 passed and 10 failed in parallel, each with `read: Resource temporarily unavailable (os error 35)`, and 19 of 19 passed with `-- --test-threads=1` in 54.94 s, including `end_to_end_real_captures`. `src/bridge.rs` is the same at `255c420ec` and `64482eef8`; its line 5847 is that test's `assert_eq!(discarded, DRAIN_BYTES, "the byte budget, exactly")`, and `DRAIN_BYTES` is `2 * MAX_BODY`, 2097152 [src/bridge.rs:95]. On the Linux build host at `64482eef8` the test passes when run alone, on two runs (`drain discarded 2097152 bytes`), and `tests/bridge.rs` passes 19 of 19 in parallel (4 test threads, 9.20 s) | **OPEN; not a parity difference.** The Linux build host cannot reproduce any of this. The cause is unknown. It is a timing or socket-behaviour difference on macOS, not a parity difference: on macOS too the five cases PASS. No code or test is changed for it. `RUNNING.md` step 2 gives the command to run on macOS |
| O-13 | `testingMethods.md` sentences that are now false | [testingMethods.md:48]: "O-01 (HOME device `IOF`/`IOSL`) is open, so GC-01, GC-02 and GC-05 FAIL once captured, until it is answered." Against the code and the run: O-01 is closed (this table); `STUB_CONTEXT` holds `IOF` `#` and `IOSL` `60` [src/inquiry.rs:117-122]; PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1 are implemented; `cargo test --offline --locked --test equivalence -- --nocapture` prints `GC-01 PASS` to `GC-05 PASS`, and `REPORT.md` part 2 reads PASS for all five cases. The file's other sentences agree with the code and the five-PASS state; the 158 tests of its line 41 are the crate's earlier tests, stated as history | **OPEN.** `testingMethods.md` is frozen in round two and is not edited, so the sentence stands. **Action for the team, in a later round:** replace it with the closed O-01 and the five PASS |
| O-14 | `tests/golden/ifcap/README.md` sentences that are now false | Line 5: "The folder holds only this README: no case folder, no `.gitkeep` and no placeholder. Every case is ABSENT, and therefore NOT COMPARED." The folder holds `GC-01/` to `GC-05/`, seven files each and all READY, and `state/`. Line 117: "`STUB_CONTEXT` is `[("DUZ", Some(".5")), ("IOF", None), ("IOSL", None), ("STATION", Some("500"))]`." It holds `("IOF", Some("#"))` and `("IOSL", Some("60"))` [src/inquiry.rs:117-122]. Line 119: "`IOF` and `IOSL` are unresolved (`None`, open item O-01). Any value is accepted, but both keys must be present;" O-01 is closed, and `validate_context` accepts only `IOF` `#` and `IOSL` `60`. Line 120: "Examples: `CONTEXT MISMATCH DUZ: expected .5, actual 1`, `CONTEXT MISMATCH IOF: expected UNRESOLVED (O-01), actual ABSENT`, `CONTEXT MISMATCH IOM: expected ABSENT, actual 80`." A missing `IOF` gives `CONTEXT MISMATCH IOF: expected #, actual ABSENT` [src/golden.rs:1885]. Line 172: "**While O-01 is open.** With `cases.zwr` loaded, the Rust sessions of GC-01, GC-02 and GC-05 end after `P.O./REQ.NO.: ` (with its one trailing space), because the display's `W:$Y>0 @IOF` depends on the unresolved HOME device [Packages/IFCAP/Routines/PRCHDP1.m:12]. Those cases FAIL once captured, until O-01 is answered." The three sessions run the whole display, and all five cases PASS. Line 186: "`tests/fixtures/ifcap/*.zwr` are the 11 supporting fixtures, unchanged copies of repository exports: `411.zwr`, `411.2.zwr`, `420.5.zwr`, `420.8.zwr`, `440.zwr`, `442.3.zwr`, `442.4.zwr`, `442.5.zwr`, `442.8.zwr`, `5.zwr` and `49.zwr`." The folder holds 12 supporting fixtures, these 11 and `200.zwr`, plus `cases.zwr`. Line 260: "If paging under the unresolved `IOSL` inserts an `ENTER '^' TO HALT: ` pause [Packages/IFCAP/Routines/PRCHDP2.m:31], the first capture shows it." `IOSL` is resolved, 60, and the GC-01, GC-02 and GC-05 transcripts show only the two display pauses. Line 278: "Include every global root of the supporting fixtures: `^PRC(411)`, `^PRC(411.2)`, `^PRCD(420.5)`, `^PRCD(420.8)`, `^PRC(440)`, `^PRCD(442.3)`, `^PRC(442.4)`, `^PRCD(442.5)`, `^PRC(442.8)`, `^DIC(5)` and `^DIC(49)`." The list lacks `200.zwr`'s root `^VA(200)`; that fixture's one node lies inside line 279's `^VA(200,.5)`, which every `db-before.zwr` holds, so the export scope as captured covers it | **OPEN.** The README's existing lines are frozen in round two: it gains only the step 6 paragraph on `200.zwr`, which passes the command-identity check and stays. **Action for the team, in a later round:** correct these lines to the closed O-01, the five PASS and the 12 supporting fixtures |

## Scope statement

Parity covers OPTION `PRCH DISPLAY PO` (IEN 1901, `EN3^PRCHRPT`), the five cases GC-01 to GC-05 and `worldvista/vehu:202609` only, with no claim of VA-wide equivalence.

- **Covered:** one read-only VistA workflow, OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the repository at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`, with the fixed context `DUZ` `.5` and station 500.
- **Not claimed:** no VA-wide equivalence; equivalence with any other image, snapshot, site, user, device or option; or any behaviour the five cases do not exercise. The five cases (GC-01 to GC-05) show only their exercised paths.
- **Read-only:** the port has no write path. `PRCHQ1` (OPTION 10295, New RFQ, `EN2^PRCHQ2`) was named in the request but creates RFQs, so `PRCH DISPLAY PO` was substituted. The original's own non-business writes (`^DISV`, the device handler's scratch under `^XUTL` and the DEVICE file's open count, word-wrap and delivery scratch under `^UTILITY` and `^TMP`) are documented and not reproduced.
- **Out of scope:** every write path (creating, editing, receiving, amending or cancelling orders); every other IFCAP function and option; VistA GUI clients, terminal devices and terminal emulators; a Java port; every other VistA package; the earlier GMV vitals slice; and the request's "must not be built" list: a write-guard framework, a CLI or exit-code scheme, git or provenance modules, deterministic-report machinery, Windows or macOS branches, an async runtime, any network service other than the loopback bridge, FFI or C bindings, procedural macros, a second crate or workspace, more than two integration test files, a front-end framework or build tooling, authentication or TLS.
- **Unexercised branches:** every branch marked "not exercised, not implemented" is outside equivalence; a session that reaches one ends after the last prompt written and invents no output. Before this refine round so were the four exercised branches marked "not implemented (O-01)", which are now implemented.

## Acceptance record

Not accepted.

Observed result, on the Linux build host: `cargo test --offline --locked` exits 0, with 189 passed and 0 failed; `REPORT.md` part 2 reads `Verdict: PASS` for all five cases, `GC-01` to `GC-05`; the bridge reports `RECORDED 5 OF 5 READY 5 OF 5`; and `end_to_end_real_captures` gets `match` true for each READY case (section "Run evidence", "Round two: completion evidence").

Observed on commit: `5534d6cd22ed5587da1885f5ca776de074ed283c`.

- Date:
- image_digest:
- snapshot_id:

The team records acceptance here at `runPlaybook.md` phase 12, filling the three fields with the date, `image_digest` and `snapshot_id`. This round records no acceptance.
