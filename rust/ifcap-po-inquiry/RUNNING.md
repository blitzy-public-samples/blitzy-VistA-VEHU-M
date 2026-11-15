# Running the purchase-order inquiry port

This crate is a read-only, standard-library-only Rust port of OPTION `PRCH DISPLAY PO` (routine `EN3^PRCHRPT`) on `worldvista/vehu:202609`, with a loopback screen that runs one input sequence against the original and the port. Every command below runs from `rust/ifcap-po-inquiry/`.

- This file is the single source of the build and run commands. `testingMethods.md`, `runPlaybook.md` and `tests/golden/ifcap/README.md` quote them verbatim.
- Replace each `<…>` with a real value.
- Message texts are quoted as the code prints them. Where this file and the code differ, the code governs.

## Step 1. Prerequisites

- **Rust 1.99.1:** `rustup toolchain install 1.99.1`. `rust-toolchain.toml` pins channel `1.99.1`, and `Cargo.toml` sets `rust-version = "1.99.1"`.
- **A system C linker, `cc`**, for example from gcc. rustup's minimal toolchain ships no linker and links the binary and the tests with it.
- **No crates.io access.** `[dependencies]` is empty and there is no `[dev-dependencies]`, so the crate builds offline against the committed `Cargo.lock`.
- **Docker** is needed only for live mode (step 7) and for recording captures (step 5).
- **A browser**, current Chrome or Firefox, is needed only for the screen (step 3).

## Step 2. Build, test, run

```text
cargo build --offline --locked
cargo test --offline --locked
cargo run --offline --locked --bin bridge
```

- Set `RUSTUP_AUTO_INSTALL=0` when offline, so that a missing pinned toolchain fails instead of downloading.
- The short forms `cargo test` and `cargo run --bin bridge` are equivalent on a prepared host.
- **`cargo build`** exits 0.
- **`cargo test`** runs the unit tests, `tests/bridge.rs` and `tests/equivalence.rs`.
  - It exits 0 when no case is FAIL and nothing else fails. With no capture, all five cases, `GC-01` to `GC-05`, are NOT COMPARED, so none is FAIL and it exits 0.
  - `tests/equivalence.rs` rewrites `REPORT.md` in full on every run. Part 2 holds each case's verdict and part 3 its capture readiness.
  - The test also prints one `GC-0N <verdict>` line per case, such as `GC-01 NOT COMPARED`. Cargo shows test output only for a failing test, so to see the lines on a passing run use `cargo test --offline --locked --test equivalence -- --nocapture`.
  - It exits non-zero when any case is FAIL, and `REPORT.md` part 2 names the cause. With no case FAIL, it still exits non-zero when a harness check fails (`REPORT.md` part 1 lists each under "Harness checks in this run" with PASS or FAIL), when `SPEC.md` lacks a section the report includes (`MISSING SECTION <heading> IN SPEC.md`, with no report written), when `REPORT.md` cannot be written (`cannot write REPORT.md: <error>`), or when any other test or the build fails.
  - Exit 0 is never acceptance. Acceptance needs five PASS.
- **`cargo run`** starts the bridge, which serves until it is stopped (Ctrl-C). Once listening, it writes three lines to standard output:

  ```text
  http://127.0.0.1:8080/
  MUMPS MODE: REPLAY
  RECORDED 0 OF 5 READY 0 OF 5
  ```

  The third line counts the recorded and READY cases in the golden folder at startup. Its diagnostics go to standard error (see "Failure messages").

## Step 3. Open the screen

- Open `http://127.0.0.1:8080/` in current Chrome or Firefox. `file://` is not supported, because the page talks only to the bridge that served it.
- The bridge binds `127.0.0.1:8080` only, with no other address or port, and has no authentication.
- The screen shows the title `PURCHASE ORDER DISPLAY`, the status line, the BACKEND selector (MUMPS, RUST, COMPARE), the 80 by 24 transcript, and INPUT with SEND and EXIT.
- The status line is fetched once, when the page loads, and never refreshed: it keeps its counts when captures are added and when the bridge stops. Reload the page to refresh it after adding captures or restarting the bridge.

