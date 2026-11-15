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
  - It exits 0 when no case is FAIL. With no capture, all five cases, `GC-01` to `GC-05`, are NOT COMPARED.
  - `tests/equivalence.rs` rewrites `REPORT.md` in full on every run. Part 2 holds each case's verdict and part 3 its capture readiness.
  - The test also prints one `GC-0N <verdict>` line per case, such as `GC-01 NOT COMPARED`. Cargo shows test output only for a failing test, so to see the lines on a passing run use `cargo test --offline --locked --test equivalence -- --nocapture`.
  - It exits non-zero when any case is FAIL, and `REPORT.md` part 2 names the cause.
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
- The status line is fetched once, when the page loads. After restarting the bridge, reload the page.

## Step 4. With no captures

This is the state of a fresh checkout: `tests/golden/ifcap/` holds only its README, and `tests/fixtures/ifcap/cases.zwr` does not exist.

- The bridge's third startup line is `RECORDED 0 OF 5 READY 0 OF 5`.
- The status line reads `MUMPS BACKEND: REPLAY RECORDED 0 OF 5 READY 0 OF 5`.
- **MUMPS** answers 502 `NOT RECORDED`. **COMPARE** answers 502 `MUMPS: NOT RECORDED`. The screen shows the error body in its bottom line, for example `{"error":"NOT RECORDED"}`, and leaves the transcript as it was.
- **RUST** answers 200, and its transcript ends after the station prompt `Select STATION NUMBER ('^' TO EXIT): 500// `, which ends with one space.
  - The repository fixtures hold no file 442 header `^PRC(442,0)`, so the order lookup ends before it writes its prompt (branch PO-09.b15).
  - The original on VEHU never reaches that state (conflict K6), so no Rust transcript reaches the order prompt until `cases.zwr` is loaded.
- **Once `tests/fixtures/ifcap/cases.zwr` is loaded** and the bridge restarted, the Rust side reaches the order prompt `P.O./REQ.NO.: `.
  - An order absent from the fixtures gets the original's no-match response: `<BEL> ??`, a blank line, and the re-asked `P.O./REQ.NO.: `. `<BEL>` is byte 0x07.
  - While open item O-01 (the HOME device) is unanswered, the Rust session for an order that is found ends after `P.O./REQ.NO.: `.

## Step 5. Load captures

- The team records the captures on its Docker host by `tests/golden/ifcap/README.md` Part B, which also holds the conventions. Each case runs from inside its own case folder:

  ```text
  docker exec -i <container> <docker_exec_args> < input.txt > transcript.txt
  ```

- Copy the case folders `GC-01` to `GC-05`, each holding the seven files, into `tests/golden/ifcap/`.
- Build `tests/fixtures/ifcap/cases.zwr` by README Part B step 6.
- Run `cargo test --offline --locked`. It judges every recorded case and rewrites `REPORT.md`.
- **Restart the bridge**, because it loads the fixture Store once, at startup. Replay rereads the capture folders on every request, so new capture folders alone need no restart, but `cases.zwr` and the startup counts do.
- To serve another capture folder, start the bridge with:

  ```text
  GOLDEN_DIR=<folder> cargo run --offline --locked --bin bridge
  ```

  The default is `golden::default_dir()`, which is `tests/golden/ifcap`. `GOLDEN_DIR` affects only the bridge: the tests always read `tests/golden/ifcap`.

## Step 6. Run the three modes

- Choose MUMPS, RUST or COMPARE in the selector. Type one input in INPUT and press SEND, or RETURN, for each input line of a case. An empty line is a RETURN at the prompt. EXIT sends `^`.
- Typed input is never echoed. The transcript shows only what the backend writes.
- Every SEND posts the whole sequence typed so far, because the bridge keeps no session state. Changing the backend clears the screen and sends the same sequence to the new backend. Reloading the page starts a new, empty sequence.
- **MUMPS** replays the capture whose inputs equal the whole sequence, so a sequence that is only part of a case answers `NOT RECORDED` until the case's last input.
- **RUST** answers from the fixtures through the port.
- **COMPARE** shows both raw transcripts side by side.
  - It shows `MATCH` only when the capture for the inputs is READY, its `meta.txt` `context` is accepted, the two transcripts are equal after that case's normalisation, and the two return values are equal.
  - It shows `MISMATCH` otherwise, inputs with no capture included.
  - It shows nothing before a compare and after an error.
- Example, GC-04: SEND an empty line, type `INVALID!` and SEND, then EXIT.
- The same answers come from the command line while the bridge runs:

  ```text
  curl -s http://127.0.0.1:8080/api/status
  curl -s -X POST -H 'Content-Type: application/json' -d '{"inputs":[""]}' http://127.0.0.1:8080/api/rust/session
  ```

  `GET /api/status` reports the mode and the recorded, READY and INCOMPLETE cases, with each INCOMPLETE case's failed checks. With no captures it answers `{"mumps_mode":"replay","recorded":[],"ready":[],"incomplete":{}}`. A session request answers `{"transcript":"…","returned":null}`; replace `rust` with `mumps` or `compare` for the other backends.

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
- COMPARE's `MATCH` still needs a READY capture for the inputs. A live answer with no capture shows `MISMATCH`.
- **Live mode is delivered unverified.**
  - The documented docker-vista start command needs an argument containing spaces, its `-c` script, which the single-space `docker_exec_args` convention cannot express (open item O-07).
  - Replay is the delivered MUMPS backend.

