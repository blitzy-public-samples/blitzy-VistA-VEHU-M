# REPORT: PRCH DISPLAY PO (EN3^PRCHRPT) parity

Parity covers OPTION `PRCH DISPLAY PO` (IEN 1901, `EN3^PRCHRPT`), the five cases GC-01 to GC-05 and `worldvista/vehu:202609` only, with no claim of VA-wide equivalence.

This file is written in full by `tests/equivalence.rs` on every `cargo test` and is never edited by hand. Exit 0 of `cargo test` is not acceptance, which needs five PASS.

## Part 1: Run evidence

Each command below ran on 2026-11-15 in this checkout from `rust/ifcap-po-inquiry/` unless its entry says otherwise, with `RUSTUP_AUTO_INSTALL=0` set so that a missing toolchain fails instead of downloading. Each entry gives the command, its exit status and its output, standard error included. An output is complete unless its entry says it is trimmed. The read-only searches S5 to S9 are in `SPEC.md` section "Read-only enforcement results", the A7 commands in its section "Source-unchanged verification" and the capture listings in its section "Capture hashes".

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
total 44
drwxr-sr-x 2 root root  4096 Nov 15 10:15 .
drwxr-sr-x 3 root root  4096 Nov 15 08:14 ..
-rw------- 1 root root 33210 Nov 15 10:15 README.md
```

`ls tests/fixtures/ifcap/cases.zwr`, exit status 2:

```text
ls: cannot access 'tests/fixtures/ifcap/cases.zwr': No such file or directory
```

`tests/golden/ifcap/` holds only `README.md` and no `GC-0N` folder, and `tests/fixtures/ifcap/cases.zwr` does not exist, so every case is ABSENT and none is recorded. `cases.zwr` is not built, because there is no `db-before.zwr` to build it from.

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
test result: ok. 132 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running unittests src/bin/bridge.rs (target/debug/deps/bridge-758b3ca846ca0af2)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests/equivalence.rs (target/debug/deps/equivalence-7282b6d5b6d35e2c)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   Doc-tests ifcap_po_inquiry
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

152 tests pass: 132 unit tests in `src/`, 18 in `tests/bridge.rs` and 2 in `tests/equivalence.rs`. Its `equivalence_report` test prints one `GC-0N <verdict>` line per case on standard output, which the test runner captures and does not show for a passing test, so the verdicts appear in the next command's output.

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

All five cases are NOT COMPARED, because none has a capture. Exit 0 is not acceptance, which needs five PASS. `spec_has_report_sections` confirms that every section `REPORT.md` includes exists in this document.

`cargo test --offline --locked --test bridge` (criterion A5), exit status 0. Output with the `finished in <seconds>` timings trimmed; the per-test lines are in completion order:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s)
     Running tests/bridge.rs (target/debug/deps/bridge-c64136913db311bc)

running 18 tests
test end_to_end_real_captures ... ok
test compare_match_false_for_incomplete_case ... ok
test not_found_404_texts ... ok
test mumps_replay_502_not_recorded ... ok
test startup_with_empty_folder ... ok
test rust_session_200 ... ok
test mumps_replay_200_when_recorded ... ok
test compare_match_true_for_ready_equal_case ... ok
test compare_match_false_for_differing_ready_case ... ok
test compare_match_false_when_difference_is_outside_the_rule ... ok
test compare_match_true_when_rule_covers_the_difference ... ok
test compare_match_false_for_context_duz_1 ... ok
test bad_request_400_on_every_backend ... ok
test status_empty_folder ... ok
test status_ready_case ... ok
test status_incomplete_case ... ok
test static_paths_exact_match ... ok
test status_and_compare_incomplete_on_cr_or_malformed_rule ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Format and lint

`cargo fmt --check`, exit status 0, no output. `cargo clippy --offline --locked --all-targets`, exit status 0, with no warning. Output, with its `in <seconds>` timing trimmed and the absolute checkout path shown as `<crate>`:

```text
    Checking ifcap-po-inquiry v0.1.0 (<crate>)
    Finished `dev` profile [unoptimized + debuginfo] target(s)