## Step 4. With no captures

This is the state of a fresh checkout: `tests/golden/ifcap/` holds only its README, and `tests/fixtures/ifcap/cases.zwr` does not exist.

- The bridge's third startup line is `RECORDED 0 OF 5 READY 0 OF 5`.
- The status line reads `MUMPS BACKEND: REPLAY RECORDED 0 OF 5 READY 0 OF 5`.
- **MUMPS** answers 502 `NOT RECORDED`. **COMPARE** answers 502 `MUMPS: NOT RECORDED`. The screen shows the error body in its bottom line, for example `{"error":"NOT RECORDED"}`, and leaves the transcript as it was.
- **RUST** answers 200, and its transcript ends after the station prompt `Select STATION NUMBER ('^' TO EXIT): 500// `, which ends with one space.
  - The repository fixtures hold no file 442 header `^PRC(442,0)`, so the order lookup ends before it writes its prompt (branch PO-09.b15).
  - The original on VEHU never reaches that state (conflict K6), so no Rust transcript reaches the order prompt until `cases.zwr` is loaded.
- **Once `tests/fixtures/ifcap/cases.zwr` is loaded** and the bridge restarted, the Rust side reaches the order prompt `P.O./REQ.NO.: `. The header `^PRC(442,0)`, with piece 2 `442I`, is what this needs.
  - The original's no-match response is `<BEL> ??`, a blank line, and the re-asked `P.O./REQ.NO.: `. `<BEL>` is byte 0x07. The Rust side writes it only over a `cases.zwr` built by README Part B step 6, which holds the header and the orders' index nodes. Those give `^PRC(442)` a subscript after `"A["`, such as `"C"`, as the README Part B step 1 fixture profile requires. Over a `cases.zwr` holding only the header, an absent order ends the session after `P.O./REQ.NO.: ` with no no-match response, at branch PO-09.b16, which the port does not implement.
  - It is written for GC-03's `GC3003`, GC-04's `INVALID!` and any other absent value the O-03 guard admits (`SPEC.md` R5 step 6): a value with no lowercase letter and no comma, of at most 30 characters, that is not a canonical number, is neither `S` nor `1` nor a prefix of `SUPPLY WAREHOUSE` or `INACTIVATED`, and neither equals nor begins any subscript of an index of files 440, 442.5 or 442.3, while `^PRC(442,"G")`, `("M")` and `("MCH")` hold no entry, as the fixture profile requires.
  - Any other absent order, for example the number `123456` or the lowercase `gc3003`, reaches a branch the port does not implement: PO-09.b11, or PO-09.b13 when the uppercase retry finds a candidate. The session then ends after `P.O./REQ.NO.: `, as when input runs out.
  - While open item O-01 (the HOME device) is unanswered, the Rust session for an order that is found ends after `P.O./REQ.NO.: `.

## Step 5. Load captures

- The team records the captures on its Docker host by `tests/golden/ifcap/README.md` Part B, which also holds the conventions. Each case runs from inside its own case folder:

  ```text
  docker exec -i <container> <docker_exec_args> < input.txt > transcript.txt
  ```

- Copy the case folders `GC-01` to `GC-05`, each holding the seven files, into `tests/golden/ifcap/`.
- Build `tests/fixtures/ifcap/cases.zwr` by README Part B step 6.
- Run `cargo test --offline --locked`. It judges every recorded case and rewrites `REPORT.md`.
- **Restart the bridge**, because it loads the fixture Store once, at startup. Replay rereads the capture folders on every request, so new capture folders alone need no restart, but `cases.zwr` and the startup counts do. The screen's status line is read only when the page loads, so reload the page to refresh it after adding captures, restart or not.
- To serve another capture folder, start the bridge with:

  ```text
  GOLDEN_DIR=<folder> cargo run --offline --locked --bin bridge
  ```

  The default is `golden::default_dir()`, which is `tests/golden/ifcap`. `GOLDEN_DIR` affects only the bridge: the tests always read `tests/golden/ifcap`.

## Step 6. Run the three modes

