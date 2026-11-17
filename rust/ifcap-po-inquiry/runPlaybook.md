# Run playbook: PRCH DISPLAY PO (EN3^PRCHRPT) parity

Twelve phases, in order, take a clean checkout to a recorded acceptance decision for the Rust port of OPTION `PRCH DISPLAY PO` on `worldvista/vehu:202609`. Each phase is a table, and each row is one step with its **Owner**, what to **Do**, what to **Expect**, and what to do **If not**.

- **Owners.** `Developer` works on the build host. `Capture owner` is the team member on the Docker host with `worldvista/vehu:202609`. `Reviewer` judges the results and records acceptance.
- **Commands** are quoted verbatim from `RUNNING.md` and `tests/golden/ifcap/README.md` ("README" below), each with the section it comes from. Run them from `rust/ifcap-po-inquiry/` unless the phase says otherwise, with each `<…>` replaced by a real value.
- Where this playbook differs from those files or from the code, the code governs, then `RUNNING.md` and the README.
- Phases 1 to 6 need no capture. On a clean checkout without the captures they end with exit 0, five NOT COMPARED and `RECORDED 0 OF 5`. With the five captures present, as committed on this branch, phase 4 exits 0 with five PASS (`GC-01` to `GC-05`) and phase 5 shows `RECORDED 5 OF 5 READY 5 OF 5`.

## Phase 1. Prepare the host

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | `rustup toolchain install 1.99.1`, and install a system C linker, `cc`, for example from gcc (`RUNNING.md` step 1) | In the crate folder, `rustc -V` prints `rustc 1.99.1 (…)`, and `cc` is on `PATH` | Install the toolchain and the linker. Offline, set `RUSTUP_AUTO_INSTALL=0` (`RUNNING.md` step 2) and use a host prepared with both |

## Phase 2. Verify the source is unchanged

From the repository root (criterion A7; `SPEC.md` "Source-unchanged verification").

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | `git diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION` | Exit status 0 | Restore `Packages/**` and the root files from git before continuing: the port is judged against the pinned MUMPS source and exports only |
| Developer | `git status --porcelain -- . ':!rust/ifcap-po-inquiry'` | Nothing printed | Each printed path lies outside this work: restore it from git, or move an untracked file out of the checkout |

## Phase 3. Build

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | `cargo build --offline --locked` (`RUNNING.md` step 2) | Exit status 0 | A toolchain error (`… is not installed`): back to phase 1. A lock-file error: `Cargo.lock` must exist and be the committed one, so restore it from git and never regenerate it |

## Phase 4. Test without captures

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | `cargo test --offline --locked` (`RUNNING.md` step 2) | Exit status 0. `REPORT.md` is rewritten: part 2 gives all five cases NOT COMPARED, and part 3 gives each `ABSENT`. With the five captures present: exit status 0, `end_to_end_real_captures` in `tests/bridge.rs` gets `match` true for each case, and `REPORT.md` is rewritten: part 2 gives all five cases PASS (phase 10), and part 3 gives each READY | A FAIL in `tests/equivalence.rs` names its cause in `REPORT.md` part 2 and in the failure message, `GC-0N FAIL: <cause>`. A failure at `tests/bridge.rs`, such as `end_to_end_real_captures` naming the failing case, `GC-0N: …`, stops cargo before `tests/equivalence.rs` and leaves `REPORT.md` as it was: the next row's command gives each verdict and cause. A missing `SPEC.md` heading fails `tests/equivalence.rs`, names the heading and writes no report: restore `SPEC.md` from git |
| Developer | `cargo test --offline --locked --test equivalence -- --nocapture` (`RUNNING.md` step 2) | The lines `GC-01 NOT COMPARED` to `GC-05 NOT COMPARED`. With the five captures present: `GC-01 PASS` to `GC-05 PASS`, exit status 0, and `REPORT.md` is rewritten: part 2 gives all five cases PASS, and part 3 gives each READY | Without the captures, another verdict means `tests/golden/ifcap/` holds a case folder: this phase needs a clean checkout. With the five captures present, another verdict means the port, the fixtures or a capture differs from this branch: `REPORT.md` part 2 names each FAIL's cause, and part 3 each case's readiness, READY or `INCOMPLETE: <check names>`. A NOT COMPARED case lacks its folder, `input.txt` or `transcript.txt`: restore `tests/golden/ifcap/` from git, and never edit a capture. Treat each other case by its phase 10 row |

