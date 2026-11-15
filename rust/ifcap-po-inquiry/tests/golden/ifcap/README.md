# Golden captures: PRCH DISPLAY PO (EN3^PRCHRPT)

This folder holds the golden captures of OPTION `PRCH DISPLAY PO` (IEN 1901, routine `EN3^PRCHRPT`) [Packages/Kernel/Globals/19+OPTION.zwr:14687-14691]. The team records one folder per case, `GC-01/` to `GC-05/` (`golden::CASE_IDS`), on `worldvista/vehu:202609`, and the Rust port is judged against them.

- **Baseline.** The folder holds only this README: no case folder, no `.gitkeep` and no placeholder. Every case is ABSENT, and therefore NOT COMPARED.
- **Location.** `golden::default_dir()` is `tests/golden/ifcap` under the crate folder that Cargo fixes at compile time, which is this folder. The bridge's `GOLDEN_DIR` overrides it for the bridge only; the tests always read this folder.
- **Authority.** `src/golden.rs` implements these conventions, and `STUB_CONTEXT` in `src/inquiry.rs` holds the context every capture must match. Where this README differs from them, the code governs.
- This README is documentation only. No code reads it, and it lies outside every case folder.

## Notation

- Paths are relative to the crate folder `rust/ifcap-po-inquiry/`, except those that begin with `Packages/`, which are relative to the repository root.
- Source is cited as `[path:line]`.
- Byte notation: `\n` is a newline (0x0A), `<BEL>` is `$C(7)` (0x07) and `<TAB>` is a tab (0x09).
- Spaces inside code spans are literal, and every trailing space is also stated in words.
- "n/e" marks a branch that is not exercised and not implemented. Reaching one ends the Rust session after the last prompt written, with nothing invented after it.

## Part A: Conventions

### 1. The seven files

Each `GC-0N/` folder holds exactly the seven files of `golden::CAPTURE_FILES`, each a regular file:

| File | Content | Read by (`src/golden.rs`) |
|---|---|---|
| `input.txt` | The typed inputs, one per line | `split_input_lines` |
| `transcript.txt` | The original's standard output, byte for byte | Taken raw; compared through `normalise` |
| `returned.txt` | The return value, zero bytes for none | `read_returned` |
| `db-before.zwr` | ZWR export of the export scope (Part B step 1) immediately before the session | The `db_before_equals_after` check |
| `db-after.zwr` | The same export immediately after the session | The `db_before_equals_after` check |
| `meta.txt` | Eight `key=value` lines describing the run | `parse_meta`, `Meta` |
| `normalisation.txt` | The case's normalisation rules, zero bytes for none | `parse_normalisation`, `normalise` |

A case's content is loaded only through `golden::load_case` and `golden::find_capture`, which read each file once; `golden::status`, `readiness` and `live_start` read only what the status counts and live mode need. In a case whose `input.txt` and `transcript.txt` are both present, every file that is present, the two exports included, must be readable UTF-8: one that is not makes the whole case `Unreadable`, which is FAIL. A case lacking either file is NOT COMPARED, whatever its other files hold.

Each file has a size limit, enforced while it is read, and every read is fresh:

| File | Size limit |
|---|---|
| `input.txt` | 1048576 bytes (1 MiB), the largest request body the bridge accepts |
| `transcript.txt` | 1048576 bytes (1 MiB), the live adapter's output cap |
| `returned.txt`, `meta.txt`, `normalisation.txt` | 65536 bytes (64 KiB) each |
| `db-before.zwr`, `db-after.zwr` | 67108864 bytes (64 MiB) each; an export of the Part B step 1 scope is about 1.2 MB |

- A file larger than its limit is present but unreadable: the case is `Unreadable` with `<path>: larger than <n> bytes`, which is FAIL, and replay answers 502 `NOT RECORDED (unreadable: <path>: larger than <n> bytes)`. Its readiness fails the checks that read it and `no_carriage_return`.
- The two exports are read together in 64 KiB pieces, compared and checked piece by piece, and never held whole.
- A case folder whose metadata cannot be read, for example under a golden folder the bridge user cannot search, is not ABSENT: it counts as recorded, the case is `Unreadable` with the folder's path and cause, such as `<golden folder>/GC-01: permission denied`, which is FAIL, and its readiness fails all six checks.

### 2. Capture input and output

- **No pseudo-terminal.** Capture with `docker exec -i`, never `-t`: a pseudo-terminal echoes the typed input into standard output and can rewrite line endings.
- **Line endings.** Every file uses `\n` only. A carriage return in any present file fails `no_carriage_return`.
- **`input.txt`.** One input per line, split by `split_input_lines`:
  - one trailing `\n` ends the last line and adds no input, so end the file with exactly one `\n` after the final `^`; a second one adds an empty input;
  - inner empty lines are inputs, each one a RETURN;
  - a zero-byte file holds no input.