- Choose MUMPS, RUST or COMPARE in the selector. Type one input in INPUT and press SEND, or RETURN, for each input line of a case. An empty line is a RETURN at the prompt. EXIT sends `^` and empties INPUT without sending what was typed there.
- Typed input is never echoed. The transcript shows only what the backend writes.
- Every SEND posts the whole sequence typed so far, because the bridge keeps no session state. Changing the backend clears the screen and sends the same sequence to the new backend. Reloading the page starts a new, empty sequence.
- **MUMPS** replays the capture whose inputs equal the whole sequence, so a sequence that is only part of a case answers `NOT RECORDED` until the case's last input.
- **RUST** answers from the fixtures through the port.
- **COMPARE** shows both raw transcripts side by side.
  - It shows `MATCH` only when the capture for the inputs is READY, its `meta.txt` `context` is accepted, the two transcripts are equal after that case's normalisation, and the two return values are equal.
  - It shows `MISMATCH` otherwise, inputs with no capture included.
  - It shows nothing before a compare, while a compare request is in flight, and after an error.
- Example, GC-04: SEND an empty line, type `INVALID!` and SEND, then EXIT.
- The same answers come from the command line while the bridge runs:

  ```text
  curl -s http://127.0.0.1:8080/api/status
  curl -s -X POST -H 'Content-Type: application/json' -d '{"inputs":[""]}' http://127.0.0.1:8080/api/rust/session
  ```

  `GET /api/status` reports the mode and the recorded, READY and INCOMPLETE cases, with each INCOMPLETE case's failed checks. With no captures it answers `{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}}`. A RUST session request answers 200 `{"transcript":"…","returned":null}`, and so does a MUMPS one, with `mumps` in place of `rust`. A COMPARE request, with `compare` in place of `rust`, answers 200 `{"mumps":{"transcript":"…","returned":null},"rust":{"transcript":"…","returned":null},"match":false}`: both raw outputs, never normalised, and the boolean `match`, which is `true` when the screen shows `MATCH`. `returned` is always `null`, because the workflow returns nothing. An error answers `{"error":"<cause>"}` (see "Failure messages").

## Step 7. Live mode

```text
VEHU_CONTAINER=<container name> cargo run --offline --locked --bin bridge
```

- The second startup line becomes `MUMPS MODE: LIVE <container>`, and the status line reads `MUMPS BACKEND: LIVE …`. An empty `VEHU_CONTAINER` means replay.
- Live mode needs Docker, a running container of that name, and at least one case folder whose `meta.txt` parses.
- For each MUMPS request it runs the program `docker` directly, with no shell, as `docker exec -i <container> <docker_exec_args>`.
  - The arguments and `context` come from the first case folder, in `GC-01` to `GC-05` order, whose `meta.txt` parses.
  - The `context` is checked before anything runs.
  - Standard input is each input followed by one newline, and standard output is the transcript. The return value is always `null`.
  - At most 4 runs go at once. Each is killed after 10 s, and its output is limited to 1 MiB.
  - The bridge kills only the `docker` process it started. A process `docker` started, such as the child of a wrapper script installed as `docker` or the command `docker exec` runs inside the container, is not signalled. It can run on after the run ends, and it is not counted in the 4 runs.
  - While such a process still holds `docker`'s standard output or standard error, the output has not ended, so the run ends `TIMEOUT` at 10 s even when `docker` exited 0.
- COMPARE's `MATCH` still needs a READY capture for the inputs. A live answer with no capture shows `MISMATCH`.
- **Live mode is delivered unverified.**
  - The documented docker-vista start command needs an argument containing spaces, its `-c` script, which the single-space `docker_exec_args` convention cannot express (open item O-07).
  - Replay is the delivered MUMPS backend.

## Failure messages