## Phase 5. Start the bridge with zero captures

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | `cargo run --offline --locked --bin bridge` (`RUNNING.md` step 2), and leave it running | Standard output, in order: `http://127.0.0.1:8080/`, `MUMPS MODE: REPLAY`, `RECORDED 0 OF 5 READY 0 OF 5`; with the five captures present the third line is `RECORDED 5 OF 5 READY 5 OF 5` | Look up the standard-error line in `RUNNING.md` "Failure messages" |
| Developer | Open `http://127.0.0.1:8080/` in current Chrome or Firefox (`RUNNING.md` step 3) | The title `PURCHASE ORDER DISPLAY`, and the status line `MUMPS BACKEND: REPLAY RECORDED 0 OF 5 READY 0 OF 5`, or, with the five captures present, `MUMPS BACKEND: REPLAY RECORDED 5 OF 5 READY 5 OF 5` | `BRIDGE UNREACHABLE` or a blank page: open the page through that URL, never `file://`, and reload |
| Developer | `curl -s http://127.0.0.1:8080/api/status` (`RUNNING.md` step 6) | `{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}}`; with the five captures present, `{"mumps_mode":"replay","recorded":["GC-01","GC-02","GC-03","GC-04","GC-05"],"ready":["GC-01","GC-02","GC-03","GC-04","GC-05"],"incomplete":{}}` | Without the captures, a listed case means a case folder is present: use a clean checkout. With them, a case missing from `ready` is listed in `incomplete` with its failed checks: restore `tests/golden/ifcap/` from git, and never edit a capture |
| Developer | Select RUST and SEND an empty line, or run `curl -s -X POST -H 'Content-Type: application/json' -d '{"inputs":[""]}' http://127.0.0.1:8080/api/rust/session` (`RUNNING.md` step 6) | 200. Without `tests/fixtures/ifcap/cases.zwr`, the transcript ends after the station prompt `Select STATION NUMBER ('^' TO EXIT): 500// `, with its one trailing space, because the fixtures hold no `^PRC(442,0)` header (PO-09.b15; `RUNNING.md` step 4). With `cases.zwr` present, as committed on this branch, it ends after the order prompt `P.O./REQ.NO.: `, with its one trailing space | 500 `CORRUPT STORE <node>` names a fixture node: restore `tests/fixtures/ifcap/` from git and restart the bridge |
| Developer | Keep the sequence and select MUMPS, then COMPARE | MUMPS answers 502 `NOT RECORDED`, and COMPARE 502 `MUMPS: NOT RECORDED`. The bottom line shows the error body, such as `{"error":"NOT RECORDED"}`, and the transcript area stays empty, with the captures or without them, because no capture records the sequence `[""]` | A 200 means a capture folder records `[""]`, which none of the five committed captures does: restore `tests/golden/ifcap/` from git |

## Phase 6. Check failure handling and the idle screen

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | With the bridge running, run `cargo run --offline --locked --bin bridge` again in a second terminal | Standard error `bridge: port 8080 on 127.0.0.1 is already in use`, and exit status 1. The first bridge keeps serving | Look up the message in `RUNNING.md` "Failure messages" |
| Developer | Walk the `SPEC.md` "Front-end checklist" items that need no capture, the idle-screen identity included: load the page (MUMPS, nothing sent) and screenshot it at rest, then select RUST and screenshot it at rest | Each passes. Selecting RUST resends the empty sequence and shows the station prompt, or, with `cases.zwr` present, the order prompt `P.O./REQ.NO.: `. The two screens are pixel-identical with the backend selector, the status line and the transcript box masked (`SPEC.md` conflict K13), and the transcript box's computed styles are equal except `grid-template-rows`, which lists the row tracks its content creates | Record the failing checklist item with its browser. The identity's READY form needs a capture and is checked in phase 11 |
| Developer | Block `/api/status` in the browser's developer tools (Network panel, block request URL), reload the page, then unblock it | The status line reads `BRIDGE UNREACHABLE`, with no shake. A reload with the bridge stopped shows the browser's own connection error instead, because the bridge serves the page with `Cache-Control: no-store`; blocking the request fails it the same way | Record the failing checklist item |
| Developer | Stop the bridge (Ctrl-C), then SEND a line | The bottom line shows `REQUEST FAILED: <error>`, and the transcript stays as last rendered | Record the failing checklist item |