- **Exit input.** The last input is `^`, which is `golden::EXIT_INPUT` and, in `frontend/app.js`, `const EXIT_INPUT = "^";`, the input the screen's EXIT button sends. `^` at the order prompt ends the lookup with no output [Packages/VA FileMan/Routines/DIC11.m:23] [Packages/VA FileMan/Routines/DIC.m:45]. The check is `input_ends_with_exit`.
- **`transcript.txt`:**
  - the exact standard-output bytes of the session, with no echo of typed input;
  - `<BEL>` stays byte 0x07;
  - standard error is kept apart and is not part of the transcript;
  - the file must be UTF-8;
  - the capture command adds nothing of its own to standard output;
  - nothing is ever cut out by hand, and never by normalisation.

### 3. `returned.txt`

- `read_returned`: zero bytes is `None`; otherwise the text with one trailing `\n` removed.
- `EN3^PRCHRPT` is entered by `DO` and quits without a value [Packages/Kernel/Globals/19+OPTION.zwr:14691] [Packages/IFCAP/Routines/PRCHRPT.m:70-75]. Open item O-06 is closed, so **every case writes zero bytes**, with no newline either, and both sides' `returned` is `None`.

### 4. `db-before.zwr` and `db-after.zwr`

- ZWR exports of the export scope of Part B step 1, taken immediately before and immediately after the session, in the ZWR form stated there.
- `db_before_equals_after` passes only when both files exist, are readable and are byte-identical. Nothing is parsed or reordered, so take both exports with the same command.
- The scratch globals that Part B step 1 excludes are outside the scope, so the original's scratch writes do not show.
- Any other difference between the two files is documented in `meta.txt` `setup`. The check still fails, so such a case is not READY.

### 5. `meta.txt`

`parse_meta` reads it:

- `key=value` lines, each split at its first `=`. The value is kept exactly, so write no space around `=`.
- Lines that are empty or hold only whitespace are ignored.
- A non-blank line without `=`, a key outside `golden::META_KEYS`, or a repeated key makes the file unparsable, and the error names its 1-based line. A missing key is not a parse error: `meta_keys_present` reports it.
- `meta_keys_present` passes only when the file parses and holds all eight `META_KEYS`, which are, in order: `image_digest`, `source_revision`, `snapshot_id`, `record_ids`, `entry_point`, `context`, `setup`, `docker_exec_args`.

| Key | Value |
|---|---|
| `image_digest` | The digest of the image the capture ran on, from Part B step 2 (b) (O-05) |
| `source_revision` | The revision of the runtime routines: `dd3fd0f3124c8178657d9fff4173c324da30f7ae` only when they match it, otherwise the revision actually found |
| `snapshot_id` | The snapshot identifier from Part B step 2 (e), the same for all five cases |
| `record_ids` | GC-01 `442:990001;440:990001`, GC-02 `442:990002;440:990001`, GC-03 `442:none`, GC-04 `442:none`, GC-05 `442:990005;440:990001` |
| `entry_point` | `EN3^PRCHRPT` |
| `context` | The context object of section 6 |
| `setup` | Free text: the HOME device name, `IOM`, any extra paging pauses, the reason a case is not reproducible, and any documented export difference |
| `docker_exec_args` | The start arguments of section 7 |

The template, with each `<…>` replaced by the recorded value:

```text
image_digest=<image digest>
source_revision=<revision>
snapshot_id=<snapshot id>
record_ids=<record ids>
entry_point=EN3^PRCHRPT
context={"DUZ":".5","IOF":"<IOF>","IOSL":"<IOSL>","STATION":"500"}
setup=<free text>
docker_exec_args=<arguments separated by single spaces>
```

`Meta::get` returns any key's value as recorded; `REPORT.md` part 2 shows each case's `image_digest` and `snapshot_id`.

### 6. `context`

- A flat JSON object whose values are all strings, holding exactly the four keys of `STUB_CONTEXT`: `{"DUZ":".5","IOF":"<value>","IOSL":"<value>","STATION":"500"}`.
- `Meta::context` reads it. Whitespace between tokens is allowed, and strings take the JSON escapes, `\uXXXX` included, so an `IOF` that holds control characters can be written. A repeated key, a nested value or a value that is not a string makes it fail.
- `STUB_CONTEXT` is `[("DUZ", Some(".5")), ("IOF", None), ("IOSL", None), ("STATION", Some("500"))]`. `golden::validate_context` checks each of its keys in that order, then every capture key it lacks:
  - `DUZ` must be `.5` and `STATION` must be `500`. Both are source-resolved: `D DUZ^XUP(.5)` [Packages/Kernel/Routines/ZOSVGUT1.m:13], and station 500, the default `$O(^PRC(411,"AC","Y",0))` [Packages/IFCAP/Routines/PRCFSITE.m:14] [Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr:290];
  - `IOF` and `IOSL` are unresolved (`None`, open item O-01). Any value is accepted, but both keys must be present;
  - a missing key, an extra key or a differing value fails, and the first failure is reported as `CONTEXT MISMATCH <key>: expected <value or ABSENT>, actual <value or ABSENT>`, where the expected value of a missing unresolved key is `UNRESOLVED (O-01)`. Examples: `CONTEXT MISMATCH DUZ: expected .5, actual 1`, `CONTEXT MISMATCH IOF: expected UNRESOLVED (O-01), actual ABSENT`, `CONTEXT MISMATCH IOM: expected ABSENT, actual 80`.