| Message | Meaning | What to do |
|---|---|---|
| `bridge: port 8080 on 127.0.0.1 is already in use` | Standard error at startup. Another process, such as an earlier bridge, holds the port. Exit status 1 | Stop the other process, then start the bridge again |
| `bridge: cannot listen on 127.0.0.1:8080: <error>` | Standard error at startup. Any other bind failure, named by `<error>`. Exit status 1 | Correct the cause `<error>` names |
| `bridge: golden directory missing: <path>` | Standard error at startup. `GOLDEN_DIR`, or the default folder, does not exist, or is not a directory. The bridge keeps serving, the third line reads `RECORDED 0 OF 5 READY 0 OF 5`. In replay MUMPS answers 502 `NOT RECORDED`; in live mode it answers 502 `NO META.TXT IN <path>` | Correct `GOLDEN_DIR`, or create the folder, and restart |
| `bridge: golden directory unreadable: <path>: <error>` | Standard error at startup. The metadata of `GOLDEN_DIR`, or the default folder, cannot be read for a reason other than its absence, named by `<error>`, such as a symbolic-link loop or a parent folder the bridge user cannot search. The bridge keeps serving, the third line reads `RECORDED 5 OF 5 READY 0 OF 5`, and the status lists all five cases recorded and INCOMPLETE with all six checks. In replay MUMPS answers 502 `NOT RECORDED (unreadable: <path>/GC-01: <cause>)`, and COMPARE puts `MUMPS: ` before it; in live mode it answers 502 `NO META.TXT IN <path>` | Give the bridge user search access to the folder and its parents, or correct `GOLDEN_DIR`, and restart |
| `bridge: cannot load fixtures: <error>` | Standard error, after the three startup lines and before anything is served. A file under `tests/fixtures/ifcap/` cannot be read (`<path>: cannot read: <cause>`), an entry named `*.zwr` there is not a regular file, such as a directory or a FIFO (`<path>: cannot read: not a regular file`), or a file holds a malformed line (`<path>: MALFORMED ZWR LINE <line>: <message>`). Exit status 1 | Restore the supporting fixture from git, and remove any entry named `*.zwr` that is not a fixture file. Rebuild `cases.zwr` by README Part B step 6. Never edit an export |
| `bridge: serving failed: <error>` | Standard error, after the three startup lines, once the fixtures have loaded, usually after the bridge has served. Serving stopped because accepting a connection failed in a way no retry mends (`accepting a connection failed: <cause>`), a connection could not be given its socket settings (`configuring a connection failed: <cause>`), or the listener could not be made non-blocking. Running out of open files, threads or memory does not stop serving: the bridge pauses accepting, at most 1 s at a time, closes without a response a connection it cannot give a thread, and serves again once open connections end. The fixtures are not at fault. Exit status 1 | Correct the cause `<error>` names, then start the bridge again |
| `NOT RECORDED` | 502 from MUMPS in replay: no recorded case's inputs equal the whole sequence. COMPARE gives `MUMPS: NOT RECORDED` | Type the full sequence of a recorded case, or record the capture (step 5) |
| `NOT RECORDED (unreadable: <path>: <cause>)` | 502 in replay: a capture file that is present cannot be read, is larger than its size limit or is not UTF-8, or its case folder cannot be inspected | Copy the capture again as it was recorded. Never edit its content |
| `CONTEXT MISMATCH <key>: expected <value or ABSENT>, actual <value or ABSENT>` | The `meta.txt` `context` disagrees with `STUB_CONTEXT`, for example `CONTEXT MISMATCH DUZ: expected .5, actual 1`. In live mode it is 502 before any command runs. In replay COMPARE shows `MISMATCH`, and `cargo test` reports the case FAIL | Check the capture against README Part A section 6, and record it again if it is wrong |
| `NO META.TXT IN <golden_dir>` | 502 in live mode: no case folder holds a `meta.txt` that parses | Load captures with their `meta.txt` (step 5) |
| `EXIT <code>: <first line of docker's standard error>` | 502 in live mode: `docker` exited non-zero, for example because the container is not running or not found. The bridge keeps serving | Start the container, or correct `VEHU_CONTAINER`, and restart the bridge |
| `CANNOT RUN docker: <error>` | 502 in live mode: the bridge could not start the program `docker`. `<error>` is the system's reason, such as `No such file or directory (os error 2)` when Docker is not installed or `docker` is not on the `PATH` the bridge was started with, or `Permission denied (os error 13)`. Nothing ran, and the bridge keeps serving. COMPARE gives `MUMPS: CANNOT RUN docker: <error>` | Install Docker, or start the bridge with `docker` on its `PATH` |
| `TIMEOUT`, `OUTPUT TOO LARGE`, `NON-UTF8`, `EXIT NONE: <first line of docker's standard error>` | 502 in live mode: no run slot freed or the run passed 10 s, including a run whose output a process `docker` started still held open after `docker` exited (step 7); the output passed 1 MiB; the output is not UTF-8; `docker` was ended by a signal | Check the container and the start arguments in `meta.txt`. Only `docker` itself is killed, so a process it started can outlive the run: find it and stop it by hand |
| `BRIDGE UNREACHABLE` | The screen's status line: `GET /api/status` failed, answered non-2xx, or returned a body that is not the status JSON | Start the bridge, open the page through `http://127.0.0.1:8080/`, and reload |
| `REQUEST FAILED: <error>` | The screen's bottom line: a session request failed before a complete answer arrived. Either `fetch` failed, for example because the bridge is not running or the connection broke, or the answer's body could not be read. The request may have reached the bridge and run. `<error>` is the browser's error text, or `UNKNOWN ERROR` when the error cannot be turned into text. The transcript stays as it was, COMPARE's verdict line is emptied, and in RUST mode the transcript shakes as for any rejection. The status line above is not re-read, so it keeps the counts it read when the page loaded | Make sure the bridge is running, and start it if not. The failed input is already in the sequence and SEND would add another, so change the backend and back to resend the sequence as it stands. Resending is safe: every request is read-only and the bridge keeps no session state |