```

### Audit searches S1 to S4

S1, `grep -rnE 'unsafe|\.unwrap\(|\.expect\(|panic!' src/`, exit status 0. The one hit is the allowlisted crate attribute:

```text
src/lib.rs:9:#![forbid(unsafe_code)]
```

S2, `grep -rnE '[A-Za-z_)]\[[^]]+\]' src/`, exit status 0. The one hit, reviewed, is a comment quoting the M code `DIC(0)["E"` [DIC.m:63], not Rust indexing:

```text
src/inquiry.rs:720:        // W:DIC(0)["E" ! ends the line before G ASK [DIC.m:63], whose W ! and
```

S3, `grep -n 'golden' src/inquiry.rs src/store.rs src/mumps.rs`, exit status 1, no output.

S4, `grep -rnE 'input\.txt|transcript\.txt|returned\.txt|meta\.txt|normalisation\.txt|db-before|db-after|tests/golden' src/`, exit status 0, 47 hits, every one in `src/golden.rs`. Output trimmed to the per-file summary of `grep -rlE` and `grep -cE` with the same pattern:

```text
$ grep -rlE '<same pattern>' src/
src/golden.rs
$ grep -rcE '<same pattern>' src/golden.rs
47
```

### Specification counts

Run on this document after its last edit, each exit status 0. Commands and outputs:

```text
$ grep -c '^| PO-[0-9]*\.b' SPEC.md
108
$ grep -c '^| PO-' SPEC.md
160
$ grep -c '^### PO-' SPEC.md
41
$ awk -F'|' '/^\| PO-[0-9]+\.b/{print NF}' SPEC.md | sort -u
8
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -cE '\| exercised by GC-0[1-5][^;|]* \|$'
43
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -c '| exercised by GC-01, GC-02, GC-05; not implemented (O-01) |$'
4
$ grep -E '^\| PO-[0-9]+\.b' SPEC.md | grep -c '| not exercised, not implemented'
61
```

- `108` is the branch inventory rows (A1), and `8` fields per row means each has six cells.
- `160` also counts the 41 "Mapping table" rows and the 11 "Data reads" rows: 108 + 41 + 11. It was 160 before this edit as well.
- `41` is the contract headings.
- `43`, `4` and `61` are the coverage groups of section "Unexercised branches": exercised and implemented; exercised, not implemented pending O-01; not exercised, not implemented.

### Document limits

`wc -l -w testingMethods.md`, exit status 0: `52  593 testingMethods.md` (at most 60 lines and 600 words). `wc -l runPlaybook.md`, exit status 0: `116 runPlaybook.md` (at most 150 lines).

### HL7 domain

From `Packages/IFCAP/Routines/`, `grep -il "HLCS\|HL7\|DOMAIN" PRCHRPT.m PRCFSITE.m PRCHPAT.m PRCHDP1.m PRCHFPNT.m PRCUTL.m PRCHDP2.m PRCHUTL.m PRCHDP6.m PRCHDP4.m`, exit status 1, no output: no routine on the call tree references an HL7 or domain node (section "Stubs").

### Bridge runtime (A12)

Several checkouts share this host, so the bridge, which always binds `127.0.0.1:8080`, ran inside a private network namespace (`unshare -n`, loopback brought up), never in the host's. One `bash` script ran there, exit status 0:

```sh
ip link set lo up
cargo run --offline --locked --bin bridge > out1 2> err1 &
cpid=$!
for i in $(seq 1 120); do curl -s -o /dev/null http://127.0.0.1:8080/ && break; sleep 0.5; done
curl -s http://127.0.0.1:8080/api/status
timeout 10 ./target/debug/bridge
timeout 20 cargo run --offline --locked --bin bridge
bpid=$(ss -ltnpH "sport = :8080" | sed -n "s/.*pid=\([0-9]*\).*/\1/p" | head -1)
kill $bpid; wait $cpid
```

The first instance's standard output, its three startup lines:

```text
http://127.0.0.1:8080/
MUMPS MODE: REPLAY
RECORDED 0 OF 5 READY 0 OF 5
```

Its standard error held only cargo's two status lines, `Finished …` and `` Running `target/debug/bridge` ``. `curl -s http://127.0.0.1:8080/api/status`, exit status 0:

```text
{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}}
```

The second instance, run as `./target/debug/bridge`, exited with status 1 and wrote nothing to standard output; its standard error:

```text
bridge: port 8080 on 127.0.0.1 is already in use
```

Run again as `cargo run --offline --locked --bin bridge`, it exited with status 1 and the same message after cargo's `Finished` and `Running` lines. The first instance was then stopped by `kill` on the pid owning port 8080 in that namespace, cargo reported status 143 (terminated by signal 15), and no socket was left listening.

### Reconciliation search

The whole-document search for removed APIs, closed items and superseded wording, run on this document after its last edit with this section left out, because this section quotes the search itself. Command and output, exit status 0:

```sh
awk '/^## Run evidence$/{s=1;next} /^## /{s=0} !s' SPEC.md | grep -oE 'UnsupportedBranch|Halt::Error|BLOCKED|422|serve_until|MANIFEST|O-03|test-results|UNSUPPORTED BRANCH|StoreError::Io|absorb|NOT COMPARED|split_input_lines|parse_normalisation|parse_meta|inquiry_error_to_response|decide_live_result|run_command|predicted' | sort | uniq -c
```

```text
      3 422
      3 BLOCKED
      3 MANIFEST
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

- `O-03`: most are the substring of the contract ID `PO-03`; the rest name O-03 as closed (R4, "Open items") or name the O-03 guard and fixture conditions.
- `422`: two are `DD.zwr` line numbers (514222, 514228); one, in "Source corrections and interface departures", C12, states that the 422 status is removed.
- `UnsupportedBranch`, `UNSUPPORTED BRANCH`, `BLOCKED`, `serve_until`, `decide_live_result`, `run_command`, `inquiry_error_to_response`: in sentences marked as history ("Before this run", "now"), in the request's statements of "Existing-state verification", or naming the private functions of `src/bridge.rs` and their unit tests.
- `split_input_lines`, `parse_normalisation`, `parse_meta`: the items of `src/golden.rs`, in "Mapping table" and "Normalisation".
- `MANIFEST`, `test-results`: the deleted items, in "Existing items outside the layout", "Existing-state verification" and "Source-unchanged verification".
- `NOT COMPARED`: the verdict, never a front-end `match` display.
- `predicted`: "the plan's predicted" station echo, history, in "Source corrections and interface departures".

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

`STUB_CONTEXT` in `src/inquiry.rs` is exactly `[("DUZ", Some(".5")), ("IOF", None), ("IOSL", None), ("STATION", Some("500"))]`. Every capture's `meta.txt` `context` must be `{"DUZ":".5","IOF":"<value>","IOSL":"<value>","STATION":"500"}`, and `golden::validate_context` checks it against `STUB_CONTEXT` key by key:

- every `STUB_CONTEXT` key must be present;
- a resolved constant must be equal;
- a key whose constant is `None` (`IOF` and `IOSL`, O-01) accepts any value and is reported `UNRESOLVED (O-01)`, the expected value a failure shows when that key is missing;
- an extra key fails;
- a failure reads `CONTEXT MISMATCH <key>: expected <value or ABSENT>, actual <value or ABSENT>`, for example `CONTEXT MISMATCH DUZ: expected .5, actual 1`.

No capture value overrides a constant. Diagnostic values such as `IOM` and the HOME device's name go in `meta.txt` `setup`, never in `context` (O-01). This replaces the rule, before this run, that failed every capture on `IOF` while O-01 was open.

| Stub | Original source | Fixed value or behaviour | Status |
|---|---|---|---|
| `DUZ` | Test code `D DUZ^XUP(.5)` [ZOSVGUT1.m:13]; POSTMASTER `^VA(200,.5,0)` [200+NEW PERSON.zwr:4]; node 20 piece 2 `POSTMASTER` [200+NEW PERSON.zwr:12], so `D A` never writes [PRCFSITE.m:38]. The request cites tech-spec section 9.1 for this test code; the code is in `ZOSVGUT1.m`, and 9.1 records only station 500 (conflict C10) | `".5"` | SOURCE-RESOLVED |
| `STATION` | Default `$O(^PRC(411,"AC","Y",0))` [PRCFSITE.m:14] = 500 [411 export:290]; accepted by RETURN [PRCFSITE.m:18] | `"500"` | SOURCE-RESOLVED |
| `IOF` | `W:$Y>0 @IOF` [PRCHDP1.m:12] [PRCHDP2.m:42]; set by `^%ZIS` from the HOME device's terminal type, default `"#"` [ZIS1.m:103, 106, 113] | UNRESOLVED (`None`); PO-10.b4, PO-12.b1 and PO-26.b1 are not implemented | OPEN (O-01) |
| `IOSL` | `$Y+5>IOSL` [PRCHDP2.m:31, 54]; set by `^%ZIS` [ZIS1.m:103-104] | UNRESOLVED (`None`); PO-30.b1 is not implemented | OPEN (O-01) |
| `prcfsite` + `dic` output (PO-03 to PO-05) | `W !` [PRCFSITE.m:8]; `W !` [PRCFSITE.m:18]; `W !` [DIC.m:25]; prompt and default [DIC11.m:15-17]; echo [DIC2.m:10, 32] through `^DD(411,0,"ID",.1)` [DD.zwr:488724] and `^DIC(4,500,0)="CAMP MASTER^…"` [4+INSTITUTION.zwr:494] | Writes `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// `; consumes one input line; an empty line writes `      CAMP MASTER` (six spaces, no newline) and sets `PRC("SITE")=500`; a non-empty line ends the session after the station prompt (PO-05.b2, not implemented); no line left is `Halt::InputExhausted`, which ends it there too. Reads nothing from `Store` | SOURCE-RESOLVED (byte trace R1 in SPEC.md) |
| `duz_prcfsite` (PO-04) | [PRCFSITE.m:35-40] | No output; no `Store` read; `PRC("PER")` not modelled because no output reads it | SOURCE-RESOLVED |
| `ix_dic` lookup (PO-09) | `IX^DIC` [DIC.m:84-86] and the lookup routines | Lookup rules over `Store` data (R5, R6), never a list of known identifiers. The unique-match echo is resolved; the search-exhausted outcome PO-09.b14 is implemented: the R3 sequence, under the O-03 guard of R5 step 6, then the prompt again | b3, b8, b14 SOURCE-RESOLVED (O-03 closed for the five cases' values) |
| `pct_zis` (PO-12) | [ZIS.m:28-37] [ZIS1.m:103-113] | Always `Halt::Unexercised("PO-12.b1")`: the session ends after the last prompt written | OPEN (O-01); PO-12.b1 not implemented |
| `pct_dt` (PO-41) | `D ^%DT` with `%DT=""`, `X="T"` [PRCFSITE.m:15]; `%DT` writes only under the `A` or `E` flag [DIDT.m:11-12, 63, 67, 92] [ZTDT.m:3-4, 43, 46, 55] | No output and no value. The date feeds only `PRC("FY")`, `PRC("QTR")`, `A` and `B` [PRCFSITE.m:15]; `OUT` kills `A`, `B`, `X`, `Y` and leaves `PRC("FY")`, `PRC("QTR")` defined [PRCFSITE.m:29]; their only readers on the path are the copy into `X` at `FY`, which `SE1` overwrites [PRCFSITE.m:21, 26], and the year and quarter prompts and budget line that `PRCF("X")="SP"` skips [PRCFSITE.m:20, 23, 28] [PRCHRPT.m:5]; `EN3` runs `ST` once [PRCHRPT.m:70-75] and no display routine reads them. So they are omitted | SOURCE-RESOLVED (runtime identity O-04) |
| Control point | No control-point read or prompt anywhere on the `EN3` path (conflict C11) | None needed | SOURCE-RESOLVED |
| Current date and time | Not stubbed: the one date the path computes, through `^%DT`, reaches no output | Handled only through `normalisation.txt`, expected empty | Team confirms with the captures |
| `DTIME`, `U`, `DT` | `R X:DTIME` [PRCHDP2.m:10, 41]; `DT` [PRCFSITE.m:7]; `U` [PRCFSITE.m:9] | Preconditions set by the capture start command (O-07), with `DT=$$DT^XLFDT` [XLFDT.m:159]. Because `DT` is defined, the `NOW^%DTC` branch PO-03.b2 [PRCFSITE.m:7] stays not exercised. The Rust READ never times out | README states them (O-07) |
| HL7 domain `HL7.VEHU.DOMAIN.GOV` | No routine on the call tree references an HL7 or domain node: `grep -il "HLCS\|HL7\|DOMAIN"` over the ten call-tree routines finds none | Not stubbed and not in `context` | Recorded as instance identity only |

## Flagged hard-MUMPS features

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`.

Rule (conflict K10, superseding C8): every indirection, XECUTE, LOCK or device-control feature on the call tree is flagged "requires runtime shim, not converted" here, and `REPORT.md` copies this section. No XECUTE, indirection or LOCK evaluator and no device shim exists in the crate. Separately, where the operand is a literal fixed in the pinned source or data dictionary on an exercised path, `src/inquiry.rs` holds a rule equivalent: ordinary Rust code reproducing the observable effect of that one literal, cited to the literal's line. It is not a conversion of the language feature: it evaluates no M code and would not follow a change to the literal. Without the rule equivalents for the lookup screens and identifiers no case could reach its output, while the flag keeps every such site visible. Where the operand depends on runtime state there is no rule equivalent: the branch is not implemented, and a session that reaches it ends after the last prompt written.

| Feature | Routine and line | Flag | Rule equivalent in inquiry.rs |
|---|---|---|---|
| `D ^%ZIS` device open, then `W:$Y>0 @IOF` | [PRCHDP1.m:12] | requires runtime shim, not converted | None. The operand depends on the HOME device (O-01), so the session ends after the last prompt written: PO-12.b1 and PO-10.b4 are not implemented, and a found order ends at PO-12.b1, the first of them it reaches (`pct_zis`) |
| `W:$Y>0 @IOF,!!…` column header | [PRCHDP2.m:42] [PRCHDP4.m:35] | requires runtime shim, not converted | None (O-01). The postcondition governs every argument of the WRITE; PO-26.b1 is not implemented and the session ends after the last prompt written (`form_feed`); the `PRCHDP4` occurrence is n/e |
| `$Y+5>IOSL` paging | [PRCHDP2.m:31, 54] | requires runtime shim, not converted | None (O-01). `IOSL` is unresolved; PO-30.b1 is not implemented and the session ends after the last prompt written (`ask_prchdp2`) |
| `X DIC("S")` order screen | [DIC3.m:124] [PRCHRPT.m:9] [PRCHPAT.m:81] | requires runtime shim, not converted | Fixed predicate for the literal screen `I +$P(^(0),U,1)=PRC("SITE"),$P(^(0),U,2)<10!($P(^(0),U,2)=25)`, with the naked reference on `^PRC(442,ien,0)` [DIC3.m:122] (`screen_passes`) |
| `X DIC("S")` station screen `I +Y<1000000` | [PRCFSITE.m:18] [DIC3.m:124] | requires runtime shim, not converted | Fixed predicate for the literal screen inside the `dic` stub; 500 passes |
| Identifier XECUTE: `DIC("W")` built from `^DD(file,0,"ID",…)`; for 442 it overflows 224 characters and becomes `D WOV^DICQ1`, which XECUTEs each identifier after `W "  ",$E(^PRC(442,Y,0),0)` resets the naked reference | [DIC1.m:46-60] [DICQ1.m:86-88] [DD.zwr:514202-514204] | requires runtime shim, not converted | Fixed output for the literal identifiers (R2), never by executing the code (`write_order_echo`) |
| Station identifier `^DD(411,0,"ID",.1)` with `@(…)` and `D Y^DIQ` | [DD.zwr:488724] (exported again at 488748) | requires runtime shim, not converted | Fixed station stub output: evaluated once for the fixed context into the `dic` constant `STATION_ECHO` (R1) |
| Pre-lookup node `X ^DD(411,.01,7.5)` | [DIC.m:36] [DD.zwr:488792] | requires runtime shim, not converted | Fixed station stub output: the literal `S PRCRI=$P(^PRC(411,0),U,3)` writes nothing, so the `dic` stub writes nothing for it |
| Pointer-transform screen: `P^DICM0` builds `DIC("S")` from literal index and file names and XECUTEs it inside a nested lookup; `G @Y` dispatches on the literal field type | [DICM0.m:8-41] [DICM1.m:8-11] [DICM.m:30-34] | requires runtime shim, not converted | R6 (`pointer_candidate`) and the O-03 guard of R5 step 6 (`no_match_guard`); PO-09.b11 is n/e |
| Name indirection `@(DIC_…)` | [DIC1.m:41] [DIC3.m:42, 63, 83, 109-125] | requires runtime shim, not converted | Fixed roots `^PRC(442,` [PRCHPAT.m:79] and `^PRC(411,` [PRCFSITE.m:18] |
| `%XY^%RCR` indirection `@%Z`, `$Q(@%Z)` | [DIRCR.m:29] (the Kernel copy uses `$N` [ZTRCR.m:24-27]) | requires runtime shim, not converted | A copy of the literal scratch lines: `%X` and `%Y` are literal [PRCHDP1.m:54] [PRCHDP2.m:48] (`pct_xy_pct_rcr`) |
| `RCR^DIWW` `\|` windows | [DIWP.m:40] | requires runtime shim, not converted | None: PO-15.b2 is n/e, and a session that reaches it ends after the last prompt written |
| `D EN^DIQ`; `D Y^DIQ` | [PRCHDP1.m:9] [PRCHUTL.m:76-77] | requires runtime shim, not converted | None: PO-10.b2 and PO-29.b3 are n/e |
| `G @DIPGM`; `G @^("LOOK")` | [DIC.m:21, 38] [DICM.m:10] | requires runtime shim, not converted | None. Not reached on the `EN3` path: the target nodes are absent from the DD (section "Workflow selection", "Dynamic targets") |
| `D @PRCHQ` | [PRCHRPT.m:43] | requires runtime shim, not converted | None. Not reached on the `EN3` path: `EN0` path only |
| `LOCK`: `L +^PRC(442.6,…)` in `NUM`, reached from `EN`, not `EN3` | [PRCHPAT.m:29] | requires runtime shim, not converted | None. Not reached on the `EN3` path |

Not hard features, so not flagged: naked references `^(…)`, throughout (for example [PRCHDP1.m:4] [PRCHDP2.m:11, 49-50] [PRCHDP1.m:61-62]), each resolved to the full global reference it denotes at that point; and `R X:DTIME`, `W !`, `W ?n` and `$C(7)`, throughout, modelled in `src/mumps.rs`.

## Unexercised branches

These branches are outside equivalence: a branch no case exercises is not implemented, no capture judges it, and no verdict in `REPORT.md` covers it. A session that reaches a branch that is not implemented, whether exercised by no case or left unresolved by an open item, ends after the last prompt written. The contract function stops with the private `Halt::Unexercised` carrying the branch ID, and `inquire` returns `Ok`, with the transcript cut at that prompt by `Session::into_transcript_at_prompt` and `returned` `None`, so no output is invented after it. There is no error variant for it: `InquiryError` has only `CorruptStore`.

Citations: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, `Z*.m` are `Packages/Kernel/Routines/`; `DD.zwr` is `Packages/VA FileMan/Globals/`; the 411 export is `Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr`.

### Coverage counts

| Coverage | Count | Branches |
|---|---|---|
| Exercised and implemented | 43 | The 42 rows exercised before this run, and PO-09.b14, exercised by GC-03 and GC-04 since the O-03 closure (R4) made its sequence source-determined for those inputs |
| Exercised, not implemented pending O-01 | 4 | PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1, each depending on `IOF` or `IOSL` [PRCHDP1.m:12] [PRCHDP2.m:31, 42, 54] |
| Not exercised, not implemented | 61 | Every other row. PO-09.b7 and PO-09.b12 are unreachable inside this group. PO-03.b2 (`DT` undefined, `NOW^%DTC` [PRCFSITE.m:7]) stays in it, because the capture start command defines `DT` (O-07) |

### Branch granularity

- **Decisions:** an `IF`, with its `ELSE`, that governs a `DO` or extrinsic call, a `GOTO`, `QUIT` or `READ`, a loop or a block; a postcondition on a `DO`, `GOTO` or `QUIT`; a loop over stored entries whose body runs a `DO`, `GOTO`, `QUIT` or block, with the outcomes "at least one entry" and "no entry".
- **Not decisions** (they belong to the enclosing branch and are implemented with it): an `IF` or postcondition governing only `SET`, `WRITE` or `KILL` commands; a `$SELECT` arm; a loop whose body only formats the entries it visits; a fixed-count loop.
- **Branch IDs:** each outcome of a decision is a branch, numbered `PO-NN.bM` in source order within its contract. PO-09.b15 and PO-09.b16 were found after the plan fixed b1 to b14 and are appended so existing IDs stay stable. An exercised outcome may be described inside its enclosing branch's row; every outcome no case exercises has its own ID, because `Halt::Unexercised` carries it and the unit tests assert it through the private `run`; decisions inside an n/e branch are covered by that branch's ID.
- **Coverage words:** "exercised by GC-0N", for an exercised and implemented branch; "exercised by GC-01, GC-02, GC-05; not implemented (O-01)", for an exercised branch whose source cannot be settled while O-01 is open (conflict K9); "not exercised, not implemented" (n/e). A session that reaches either of the last two ends after the last prompt written and writes nothing for the branch. "Unreachable", inside the n/e group, marks an outcome that a fixed constant on this path rules out; it needs no code. The coverage of exercised branches follows from source and the fixture profile; no capture has yet confirmed it.
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
| PO-10.b4 | `^PRCHDP1` | [PRCHDP1.m:10-44] | 442 node 1, whole (its first field 5 VENDOR `1;1` [DD.zwr:514589]) | The page-1 header (ship-to, `^%ZIS`, `@IOF`, labels, vendor and ship-to rows, underscores) | exercised by GC-01, GC-02, GC-05; not implemented (O-01) |
| PO-10.b5 | `^PRCHDP1` | [PRCHDP1.m:16-17] | 442.11 PARTIAL, node 11, field .6 [DD.zwr:514467] | Node 11 present: the `LAST PARTIAL RECD.` loop | not exercised, not implemented |
| PO-10.b6 | `^PRCHDP1` | [PRCHDP1.m:32-34] | 440,.2 EDI VENDOR? `3;2` [DD.zwr:510094]; 442,116 DO YOU WANT TO SEND THIS EDI? `12;16` [DD.zwr:515424] | EDI vendor (440 field .2 `Y`) and 442 field 116 not `n`: the EDI block | not exercised, not implemented |
| PO-10.b7 | `^PRCHDP1` | [PRCHDP1.m:45-52] | 442,65 INTERFACE PACKAGE PREFIX `24;3` [DD.zwr:515073] | Field 65 equal to `RMPR`: the prosthetics block | not exercised, not implemented |
| PO-11.b1 | `FTYP^PRCHFPNT` | [PRCHFPNT.m:44-45, 49, 52-53] | 442,31 SUBSTATION `23;7` [DD.zwr:514881], empty; then 411,.5 FACILITY TYPE `0;7` [DD.zwr:488809] | Facility type from site piece 7 through `^PRC(411.2)` piece 2 | exercised by GC-01, GC-02, GC-05 |
| PO-11.b2 | `FTYP^PRCHFPNT` | [PRCHFPNT.m:46-47] | 442,31 SUBSTATION `23;7` [DD.zwr:514881] | Substation, node 23 piece 7 | not exercised, not implemented |
| PO-12.b1 | `^%ZIS` | [ZIS.m:28-37] [ZIS1.m:103-113] | none (`^XUTL("XQ",$J,"IO")` and the HOME device's 3.5 and 3.2 entries, O-01) | `IOP="HOME"` device set-up | exercised by GC-01, GC-02, GC-05; not implemented (O-01) |
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
| PO-26.b1 | `HDR^PRCHDP2` | [PRCHDP2.m:42] | none (`$Y` and `IOF`, O-01) | `W:$Y>0 @IOF,…` column header, then dashes | exercised by GC-01, GC-02, GC-05; not implemented (O-01) |
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
| PO-30.b1 | `ASK^PRCHDP2` | [PRCHDP2.m:31] | none (`$Y` and `IOSL`, O-01) | `$Y+5'>IOSL`: no prompt | exercised by GC-01, GC-02, GC-05; not implemented (O-01) |
| PO-30.b2 | `ASK^PRCHDP2` | [PRCHDP2.m:31] | none (`$Y` and `IOSL`, O-01) | Paging prompt and `HDR` | not exercised, not implemented |
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
| O-01 | HOME device context: the bytes `^%ZIS` writes for `IOP="HOME"`, the `IOF` value and the bytes `W @IOF` emits, `IOSL`, and `$Y` after a form feed | [PRCHDP1.m:12] [PRCHDP2.m:31, 42] [ZIS.m:28-38, 46, 58-81] [ZIS1.m:103-113] [3.5+DEVICE.zwr:14-18, 679] [3.2+TERMINAL TYPE.zwr:816-817, 1153] | **OPEN, narrowed.** `PRCHDP1` sets `IOP="HOME",%ZIS=""` and calls `^%ZIS`, then `W:$Y>0 @IOF` [PRCHDP1.m:12]. With `^XUTL("XQ",$J,"IO")` defined, `HOME` restores the sign-on values silently [ZIS.m:28, 58-81]. Without it, `GETHOME` looks the device up by `$I` through `^%ZIS(1,"G","SYS."_vol_"."_$I)` [ZIS.m:31-38, 46], and when nothing matches it can write `HOME DEVICE (…) DOES NOT EXIST IN THE DEVICE FILE` [ZIS.m:36]. The candidates disagree: device 267 `PMT290` (`$I` `"0"`, key `SYS..0`) gives `132^#^60` [3.5+DEVICE.zwr:14-18, 679] [3.2+TERMINAL TYPE.zwr:816-817]; device 595 `CONSOLE` gives `80^#,$C(27,91,50,74,27,91,72)^24` [3.2+TERMINAL TYPE.zwr:1153]; an empty `IOF` becomes `#` [ZIS1.m:103-113]. The YottaDB documentation does not settle the bytes or `$Y` of `WRITE #` on a piped principal device. **Question for the team:** under the capture start command, which `IOF` and `IOSL` does `^%ZIS` set at PRCHDP1.m:12, and what bytes does the GC-01 transcript show for `W @IOF`? Record `IOF` and `IOSL` in `meta.txt` `context`, which holds exactly the four `STUB_CONTEXT` keys, and `IOM` and the device name, diagnostic only, in `meta.txt` `setup`. The next run then sets the `STUB_CONTEXT` constants and implements PO-10.b4, PO-12.b1, PO-26.b1 and PO-30.b1; if the answer shows cursor addressing, the stop condition of section "Workflow selection" applies. **Effect:** the GC-01, GC-02 and GC-05 Rust sessions end after `P.O./REQ.NO.: `, so those cases FAIL once captured, until O-01 is answered |
| O-02 | Exact FileMan echo bytes for station default acceptance, the unique order match, and the no-match sequence | [DIC.m:25, 63-65, 80-81] [DIC2.m:8-98] [DIC5.m:21-31] [DIC1.m:46-60] [DICQ1.m:86-88] [DD.zwr:488724, 514202-514204] | **Closed by R1, R2 and R3** of section "Resolution decisions binding src/inquiry.rs"; PO-03, PO-05.b1, PO-09.b8 and PO-09.b14 are SOURCE-RESOLVED. A capture that disagrees reopens it |
| O-03 | Transform behaviour for the GC-03 and GC-04 values | [DICM.m:10, 13-36, 107] [DICM1.m:13-40] [DICM0.m:8-60] [DICF2.m:110-127] [DIDT.m:8-25] | **CLOSED for the five cases' values, under the fixture profile (R4).** A nested pointer lookup runs only for a 442 pointer index that holds entries [DICM0.m:26, 32]. With `G`, `M` and `MCH` empty, only 440 (`D`, field 5 [DD.zwr:514589]), 442.5 (`F`, .02 [DD.zwr:514331]) and 442.3 (`ST`, .5 [DD.zwr:514433]) are visited. None of the three has a date, pointer or variable-pointer index, a node-7 field or a new-style index [DD.zwr:509908-509917, 517989-517991, 518127-518128] [0.11+INDEX.zwr], so no `%DT` echo can occur. The only set-typed indexes are 440 `AC` (.05, `S:SUPPLY WAREHOUSE` [DD.zwr:510071]) and `AE` (31.5, `1:INACTIVATED` [DD.zwr:510441]); `S^DICM1` matches an internal code or an external prefix [DICM1.m:30-39], and neither `GC3003` nor `INVALID!` does either. Nested misses write nothing [DICM.m:107]. `ix_dic` writes PO-09.b14 only under the guard of R5 step 6, which checks these conditions; otherwise the session ends as the n/e PO-09.b11 |
| O-04 | Which `%RCR` and `%DT` are installed at runtime: `DIRCR.m` and `DIDT.m` (FileMan 22.2) or `ZTRCR.m` and `ZTDT.m` (FileMan 19.0 copies under Kernel) | [DIRCR.m:1-3, 24-30] [ZTRCR.m:1-3, 21-27] [DIDT.m:1-2, 11-12] [ZTDT.m:1-4] | OPEN, not blocking: each pair behaves the same on this path (subtree copy; no output without `A` or `E`) |
| O-05 | Image digest, and whether the runtime database is commit `dd3fd0f3…` (`snapshot_id`) | `meta.txt` (team) | OPEN; recorded with the captures as `meta.txt` `image_digest` and `snapshot_id`, which `REPORT.md` part 2 shows per case |
| O-06 | How live mode obtains the original's return value | [19+OPTION.zwr:14691] [PRCHRPT.m:70-75] | **CLOSED.** `EN3^PRCHRPT` is entered by `DO` [19+OPTION.zwr:14691] and quits without a value [PRCHRPT.m:70-75], so `returned` is `None` on both sides and `returned.txt` is zero bytes; live mode needs no channel for a return value |
| O-07 | The capture start command and `docker_exec_args`: the context set-up (`U`, `DTIME`, `DT` through `$$DT^XLFDT`, `DUZ` through `DUZ^XUP(.5)`; the HOME device is O-01) | [XLFDT.m:159] [XUP.m:54-64] [PRCFSITE.m:7] | **OPEN.** `DUZ^XUP` sets `DUZ` and its parts but not `DT`, `DTIME` or `IO*` [XUP.m:54-64]; defining `DT` keeps `^PRCFSITE` off `NOW^%DTC`, PO-03.b2 [PRCFSITE.m:7]. The README's capture command, run from inside the case folder in the team's shell and unverified, is `docker exec -i vehu su - vehu -c 'source /home/vehu/etc/env && yottadb -run %XCMD "S U=\$C(94),DTIME=300,DT=\$\$DT^XLFDT D DUZ^XUP(.5) D EN3^PRCHRPT"' < input.txt > transcript.txt`. `meta.txt` `docker_exec_args` is split on single spaces and cannot hold its `-c` argument (conflict K3). **Question for the team:** supply a start command whose arguments contain no space, or change the convention. **Live mode is delivered unverified; replay is the delivered MUMPS backend** |
| O-08 | Pinned toolchain presence on the build host | Planning host: `rustc: command not found` | **CLOSED for this run's host.** Rust 1.99.1 and the C linker `cc` are verified: section "Run evidence", "Toolchain" |

## Scope statement

Parity covers OPTION `PRCH DISPLAY PO` (IEN 1901, `EN3^PRCHRPT`), the five cases GC-01 to GC-05 and `worldvista/vehu:202609` only, with no claim of VA-wide equivalence.

- **Covered:** one read-only VistA workflow, OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the repository at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`, with the fixed context `DUZ` `.5` and station 500.
- **Not claimed:** no VA-wide equivalence; equivalence with any other image, snapshot, site, user, device or option; or any behaviour the five cases do not exercise. The five cases (GC-01 to GC-05) show only their exercised paths.
- **Read-only:** the port has no write path. `PRCHQ1` (OPTION 10295, New RFQ, `EN2^PRCHQ2`) was named in the request but creates RFQs, so `PRCH DISPLAY PO` was substituted. The original's own non-business writes (`^DISV`, word-wrap and delivery scratch under `^UTILITY` and `^TMP`) are documented and not reproduced.
- **Out of scope:** every write path (creating, editing, receiving, amending or cancelling orders); every other IFCAP function and option; VistA GUI clients, terminal devices and terminal emulators; a Java port; every other VistA package; the earlier GMV vitals slice; and the request's "must not be built" list: a write-guard framework, a CLI or exit-code scheme, git or provenance modules, deterministic-report machinery, Windows or macOS branches, an async runtime, any network service other than the loopback bridge, FFI or C bindings, procedural macros, a second crate or workspace, more than two integration test files, a front-end framework or build tooling, authentication or TLS.
- **Unexercised branches:** every branch marked "not exercised, not implemented", and every exercised branch marked "not implemented (O-01)", is outside equivalence; a session that reaches one ends after the last prompt written and invents no output.

## Acceptance record

Not accepted: no case has a capture.