## Failure messages

| Message | Meaning | What to do |
|---|---|---|
| `bridge: port 8080 on 127.0.0.1 is already in use` | Standard error at startup. Another process, such as an earlier bridge, holds the port. Exit status 1 | Stop the other process, then start the bridge again |
| `bridge: cannot listen on 127.0.0.1:8080: <error>` | Standard error at startup. Any other bind failure, named by `<error>`. Exit status 1 | Correct the cause `<error>` names |
| `bridge: VEHU_CONTAINER is not valid UTF-8` | Standard error at startup. Exit status 1 | Set a valid container name, or unset the variable for replay |
| `bridge: golden directory missing: <path>` | Standard error at startup. `GOLDEN_DIR`, or the default folder, is not a directory. The bridge keeps serving, the third line reads `RECORDED 0 OF 5 READY 0 OF 5`, and MUMPS answers 502 `NOT RECORDED` | Correct `GOLDEN_DIR`, or create the folder, and restart |
| `bridge: cannot load fixtures: <error>` | Standard error. A file under `tests/fixtures/ifcap/` cannot be read (`<path>: cannot read: <cause>`) or holds a malformed line (`<path>: MALFORMED ZWR LINE <line>: <message>`). Exit status 1 | Restore the supporting fixture from git. Rebuild `cases.zwr` by README Part B step 6. Never edit an export |
| `NOT RECORDED` | 502 from MUMPS in replay: no recorded case's inputs equal the whole sequence. COMPARE gives `MUMPS: NOT RECORDED` | Type the full sequence of a recorded case, or record the capture (step 5) |
| `NOT RECORDED (unreadable: <path>: <cause>)` | 502 in replay: a capture file that is present cannot be read or is not UTF-8 | Copy the capture again as it was recorded. Never edit its content |
| `CONTEXT MISMATCH <key>: expected <value or ABSENT>, actual <value or ABSENT>` | The `meta.txt` `context` disagrees with `STUB_CONTEXT`, for example `CONTEXT MISMATCH DUZ: expected .5, actual 1`. In live mode it is 502 before any command runs. In replay COMPARE shows `MISMATCH`, and `cargo test` reports the case FAIL | Check the capture against README Part A section 6, and record it again if it is wrong |
| `NO META.TXT IN <golden_dir>` | 502 in live mode: no case folder holds a `meta.txt` that parses | Load captures with their `meta.txt` (step 5) |
| `EXIT <code>: <first line of docker's standard error>` | 502 in live mode: `docker` exited non-zero, for example because the container is not running or not found. The bridge keeps serving | Start the container, or correct `VEHU_CONTAINER`, and restart the bridge |
| `TIMEOUT`, `OUTPUT TOO LARGE`, `NON-UTF8`, `EXIT NONE: <stderr head>` | 502 in live mode: no run slot freed or the run passed 10 s; the output passed 1 MiB; the output is not UTF-8; `docker` was ended by a signal | Check the container and the start arguments in `meta.txt` |
| `BRIDGE UNREACHABLE` | The screen's status line: `GET /api/status` failed, answered non-2xx, or returned a body that is not the status JSON | Start the bridge, open the page through `http://127.0.0.1:8080/`, and reload |
| `REQUEST FAILED: <error>` | The screen's bottom line: a session request never reached the bridge | Start the bridge, then SEND again |

Other HTTP answers. Every error body is `{"error":"<cause>"}`, and the screen shows it verbatim in its bottom line.

- **400.**
  - A malformed request gives `BAD REQUEST: <rule>`, such as `BAD REQUEST: CONTENT-LENGTH REQUIRED`.
  - A session body that is not `{"inputs":[string, …]}` gives one of: `MALFORMED JSON`, `BODY IS NOT A JSON OBJECT`, `INPUTS MISSING OR REPEATED`, `INPUTS IS NOT AN ARRAY`, `INPUT <n> IS NOT A STRING`, `INPUT <n> CONTAINS A CARRIAGE RETURN OR LINE FEED`.
- **404.** `NOT FOUND` for an unknown path, `UNKNOWN BACKEND` for a backend other than `mumps`, `rust` or `compare`, and `NOT FOUND: POST REQUIRED` or `NOT FOUND: GET REQUIRED` for the wrong method.
- **500.** `CORRUPT STORE <node>` from RUST: the inquiry read a node that holds no value where the original raises an M `UNDEF` error, such as `CORRUPT STORE ^PRC(442,1,0)`. Check `cases.zwr` against README Part B step 6. COMPARE answers it as 502 `RUST: CORRUPT STORE <node>`.
- **502.** Every MUMPS-side failure above. COMPARE puts `MUMPS: ` before it.