Phases 1 to 6 on a clean checkout without the captures give exit 0, five NOT COMPARED and `RECORDED 0 OF 5`. With the five captures present, phase 4 exits 0 with five PASS (`GC-01` to `GC-05`) and phase 5 shows `RECORDED 5 OF 5 READY 5 OF 5`.

## Phase 7. Prepare the capture environment

On the Docker host. Every Docker command is a README plan default that the team confirms (O-07).

| Owner | Do | Expect | If not |
|---|---|---|---|
| Capture owner | README Part B step 2 (a) to (c): `docker pull worldvista/vehu:202609`, then `docker image inspect --format '{{index .RepoDigests 0}}' worldvista/vehu:202609`, then `docker run -d --name vehu worldvista/vehu:202609` | A running container named `vehu`, and the digest recorded as `image_digest` (O-05) | Fix the Docker host or the image tag first: no later phase works without them |
| Capture owner | README Part B step 2 (d): load the records of README Part B step 1, order IEN 990001 `500-GC1001` (GC-01), 990002 `500-GC2002` (GC-02) and 990005 `500-GC5005` (GC-05) with vendor IEN 990001 `GC TEST VENDOR`, and confirm `GC3003` is absent from every `^PRC(442)` index | Every record meets the README Part B step 1 "Fixture profile" | A record outside the profile ends the Rust session at an n/e branch and its case FAILs. Load a record that meets it, and record any changed default in `meta.txt` `setup`; for GC-05, record the outcome as it is (README "GC-05 notes") |
| Capture owner | README Part B step 2 (e): `docker commit vehu vehu-gc-snapshot`, then `docker image inspect --format '{{.Id}}' vehu-gc-snapshot` | The id recorded as `snapshot_id`, the same for all five cases | Repeat (d) and (e) before any case runs |
| Capture owner | README Part B step 3: confirm `DUZ` `.5` and station 500 `CAMP MASTER`, and record the `IOF` and `IOSL` that `^%ZIS` sets at PRCHDP1.m:12 (the GC-01 transcript shows the `W @IOF` bytes). Open item O-01, which asked for them, is closed with `IOF` `#` and `IOSL` `60` | `IOF` and `IOSL` recorded for `context`, `#` and `60` in the committed captures, and `IOM` and the device name for `setup` | If `IOF` is cursor-addressed, such as `$C(27,91,50,74,27,91,72)`, apply the `SPEC.md` "Workflow selection" stop condition: stop and report to the team |
| Capture owner | Answer O-07 (README Part A section 7): a start command whose arguments contain no space, or a changed `docker_exec_args` convention | The answer recorded for the team | Live mode stays unverified, and replay stays the MUMPS backend. Captures still use the start command of README Part B step 1 |

## Phase 8. Capture the five cases

In each case's staging folder, `<staging>/GC-0N/`.