- `IOM` and the HOME device name are diagnostic only. They go in `setup`, never in `context`, where an extra key fails.

### 7. `docker_exec_args`

- `Meta::docker_exec_args` splits the value on single spaces. An empty value, or an empty argument from a leading, trailing or doubled space, is an error.
- Live mode runs the program `docker` directly with the arguments `exec`, `-i`, `<container>` and then these arguments, so the command is `docker exec -i <container> <args…>`. No shell runs on the host. `<container>` comes only from the bridge's `VEHU_CONTAINER`, never from a capture or a request.
- `golden::live_start` takes the arguments and `context` from the first case folder, in `CASE_IDS` order, whose `meta.txt` is readable UTF-8 and parses. With none, live mode answers 502 `NO META.TXT IN <golden_dir>`.
- `validate_context` runs before any command. A failure is 502 with its `CONTEXT MISMATCH …` text, and nothing runs.
- Standard input is each input followed by one `\n`, the framing of `input.txt`, and standard output is the transcript. A non-zero exit is 502 `EXIT <code>: <first line of docker's standard error>`.
- **Open (O-07; conflict K3).** An argument that contains a space cannot be expressed, and the documented start command of Part B step 1 needs one, its `-c` script. Live mode is therefore delivered unverified, and replay is the delivered MUMPS backend. **Question for the team:** supply a start command whose arguments contain no space, or change the convention. This README prescribes no workaround.

### 8. `normalisation.txt`

`parse_normalisation` and `normalise` implement it:

- The file is split on `\n`, and nothing is trimmed. A zero-length line is blank and ignored; a line of spaces is not blank.
- Every other line is a non-empty open delimiter, exactly one `<TAB>`, and a non-empty close delimiter, both literal. Any other line is a parse error naming its 1-based number (`normalisation line <n>: expected one tab between two non-empty delimiters`), and `normalisation_parses` fails.
- Rules apply in file order to both sides, each to the result of the one before. Scanning left to right, the span from each open delimiter through the next close delimiter after it, both included, becomes `<NORM>`, and scanning resumes after the span. An open delimiter with no later close delimiter leaves the rest of the text unchanged.
- No other transformation exists. COMPARE and `tests/equivalence.rs` use this one function.
- A rule may cover only run-time timestamps, internal record numbers and other variable fields documented in that case's `meta.txt`. It never covers order identifiers, status, quantities, amounts or messages, and no rule is ever added to hide a difference.
- A zero-byte file is valid and means no rules. `REPORT.md` part 2 lists every rule as `open → close`.

### 9. Readiness and verdicts

`golden::readiness` evaluates the six `golden::CHECKS`, in this order:

| Check | Passes when |
|---|---|
| `seven_files_present` | Each of the seven files exists as a regular file |
| `meta_keys_present` | `meta.txt` is UTF-8, parses, and holds all eight `META_KEYS` |
| `input_ends_with_exit` | `input.txt` is UTF-8 and its last input is `^` |
| `normalisation_parses` | `normalisation.txt` is UTF-8 and parses; a zero-byte file passes |
| `db_before_equals_after` | `db-before.zwr` and `db-after.zwr` are both readable and byte-identical |
| `no_carriage_return` | No present file contains a carriage return; a present file that cannot be read fails |

A check passes only when the files it reads exist and satisfy it; `no_carriage_return` tests the files that are present.

- **READY**: all six checks pass.
- **INCOMPLETE**: at least one check fails, and each failed check is listed by name.
- **ABSENT**: the `GC-0N` folder does not exist.
- A case is recorded when its `input.txt` and `transcript.txt` both exist (`golden::is_recorded`): present at all, readable or not, so a directory at either path counts. A case folder that cannot be inspected counts as recorded too. These are the cases `golden::load_case` finds `Recorded` or `Unreadable`.

The verdicts `tests/equivalence.rs` gives:

| Verdict | When |
|---|---|
| NOT COMPARED | The folder is absent, or `input.txt` or `transcript.txt` is absent |
| PASS | The case is READY, `validate_context` accepts its `context`, the normalised transcripts are equal, the `returned` values are equal, and the store equals its pre-call clone |
| FAIL | Every other recorded case: an INCOMPLETE one (conflict K12), an unreadable or non-UTF-8 file, a transcript or `returned` difference, a context mismatch, a store change, or `CORRUPT STORE` |

- Of the verdicts, only FAIL fails `cargo test`, and `REPORT.md` part 2 names its cause. With no case FAIL, `cargo test` still fails when a harness check fails (`REPORT.md` part 1 lists each with PASS or FAIL), when `SPEC.md` lacks a section the report includes (`MISSING SECTION <heading> IN SPEC.md`, with no report written), when `REPORT.md` cannot be written (`cannot write REPORT.md: <error>`), or when any other test or the build fails. Exit 0 is not acceptance, which needs five PASS.
- **While O-01 is open.** With `cases.zwr` loaded, the Rust sessions of GC-01, GC-02 and GC-05 end after `P.O./REQ.NO.: ` (with its one trailing space), because the display's `W:$Y>0 @IOF` depends on the unresolved HOME device [Packages/IFCAP/Routines/PRCHDP1.m:12]. Those cases FAIL once captured, until O-01 is answered. The no-match path of GC-03 and GC-04 never reaches that line.