Other HTTP answers. Every error body is `{"error":"<cause>"}`, and the screen shows it verbatim in its bottom line.

- **400.**
  - A malformed request gives `BAD REQUEST: <rule>`, such as `BAD REQUEST: CONTENT-LENGTH REQUIRED`.
  - `BAD REQUEST: HOST NOT ALLOWED` on any path: the `Host` header, or the host of an absolute `http://…` request target, is not `127.0.0.1`, `localhost` or `[::1]` (any port). The bridge answers only requests addressed to the loopback host, so a page whose name was made to resolve to `127.0.0.1` reaches nothing. Open the page through `http://127.0.0.1:8080/` or `http://localhost:8080/`.
  - `BAD REQUEST: ORIGIN NOT ALLOWED` on a session `POST`: it carries an `Origin` other than the page's own, `http://` followed by the request's `Host`, such as a page served from another host or port, `null` or `https://…`. A request without `Origin`, such as `curl`'s, is not refused for it.
  - `BAD REQUEST: CONTENT-TYPE APPLICATION/JSON REQUIRED` on a session `POST`: it carries no `Content-Type`, more than one, or one whose media type is not `application/json`, such as `text/plain` or a form type. A parameter such as `; charset=utf-8` is accepted. The screen and the `curl` lines above send `Content-Type: application/json`.
  - A session body that is not `{"inputs":[string, …]}` gives one of: `BAD REQUEST: MALFORMED JSON`, `BAD REQUEST: BODY IS NOT A JSON OBJECT`, `BAD REQUEST: INPUTS MISSING OR REPEATED`, `BAD REQUEST: INPUTS IS NOT AN ARRAY`, `BAD REQUEST: INPUT <n> IS NOT A STRING`, `BAD REQUEST: INPUT <n> CONTAINS A CARRIAGE RETURN OR LINE FEED`.
- **404.** `NOT FOUND` for an unknown path, `UNKNOWN BACKEND` for a backend other than `mumps`, `rust` or `compare`, and `NOT FOUND: POST REQUIRED` or `NOT FOUND: GET REQUIRED` for the wrong method.
- **500.** `CORRUPT STORE <node>` from RUST: the inquiry read a node that holds no value where the original raises an M `UNDEF` error, such as `CORRUPT STORE ^PRC(442,1,0)`. Check `cases.zwr` against README Part B step 6. COMPARE answers it as 502 `RUST: CORRUPT STORE <node>`.
- **502.** Every MUMPS-side failure above. COMPARE puts `MUMPS: ` before it.