| Owner | Do | Expect | If not |
|---|---|---|---|
| Capture owner | README Part B step 4: restore the snapshot with `docker rm -f vehu && docker run -d --name vehu vehu-gc-snapshot`, export `db-before.zwr`, run `docker exec -i <container> <docker_exec_args> < input.txt > transcript.txt` from inside the case folder (the start command of README Part B step 1), export `db-after.zwr`, then write `: > returned.txt` and `meta.txt` | `input.txt`, `transcript.txt`, a zero-byte `returned.txt`, `db-before.zwr`, `db-after.zwr`, and `meta.txt` with its eight keys (README Part A section 5) | Record any other difference between the two exports in `meta.txt` `setup`; the case is then not READY |
| Capture owner | README Part B step 5: run the case again from the restored snapshot, and compare the two runs | A `normalisation.txt` covering only the variable fields `meta.txt` documents, zero bytes when none: seven files | A case that is not reproducible keeps no `normalisation.txt`, and `meta.txt` `setup` gives the reason. It reports INCOMPLETE |
| Capture owner | Copy `<staging>/GC-01` to `GC-05` into `tests/golden/ifcap/`, then build `tests/fixtures/ifcap/cases.zwr` by README Part B step 6 | Five case folders of seven files each, and one `cases.zwr` | Never edit a capture file or an existing `cases.zwr`: record the case again. An existing `cases.zwr` that lacks a case's records is checked by the README Part B step 6 preflight, which names the missing nodes, and is then rebuilt whole by README Part B step 6, never edited |

## Phase 9. Check readiness and rerun

README Part B step 7.

| Owner | Do | Expect | If not |
|---|---|---|---|
| Developer | `cargo test --offline --locked` | `REPORT.md` part 2 gives each case's verdict, and part 3 shows each case `READY`. The test also prints one `GC-0N <verdict>` line per case, which Cargo shows only when the test fails; to see the lines on a passing run, use `cargo test --offline --locked --test equivalence -- --nocapture` (`RUNNING.md` step 2). Exit 0 only when no case is FAIL (README Part A section 9). With the five captures of this branch: exit 0, all five cases PASS, and each `READY` | Part 3 shows `INCOMPLETE: <check names>`: go to phase 10 |
| Developer | Restart the bridge with `cargo run --offline --locked --bin bridge`, then reload the page | The third startup line `RECORDED 5 OF 5 READY 5 OF 5`, and the status line `MUMPS BACKEND: REPLAY RECORDED 5 OF 5 READY 5 OF 5` | `curl -s http://127.0.0.1:8080/api/status` lists each INCOMPLETE case with its failed checks |
| Developer | Record the capture hashes: `find tests/golden/ifcap -type f ! -name README.md -exec sha256sum {} +`, then `if [ -f tests/fixtures/ifcap/cases.zwr ]; then sha256sum tests/fixtures/ifcap/cases.zwr; else echo "ABSENT tests/fixtures/ifcap/cases.zwr"; fi` | One hash per capture file, and one for `cases.zwr` | A missing file or `ABSENT tests/fixtures/ifcap/cases.zwr`: back to phase 8 |

## Phase 10. Read the outcome

| Outcome | Owner | Do | Expect | If not |
|---|---|---|---|---|
| All five PASS | Reviewer | Confirm the five PASS blocks in `REPORT.md` part 2 | Each shows its `image_digest` and `snapshot_id` | Treat the case by the matching row below |
| Any FAIL | Reviewer, then Developer | Read the cause in `REPORT.md` part 2; for a transcript, the first differing line, both texts and its `TAG^ROUTINE`. Re-read the cited routine line, and correct the port only where a cited source line supports it, at most three attempts per case | The case PASSes on the next `cargo test --offline --locked` | After the third attempt, report FAIL with routine, tag, first differing line number and both texts. A difference source cannot explain becomes a new `SPEC.md` "Open items" entry. Never edit a capture, `normalisation.txt` or `cases.zwr`, and never add a rule to hide a difference |
| INCOMPLETE | Capture owner | Read the failed checks in `REPORT.md` part 3 (README Part A section 9), and record the case again (phase 8) | The case reports READY | It stays out of acceptance, and `meta.txt` `setup` gives the reason |
| `NOT RECORDED` | Developer | Check that the inputs sent equal a recorded case's `input.txt` in full, and that its case folder exists in the golden folder | MUMPS answers 200 | Record the case (phase 8). `NOT RECORDED (unreadable: <path>: <cause>)`: copy the capture again as it was recorded |
| `CONTEXT MISMATCH <key>: expected <value or ABSENT>, actual <value or ABSENT>` in live mode | Capture owner | Compare `meta.txt` `context` with `STUB_CONTEXT` (README Part A section 6) | Exactly the keys `DUZ` `.5`, `IOF`, `IOSL` and `STATION` `500` | Record the case again; never edit its `meta.txt` |
| `bridge: port 8080 on 127.0.0.1 is already in use` | Developer | Stop the other process on the port, then start the bridge again | The three startup lines | Look up the message in `RUNNING.md` "Failure messages" |
| Container not running: `EXIT <code>: <first line of docker's standard error>` | Capture owner | Start it with `docker rm -f vehu && docker run -d --name vehu vehu-gc-snapshot` (README Part B step 2 (f)), then restart the bridge with `VEHU_CONTAINER=<container name> cargo run --offline --locked --bin bridge` | MUMPS answers again | Correct `VEHU_CONTAINER`. Live mode is unverified (O-07), and replay stays the MUMPS backend |