### 10. Bridge use

- **Replay** (when `VEHU_CONTAINER` is unset or empty) answers `POST /api/mumps/session`, and the MUMPS side of `POST /api/compare/session`, through `golden::find_capture`: the first recorded case, in `CASE_IDS` order, whose inputs equal the request's. It reads the folder on every request, so new captures need no restart.
  - A hit gives 200 with the capture's raw `transcript.txt` and `returned`.
  - No hit gives 502 `NOT RECORDED`.
  - A present capture file that cannot be read, or a case folder that cannot be inspected (section 1), met on the way, gives 502 `NOT RECORDED (unreadable: <path>: <cause>)`.
  - COMPARE puts `MUMPS: ` before a MUMPS-side cause.
- **COMPARE `match`** is true only when the capture for the inputs is READY, `validate_context` accepts its `context`, the transcripts are equal after its rules, and the `returned` values are equal. Otherwise it is false, inputs with no capture included. Responses always carry the raw transcripts; normalisation applies only inside the comparison.
- **Status.** `GET /api/status` lists the recorded, READY and INCOMPLETE cases, with each INCOMPLETE case's failed checks, read afresh on every request. The bridge's third startup line reads `RECORDED <n> OF 5 READY <m> OF 5`, and the screen's status line, fetched when the page loads, reads `MUMPS BACKEND: REPLAY RECORDED <n> OF 5 READY <m> OF 5`. Reload the page to refresh the status line after adding captures, restart or not.

### 11. Fixtures and `cases.zwr`

- `tests/fixtures/ifcap/*.zwr` are the 11 supporting fixtures, unchanged copies of repository exports: `411.zwr`, `411.2.zwr`, `420.5.zwr`, `420.8.zwr`, `440.zwr`, `442.3.zwr`, `442.4.zwr`, `442.5.zwr`, `442.8.zwr`, `5.zwr` and `49.zwr`.
- `tests/fixtures/ifcap/cases.zwr` is the file Part B step 6 builds. `store::load_fixtures` loads every `*.zwr` file in byte order of name with `cases.zwr` last, so it wins on a repeated node; a missing `cases.zwr` is not an error [src/store.rs:323-387].
- The bridge loads the Store once, when it starts, so restart it after adding `cases.zwr`.
- Without `cases.zwr` the fixtures hold no `^PRC(442,0)` header, so the order lookup ends before writing its prompt [Packages/VA FileMan/Routines/DIC.m:14] and a Rust session ends after the station prompt (PO-09.b15; conflict K6). The original on VEHU never reaches that state.
- No hash or listing file is kept beside the fixtures.

### 12. Readers and integrity

- Only `src/golden.rs` reads the capture files. Its callers are `src/bridge.rs`, `src/bin/bridge.rs` (the startup counts), `tests/equivalence.rs` and `tests/bridge.rs`, and none of them re-implements a check. `inquire` and `Store` never read them.
- No rule is ever derived from capture content (acceptance criterion A4): captures judge the port, and source alone decides its rules.
- Capture files, `normalisation.txt` and an existing `cases.zwr` are never edited (a run with READY cases checks an existing `cases.zwr` by the preflight of Part B step 6), and no code is special-cased to a transcript.
- `tests/bridge.rs` writes its synthetic case folders only under `std::env::temp_dir()`, never here.
- No test sets or reads `GOLDEN_DIR` or `VEHU_CONTAINER`; only `src/bin/bridge.rs` reads them.

### 13. Prompts and echoes

Each prompt is followed by its READ on the same line:

| Prompt | Written just before it | Exact prompt bytes | Trailing space | Source |
|---|---|---|---|---|
| Station | `\n\n\n` | `Select STATION NUMBER ('^' TO EXIT): 500// ` | One after the colon and one after `//` | [Packages/IFCAP/Routines/PRCFSITE.m:8, 14, 18] [Packages/VA FileMan/Routines/DIC.m:25] [Packages/VA FileMan/Routines/DIC11.m:15-17] |
| Order | `\n\n` [Packages/IFCAP/Routines/PRCHPAT.m:83], then `\n` [Packages/VA FileMan/Routines/DIC.m:25] | `P.O./REQ.NO.: ` | One after the colon | [Packages/IFCAP/Routines/PRCHRPT.m:12] [Packages/VA FileMan/Routines/DIC11.m:15-17] |
| Halt | `\n`, then spaces to column 8 (`?8`) | `ENTER '^' TO HALT: ` | One after the colon | [Packages/IFCAP/Routines/PRCHDP2.m:10] |
| End of display | `\n` and `<BEL>` | `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ` | One after the colon | [Packages/IFCAP/Routines/PRCHDP2.m:41] |

Echoes between the prompts:

- **R1, station default accepted.** `      CAMP MASTER`: six leading spaces, no trailing space and no newline [Packages/VA FileMan/Globals/DD.zwr:488724] [Packages/Kernel/Globals/4+INSTITUTION.zwr:494].
- **R2, unique order match.** Two spaces and the full `.01` value, then two spaces before each identifier's output [Packages/VA FileMan/Globals/DD.zwr:514202-514204]:
  - `Z.01`: the order date as `MM-DD-YY`;
  - `Z.02`: the method-of-processing abbreviation, three spaces and the status name;
  - `Z.03`: `\n`, 13 spaces, `FCP: ` (one trailing space), the first word of the fund control point, five spaces, `$ ` (one trailing space) and the order total with two decimals. No newline follows.
- **R3, no match (PO-09.b14).** `<BEL> ??\n\nP.O./REQ.NO.: `, ending with the prompt's one trailing space, then the next READ, which gets the same read checks and search [Packages/VA FileMan/Routines/DIC.m:25, 63-65, 80-81] [Packages/VA FileMan/Routines/DIC11.m:15-17]. The Rust side implements it: O-03 is closed for the case values under the fixture profile of Part B step 1.

**Identifier.** The value typed at the order prompt is the order's `C` index value, the part of the `.01` number after `500-`: `$P(X,"-",2)` [Packages/VA FileMan/Globals/DD.zwr:514318-514320], on the index `C` that the lookup uses [Packages/IFCAP/Routines/PRCHPAT.m:80]. It meets the input transform's `Z?3UN3N`: three uppercase letters or digits, then three digits [Packages/VA FileMan/Globals/DD.zwr:514312].

**`input.txt`, byte for byte:**

| Case | Bytes |
|---|---|
| GC-01, GC-02, GC-05 | `\n<C value>\n\n\n^\n` |
| GC-03 | `\n<C value>\n^\n` |
| GC-04 | `\nINVALID!\n^\n` (12 bytes) |

**GC-05 notes.**

- Status 45 `Cancelled Order` has status order 45 [tests/fixtures/ifcap/442.3.zwr:26], which is not below 10, so `EN30` writes no ` >>> Status makes this record non-specifiable here.` (one leading space) [Packages/IFCAP/Routines/PRCHRPT.m:73]. The status name prints at column 37 [Packages/IFCAP/Routines/PRCHDP1.m:15], and this path writes no other cancellation message. Status 25, of GC-01 and GC-02, has status order 20 [tests/fixtures/ifcap/442.3.zwr:17], also not below 10.
- **Profile risk.** A cancelled order with retained history may carry node 6 (field 50 AMENDMENT [Packages/VA FileMan/Globals/DD.zwr:514981]), node 11 (field .6 PARTIAL [Packages/VA FileMan/Globals/DD.zwr:514467]) or item partial receipts (item node 3, field 20 DATE RECEIVED [Packages/VA FileMan/Globals/DD.zwr:516311]). Any of them takes the order off the profile, and the case then FAILs. Record the outcome as it is, and never alter data to make the order fit.

## Part B: Capture procedure

The team runs these steps on its Docker host. Nothing here was run by this repository's build or tests: every Docker command is a plan default, unverified, that the team confirms (O-07).

### Step 1. Inputs, records, fixture profile, export scope and start command

**Cases.** Plan defaults the team may change; record any change in `meta.txt` `setup`. In the input lists, `""` is an empty line.

| Case | `input.txt` lines | Records the team loads | `record_ids` |
|---|---|---|---|
| GC-01 | `""`, `GC1001`, `""`, `""`, `^` | Order IEN 990001, `.01` `500-GC1001`, one item: quantity 2 × unit cost `125.5` = `251`; order total `251`; status 25 `Ordered and Obligated` [tests/fixtures/ifcap/442.3.zwr:17]; vendor IEN 990001 `GC TEST VENDOR` | `442:990001;440:990001` |
| GC-02 | `""`, `GC2002`, `""`, `""`, `^` | Order IEN 990002, `.01` `500-GC2002`, items `3 × 19.95 = 59.85`, `2 × 125.5 = 251` and `1 × .99 = .99`; order total `311.84`; status 25; the same vendor | `442:990002;440:990001` |
| GC-03 | `""`, `GC3003`, `^` | None: `GC3003` is confirmed absent from every `^PRC(442)` index | `442:none` |
| GC-04 | `""`, `INVALID!`, `^` | None | `442:none` |
| GC-05 | `""`, `GC5005`, `""`, `""`, `^` | Order IEN 990005, `.01` `500-GC5005`, status 45 `Cancelled Order` [tests/fixtures/ifcap/442.3.zwr:26], two retained items `1 × 10` and `1 × 5`; order total `15`; the same vendor | `442:990005;440:990001` |

- Record numbers avoid `99999949`, which the repository export's index nodes name [Packages/IFCAP/Globals/442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:4-5].
- The totals $251.00 (GC-01) and $311.84 (GC-02) are sanity checks only. The captures decide.

**Why these lines.**