## Phase 11. Check the three modes and live mode

| Owner | Do | Expect | If not |
|---|---|---|---|
| Reviewer | Run each case, GC-01 to GC-05, in MUMPS, RUST and COMPARE (`RUNNING.md` step 6) in current Chrome and Firefox, and try the RUST reactions | Each MUMPS transcript equals its capture, COMPARE shows `MATCH` for every passing case, and the 14 items of `SPEC.md` "Front-end checklist" pass | Record the failing item with its browser, case and mode |
| Capture owner | Stop the replay bridge left running since phase 9 (Ctrl-C in its terminal): only one bridge runs at a time, because each binds the fixed `127.0.0.1:8080` (`RUNNING.md` step 3). Then start live mode with `VEHU_CONTAINER=<container name> cargo run --offline --locked --bin bridge` (`RUNNING.md` step 7), and reload the page | The second startup line `MUMPS MODE: LIVE <container>`, and the status line `MUMPS BACKEND: LIVE …` | `bridge: port 8080 on 127.0.0.1 is already in use` means the replay bridge is still running: stop it (phase 10, that message's row) and start live mode again. Look up any other message in `RUNNING.md` "Failure messages", and record the result: live mode is delivered unverified (O-07), and replay stays the MUMPS backend |
| Capture owner | Select MUMPS and run GC-01 once, sending the lines of its `input.txt` (`RUNNING.md` step 6). Compare the live transcript with the replayed GC-01 transcript of this phase's first row, which equals `tests/golden/ifcap/GC-01/transcript.txt` | A live GC-01 transcript equal to the replayed one | Look up a 502 body in `RUNNING.md` "Failure messages". Record the result: live mode is delivered unverified (O-07), and replay stays the MUMPS backend |
| Capture owner | Return to replay: stop the live bridge (Ctrl-C), start `cargo run --offline --locked --bin bridge` (`RUNNING.md` step 2), and reload the page | The second startup line `MUMPS MODE: REPLAY`, and the status line `MUMPS BACKEND: REPLAY …` with the counts of phase 9 | `bridge: port 8080 on 127.0.0.1 is already in use` means the live bridge is still running: stop it, then start replay again |

## Phase 12. Record acceptance

| Owner | Do | Expect | If not |
|---|---|---|---|
| Reviewer | `SPEC.md` "Acceptance record" begins `Not accepted`, states the observed result and the commit it was observed on, and leaves its `Date:`, `image_digest:` and `snapshot_id:` fields blank. With five PASS, record the acceptance there, filling the three fields with the date, `image_digest` and `snapshot_id`, then run `cargo test --offline --locked` | `REPORT.md` "Acceptance record" carries the record (conflict K5): `tests/equivalence.rs` copies it, and nothing else writes `REPORT.md` | Without five PASS, leave the record at `Not accepted`, naming the cases that are not PASS, with its three fields blank. Exit 0 alone is never acceptance |