- The empty first line accepts the station default 500. The station prompt is always asked: `^PRC(411,0)` piece 4 is 6 [tests/fixtures/ifcap/411.zwr:3], more than one site, and `PRCF("X")="SP"` contains `S` [Packages/IFCAP/Routines/PRCHRPT.m:5] [Packages/IFCAP/Routines/PRCFSITE.m:13, 18].
- For a found order, the two `""` lines answer `ENTER '^' TO HALT: ` and `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: `, and the `^` exits at the order prompt that `G EN30` asks again [Packages/IFCAP/Routines/PRCHRPT.m:75].
- For GC-03 and GC-04, the `^` answers the order prompt that the no-match sequence asks again. It ends the lookup with no output, and `EN30` quits [Packages/IFCAP/Routines/PRCHRPT.m:72].
- `INVALID!` passes every read check, because it is `.ANP`, and reaches the search and the no-match response. The read checks run in this order [Packages/VA FileMan/Routines/DIC11.m:15-34]: timeout (line 18), `X'?.ANP` (19), long number (21), `^` exit (23), longer than 250 characters (24), `?` help (26), leading backtick or a lone space (27), empty-answer default (30), `^`-prefixed jump (33).
- If paging under the unresolved `IOSL` inserts an `ENTER '^' TO HALT: ` pause [Packages/IFCAP/Routines/PRCHDP2.m:31], the first capture shows it. Add one `""` per pause before the `^`, and record the change in `setup` (O-01).
- These sequences replace the request's two-line convention of an identifier line and an exit line, which would answer the station prompt with the identifier (conflict K2).

**Fixture profile.** The team's records must meet it. A record outside it ends the Rust session at an n/e branch, and the case FAILs.

- `^PRC(442,0)` holds piece 2 `442I`, as in the repository export [Packages/IFCAP/Globals/442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3]. The orders' index nodes give the file a subscript after `"A["`, such as `"C"` [Packages/VA FileMan/Routines/DICM.m:71].
- Each order's node 0 piece 1 holds its full `.01` number `500-<C value>`, such as `500-GC1001` [Packages/VA FileMan/Globals/DD.zwr:514312], whose part after `500-` is its `C` index value `$P(X,"-",2)` [Packages/VA FileMan/Globals/DD.zwr:514318-514320]. The piece's leading numeric value, 500, is the station the lookup screen compares, and method of processing (field .02, node 0 piece 2) is `1`, which is below 10, so the lookup screen passes [Packages/IFCAP/Routines/PRCHRPT.m:9] [Packages/IFCAP/Routines/PRCHPAT.m:81].
- No two orders share a `C` value, and no `C` value is a prefix of another.
- Nodes 0 and 1 are present. None of nodes 11, 5, 3, 15, 4, 13 or 6 is present.
- Field 5.3 is empty. The vendor is not EDI. Field 65 is not `RMPR`. Node 23 piece 7 is empty. Field .08 is not `Y`.
- Each item has a zero node and a node 2 whose EDI status codes are empty, and a description of at most 32 characters that contains no `|`.
- No item has a contract number or a partial receipt, and `^PRC(442.8,"B")` holds no entry for the order.
- Added for O-03:
  - fields 46 PURCHASE CARD NUMBER [Packages/VA FileMan/Globals/DD.zwr:514943] and 61 PURCHASE CARD HOLDER [Packages/VA FileMan/Globals/DD.zwr:515047] are empty, and no item carries subfile 442.14 field 11 INVENTORY/DISTRIBUTION POINT [Packages/VA FileMan/Globals/DD.zwr:517723], so the indexes `M`, `MCH` and `G` stay empty;
  - no subscript of any index of files 440, 442.5 or 442.3 equals or begins with `GC3003` or `INVALID!`. The repository fixtures hold none.

**Export scope** of `db-before.zwr` and `db-after.zwr`:

- Include every global root of the supporting fixtures: `^PRC(411)`, `^PRC(411.2)`, `^PRCD(420.5)`, `^PRCD(420.8)`, `^PRC(440)`, `^PRCD(442.3)`, `^PRC(442.4)`, `^PRCD(442.5)`, `^PRC(442.8)`, `^DIC(5)` and `^DIC(49)`.
- Include also `^PRC(442)`, `^VA(200,.5)` and `^DIC(4,500)`.
- Exclude, as scratch that is not business data: `^XUTL`, which `^%ZIS` writes [Packages/Kernel/Routines/ZIS.m:13]; `^UTILITY` and `^TMP`, the word-wrap scratch; and `^DISV`, which FileMan writes on a successful lookup [Packages/VA FileMan/Routines/DIC2.m:65].

**ZWR form.** The exports and `cases.zwr` use the form `Store::from_zwr` accepts:

- lines before the first line that starts with `^` are header lines and are ignored, and so are blank lines;
- every other line is `^name(subscripts)=value`, and a repeated node replaces the earlier value;
- an unquoted subscript is a canonical number: no leading zero, no trailing zero after a decimal point, no leading `+`. A quoted subscript is a string, except that a quoted canonical number is stored as a number;
- inside quotes, `""` stands for one `"`;
- a value is a quoted string or a canonical number;
- `$C(n)` and `_` concatenation are rejected. Report such a node to the team, and never edit the export;
- a malformed line in a fixture is reported as `<path>: MALFORMED ZWR LINE <line>: <message>`, with the 1-based line counting header lines, and the bridge then stops with `bridge: cannot load fixtures: <error>`.

**Start command** (O-07): unverified and team-owned. Run it from inside the case folder, byte for byte:

```text
docker exec -i vehu su - vehu -c 'source /home/vehu/etc/env && yottadb -run %XCMD "S U=\$C(94),DTIME=300,DT=\$\$DT^XLFDT D DUZ^XUP(.5) D EN3^PRCHRPT"' < input.txt > transcript.txt
```

- It sets `U`, `DTIME`, `DT` through `$$DT^XLFDT` [Packages/Kernel/Routines/XLFDT.m:159], and `DUZ` through `DUZ^XUP` [Packages/Kernel/Routines/XUP.m:54-64], which sets no `DTIME` and no `IO*` variable.
- Because `DT` is defined, the `NOW^%DTC` branch PO-03.b2 [Packages/IFCAP/Routines/PRCFSITE.m:7] is not exercised.
- No READ may time out [Packages/IFCAP/Routines/PRCHDP2.m:10, 41] [Packages/VA FileMan/Routines/DIC11.m:17]: the Rust READ never does.
- The command calls `D EN3^PRCHRPT` directly, so no menu prompt reaches the transcript, and the workflow returns nothing.

### Step 2. Commands (a) to (f)

On the Docker host. These are plan defaults, unverified: this run had no Docker.

- (a) `docker pull worldvista/vehu:202609`
- (b) `docker image inspect --format '{{index .RepoDigests 0}}' worldvista/vehu:202609` gives `image_digest`.
- (c) `docker run -d --name vehu worldvista/vehu:202609`
- (d) Load the synthetic records of step 1, and confirm the fixture profile.
- (e) `docker commit vehu vehu-gc-snapshot`, then `docker image inspect --format '{{.Id}}' vehu-gc-snapshot` gives `snapshot_id`.
- (f) Before each case: `docker rm -f vehu && docker run -d --name vehu vehu-gc-snapshot`.

The VEHU database is inside the container filesystem (`/home/vehu/g/vehu.gld` and `vehu.dat`; the image declares no `VOLUME`), so `docker commit` captures it and (e) and (f) stand as written.

### Step 3. Setup

- Confirm that `DUZ` `.5` is POSTMASTER [Packages/Kernel/Globals/200+NEW PERSON.zwr:4] and that `^VA(200,.5,20)` piece 2 is `POSTMASTER` [Packages/Kernel/Globals/200+NEW PERSON.zwr:12], so `A^PRCFSITE` never writes [Packages/IFCAP/Routines/PRCFSITE.m:34, 37-38].
- Confirm station 500 `CAMP MASTER` [Packages/Kernel/Globals/4+INSTITUTION.zwr:494] and the `^PRC(442,0)` header of step 1.
- Determine the `IOF` and `IOSL` that `^%ZIS` sets for the HOME device at [Packages/IFCAP/Routines/PRCHDP1.m:12]:
  - with `^XUTL("XQ",$J,"IO")` defined, HOME restores the sign-on values [Packages/Kernel/Routines/ZIS.m:28]. Otherwise the device is looked up by `$I`, and a device that is not found writes `HOME DEVICE (…) DOES NOT EXIST IN THE DEVICE FILE` [Packages/Kernel/Routines/ZIS.m:31-38];
  - an empty `IOF` becomes `#` [Packages/Kernel/Routines/ZIS1.m:103-113];
  - the candidates disagree. Device 267 `PMT290` (`$I` `0`), terminal type 168, gives `132^#^60` [Packages/Kernel/Globals/3.5+DEVICE.zwr:14-18, 679] [Packages/Kernel/Globals/3.2+TERMINAL TYPE.zwr:816-817]. Device 595 `CONSOLE`, terminal type 230, gives `80^#,$C(27,91,50,74,27,91,72)^24` [Packages/Kernel/Globals/3.5+DEVICE.zwr:36-40] [Packages/Kernel/Globals/3.2+TERMINAL TYPE.zwr:1152-1153].
- Record `IOF` and `IOSL` in `context`, and `IOM` and the device name in `setup`.
- Record the bytes the GC-01 transcript shows for `W @IOF`. This answers the O-01 question.
- If `IOF` shows cursor addressing, such as the `$C(27,91,50,74,27,91,72)` clear-and-home sequence, the workflow-selection stop condition applies: stop and report to the team.

### Step 4. Per-case run

In the team's staging folder `<staging>/GC-0N/`, which is copied into `tests/golden/ifcap/` at playbook phase 8:

1. Restore the snapshot with step 2 (f).
2. Export `db-before.zwr`.
3. Run `docker exec -i <container> <docker_exec_args> < input.txt > transcript.txt` from inside the case folder, with the container `vehu` and the arguments written out in the team's shell: the start command of step 1.
4. Export `db-after.zwr`.
5. Write a zero-byte `returned.txt` with `: > returned.txt`, and write `meta.txt` (Part A section 5).

### Step 5. Run twice

Run each case a second time from the restored snapshot, and compare the two runs.

- A reproducible case gets a `normalisation.txt` covering only the variable fields its `meta.txt` documents, or a zero-byte one when there are none.
- A case that is not reproducible keeps no `normalisation.txt`. It is INCOMPLETE, failing `seven_files_present` and `normalisation_parses`, which reads the same file, so it FAILs. `meta.txt` `setup` gives the reason.

### Step 6. `cases.zwr`

- It is the union, over the five `db-before.zwr` files, of the nodes under `^PRC(442)` for the synthetic orders, the `^PRC(442,0)` header, their index nodes, and the synthetic vendor's `^PRC(440)` nodes.
- Omit any line already present verbatim in a supporting fixture.
- Keep the lines in ZWR collation order under the two ZWR header lines.
- Save it as `tests/fixtures/ifcap/cases.zwr`. It loads last and wins on a repeated node [src/store.rs:323-387].
- Add no other file.

**Conditional path: a run that finds new case folders.** A run that changes the port and finds `GC-0N` folders that were absent when it was planned takes these steps, in order:

1. Before editing any code, record the baseline capture-hash listing with the two lines of step 7, and evaluate the six readiness checks of Part A section 9 for every case.
2. **No READY case.** Create nothing: no `cases.zwr` is built, and every recorded case is judged as it stands.
3. **At least one READY case and no `tests/fixtures/ifcap/cases.zwr`.** Build it from the READY cases' `db-before.zwr` only:
   - the node selection of the first bullet above, each line once;
   - omit any line already present verbatim in a supporting fixture;
   - keep the lines in ZWR collation order under the two ZWR header lines;
   - it is a text transformation and executes no MUMPS;
   - the after listing records the file on its own line as `CREATED BY RUN`, outside the equality rule of `SPEC.md` "Capture hashes".
4. **At least one READY case and an existing `cases.zwr`: the preflight.** Leave the file untouched, and before editing any code:
   - apply the extraction of the first bullet above to the READY cases' `db-before.zwr`;
   - list every extracted line found neither in `cases.zwr` nor in a supporting fixture;
   - record that comparison in `SPEC.md` "Run evidence";
   - for any line listed, add a `SPEC.md` "Open items" entry naming the case and its nodes, with the action "rebuild `cases.zwr` by README step 6";
   - it is not a harness check: the harness reports those cases by the ordinary comparison, which shows the difference the missing records cause.
5. Each READY case then ends PASS or FAIL.

### Step 7. Checks

From `rust/ifcap-po-inquiry/`:

- Run `cargo test --offline --locked`. `REPORT.md` part 2 shows the verdicts and part 3 each case's readiness. The test also prints `GC-0N <verdict>` for each case, which Cargo shows only when the test fails; to see the lines on a passing run, use `cargo test --offline --locked --test equivalence -- --nocapture` (`RUNNING.md` step 2).
- Restart the bridge with `cargo run --offline --locked --bin bridge`, then reload the page. Confirm the bridge's third startup line `RECORDED <n> OF 5 READY <m> OF 5` and the screen's status line `MUMPS BACKEND: REPLAY RECORDED <n> OF 5 READY <m> OF 5`.
- Record the capture hashes with these two lines, verbatim:

```text
find tests/golden/ifcap -type f ! -name README.md -exec sha256sum {} +
if [ -f tests/fixtures/ifcap/cases.zwr ]; then sha256sum tests/fixtures/ifcap/cases.zwr; else echo "ABSENT tests/fixtures/ifcap/cases.zwr"; fi
```

## Commands

Run from `rust/ifcap-po-inquiry/`, with each `<…>` replaced by a real value. The last three lines are alternatives: each starts one bridge on the fixed address `127.0.0.1:8080`.

```text
cargo build --offline --locked
cargo test --offline --locked
cargo run --offline --locked --bin bridge
VEHU_CONTAINER=<container name> cargo run --offline --locked --bin bridge
GOLDEN_DIR=<folder> cargo run --offline --locked --bin bridge
```

- Set `RUSTUP_AUTO_INSTALL=0` when offline, so that a missing pinned toolchain fails instead of downloading.
- The short forms `cargo test` and `cargo run --bin bridge` are equivalent on a prepared host.
- The bridge prints three lines to standard output: `http://127.0.0.1:8080/`, then `MUMPS MODE: REPLAY` (or `MUMPS MODE: LIVE <container>`), then `RECORDED <n> OF 5 READY <m> OF 5`. A port already taken gives `bridge: port 8080 on 127.0.0.1 is already in use` on standard error and exit status 1, a golden folder that does not exist, or is not a directory, gives `bridge: golden directory missing: <path>`, and one whose metadata cannot be read for another reason, such as a symbolic-link loop or a parent folder that cannot be searched, gives `bridge: golden directory unreadable: <path>: <error>`. After either, the bridge keeps serving: with the folder missing no case is recorded, and with it unreadable every case counts as recorded but unreadable (`RECORDED 5 OF 5 READY 0 OF 5`).

