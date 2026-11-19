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

Step 6 builds `cases.zwr` only. The buyer node `^VA(200,.5,0)`, which `DIS^PRCHDP1` reads for the buyer or agent name [Packages/IFCAP/Routines/PRCHDP1.m:59], is outside the node selection of this step's first bullet, so `cases.zwr` does not hold it. It is a separate supporting fixture, `tests/fixtures/ifcap/200.zwr`, which holds that one node under the two ZWR header lines. From `rust/ifcap-po-inquiry/`, `grep -F '^VA(200,.5,0)=' tests/golden/ifcap/GC-01/db-before.zwr` extracts the node's line, the same line as [Packages/Kernel/Globals/200+NEW PERSON.zwr:4].

### Step 7. Checks

From `rust/ifcap-po-inquiry/`:

- Run `cargo test --offline --locked`. `REPORT.md` part 2 shows the verdicts and part 3 each case's readiness. The test also prints `GC-0N <verdict>` for each case, which Cargo shows only when the test fails; to see the lines on a passing run, use `cargo test --offline --locked --test equivalence -- --nocapture` (`RUNNING.md` step 2).
- Restart the bridge with `cargo run --offline --locked --bin bridge`, then reload the page. Confirm the bridge's third startup line `RECORDED <n> OF 5 READY <m> OF 5` and the screen's status line `MUMPS BACKEND: REPLAY RECORDED <n> OF 5 READY <m> OF 5`.
- Record the capture hashes with these two lines, verbatim:

```text
find tests/golden/ifcap -type f ! -name README.md -exec sha256sum {} +
if [ -f tests/fixtures/ifcap/cases.zwr ]; then sha256sum tests/fixtures/ifcap/cases.zwr; else echo "ABSENT tests/fixtures/ifcap/cases.zwr"; fi
```

## Part C: Parity corpus

Part C covers the parity corpus. Its three recording scripts are plan defaults that no build or test runs, like the commands of Part B, and they stay unproven against VEHU until the team's first recording (`SPEC.md` O-16, O-17). `SPEC.md` O-17 and "Run evidence" record what the corpus round did and did not execute, including two read-only Docker queries, `docker images -q` and `docker info`, that departed from that round's no-Docker rule. Besides those queries, `check_corpus.sh` ran whole on synthetic recordings in scratch copies, and blocks of the other two scripts ran against a stubbed `docker`; both the queries and these runs departed from that round's rule that it run no script and no Docker command, so that round is noncompliant on it (`SPEC.md` O-17, "Run evidence").

### C1. Purpose and classes

The corpus is 231 sessions in three groups:

- **CP-001 to CP-200.** Generated orders. CP-n is IEN `991000+n`, with `.01` `500-CPnnn` and `C` value `CPnnn`, n in three digits. 150 orders have one item and 50 have two to eight.
- **CE-01 to CE-26.** Edge inputs. As specified, 16 are class IN and 10 class BOUNDARY. After the generator's gate there are 13 IN and 13 BOUNDARY: CE-04 `12345`, CE-06 `gc9999` and CE-08 `991001` move to BOUNDARY at PO-09.b11, with their inputs unchanged (`SPEC.md` K17).
- **CR-01 to CR-05.** The GC-01 to GC-05 input sequences of Part B step 1, replayed on the corpus data (world B).

`manifest.tsv` gives each case one of four classes:

- **`CP`, `IN` and `CR`.** The Rust session runs to its end, closing with `P.O./REQ.NO.: ` (one trailing space) followed by `\n`.
- **`BOUNDARY`.** The Rust session halts at a not-implemented branch, the row's expected branch. The case is outside equivalence.

A row tagged `finding=<id>` is judged by its tag instead: its Rust session halts at `<id>`, a branch `SPEC.md` calls implemented, and the finding is recorded, never patched.

The corpus widens the five-case evidence only as far as a recording shows. Until the team records it (`SPEC.md` O-17), every CP, IN and CR case is NOT COMPARED and every BOUNDARY case is BOUNDARY.

No corpus order reaches the paging prompt (`$Y+5>IOSL` [Packages/IFCAP/Routines/PRCHDP2.m:31], PO-30.b2, n/e): the generator bounds every order's `$Y` at 45.

### C2. Layout

Paths in the table are relative to this folder, except `tests/fixtures/ifcap-corpus/corpus.zwr` and `captures-work/corpus/`, which are relative to the crate folder.

| File | Scope | Written by |
|---|---|---|
| `corpus/manifest.tsv` | The corpus, one row per case | `corpus_gen` |
| `corpus/<id>/input.txt` | Per case, 231 files | `corpus_gen` |
| `tests/fixtures/ifcap-corpus/corpus.zwr` | World B's added data, outside this folder | `corpus_gen` |
| `corpus/scripts/load_corpus.sh`, `record_corpus.sh`, `check_corpus.sh` | Recording tooling | Hand-written, committed |
| `corpus/BASE.txt` | Per run | `load_corpus.sh` |
| `corpus/<id>/transcript.txt` | Per case | `record_corpus.sh` |
| `corpus/meta.txt`, `corpus/run-summary.txt` | Per run | `record_corpus.sh` |
| `corpus/SHA256SUMS` | Per run, written last | `record_corpus.sh` |
| `captures-work/corpus/` | Scratch, not committed (crate `.gitignore`) | The two Docker scripts |

- **`manifest.tsv`.**
  - Header row `case<TAB>class<TAB>seed<TAB>order_ien<TAB>c_value<TAB>item_count<TAB>tags<TAB>expected_branch`, then 231 rows in `golden::corpus_case_ids()` order (CP, then CE, then CR), with `\n` line ends.
  - `seed` is `11400714819323198485`, the decimal form of `corpus::SEED`, on every row.
  - `class` is `CP`, `IN`, `BOUNDARY` or `CR`, after any move.
  - `order_ien` and `c_value` hold a CP row's order. For CE and CR rows they list the orders the sequence opens, comma-separated in first-use order, or are empty.
  - `item_count` is empty for CE and CR rows.
  - `tags` are comma-separated `dimension=value` pairs on CP rows, plus `moved=IN>BOUNDARY` or `moved=BOUNDARY>IN` on a moved CE row, `gc=GC-0N` on a CR row, and `finding=<id>` on a row recorded as a finding.
  - `expected_branch` is `none` or the branch ID the Rust session halts at.
- **`meta.txt`** holds five `key=value` lines, the `golden::CORPUS_META_KEYS`, in this order:
  - `image_digest`, the digest of `BASE_IMAGE`;
  - `snapshot_id`, the id of `vehu-snap:corpus-base`;
  - `source_revision`;
  - `context`, the constant `{"DUZ":".5","IOF":"#","IOSL":"60","STATION":"500"}`, the O-01 answer, written as a constant because a recording run runs no probe;
  - `start_command`, `docker exec -i <NAME> su - vehu -c "<start command>"`, as every case ran it.

  `golden::parse_corpus_meta` rejects an unknown, repeated or missing key.
- **`BASE.txt`** holds one `key=value` line per key: `snapshot_image` (`vehu-snap:corpus-base`), `snapshot_id`, `base_image` (`vehu-snap:gc-base`), `roots`, `export_command`, `export_sha256`, `export_bytes` and `corpus_zwr_sha256`.
- **`run-summary.txt`.**
  - Lines `exports=IDENTICAL|DIFFERENT`, `export_sha256=<hex>` and `runs=IDENTICAL|DIFFERENT`.
  - Then one `<id><TAB><RECORDED|QUARANTINED><TAB><run 1 exit status><TAB><run 2 exit status>` line per case, in manifest order.
  - Then one `DIFF<TAB><id or export>` line per collected difference.
  - The exit statuses are evidence only, and no verdict reads them: `exports=` and `runs=` decide the run. A `DIFF` line is written only beside a `DIFFERENT` identity line, so `golden::load_corpus_run` and `check_corpus.sh` reject one beside `exports=IDENTICAL` and `runs=IDENTICAL` as a contradiction, naming its line.
- **`SHA256SUMS`** holds `shasum -a 256` lines, `<64 hex>` then two spaces then the path relative to `corpus/`, sorted under `LC_ALL=C`. It covers `BASE.txt`, `meta.txt`, `run-summary.txt` and every `<id>/input.txt` and `<id>/transcript.txt`.
- **Readers.** Only `src/golden.rs` reads the corpus capture files, through `golden::read_corpus_input`, `load_corpus_case`, `load_corpus_run` and `CorpusRun::load_case`, under these integrity rules:
  - each present file is UTF-8 and within its size limit: 1 MiB for `input.txt` and `transcript.txt`, 64 KiB for each run file;
  - no file holds `\r`;
  - a present `transcript.txt` is not empty;
  - `input.txt` is not empty, and its last input begins with `^`;
  - a symbolic link found at the corpus folder, a case folder or any corpus file when it is inspected, dangling or not, fails as `<path>: symbolic link`, as `check_corpus.sh` also rejects every link it finds at the paths it inspects, though, with no check after a read that the path still holds the file it found, it assumes a stable tree (C6). A file is read only after that inspection, by an open that follows a link, so a text that no longer matches the regular file standing at its path, or a folder that is no longer the one first found, fails as `<path>: changed while read`, and a link swapped in at a file during its read never yields its target's bytes. On Linux each file is looked up through an open handle of its folder as first found, so a folder swapped for a link during a read is never followed, even when it is swapped back before the last check; elsewhere these checks assume a stable tree, as they are not atomic with the opens they guard. Folders above `corpus/` are trusted on every platform and never inspected;
  - once any transcript exists, `meta.txt`, `SHA256SUMS` and `run-summary.txt` must all exist, every `SHA256SUMS` line must match its file, every transcript must be listed, and `run-summary.txt` must read `exports=IDENTICAL` and `runs=IDENTICAL`. A `DIFF` line is rejected when it names anything other than `export` or one of the cases, and also beside `exports=IDENTICAL` and `runs=IDENTICAL`, which it contradicts, because `record_corpus.sh` writes one only beside a `DIFFERENT` identity line. A `BASE.txt` alone, written before any recording, leaves the corpus unrecorded.
  - the equivalence harness holds each case it judges to the recording run it read first (`golden::CorpusRun::load_case`): a transcript present though that run was read as absent, or other than the bytes that run's `SHA256SUMS` listed for it, fails that case, and so does a transcript that listing names but the case no longer holds. An input other than the one that run's `SHA256SUMS` listed fails the case too, even when it equals the generated model's and a rewritten `SHA256SUMS` lists it now.

  A failed run check shows in `REPORT.md` part 3 as `Recording run: INTEGRITY FAIL: <cause>`, the cause in double quotes inside a code span, and a failed case check makes a non-BOUNDARY case NOT COMPARED, while a BOUNDARY case stays BOUNDARY (C8 rule 1). Either fails `cargo test` after the report is written. `manifest.tsv` and `corpus.zwr` are generated data, not captures.
- **What ignores `corpus/`.** None of the five-case readers looks inside it:
  - `golden::status`, `find_capture` and `live_start`, which read only the `CASE_IDS` folders, and `golden::load_case`, which reads whichever case folder its caller names: every existing caller of this folder names a GC id, never `corpus`;
  - the bridge, which reaches captures only through them and still prints `RECORDED <n> OF 5 READY <m> OF 5`;
  - the kit's `GC-0[1-5]` globs (`build_cases_zwr.sh`, `add_buyer_fixture.sh`) and the literal ids of `capture_all.sh`;
  - `scripts/kit/check_captures.sh`, which checks GC-01 to GC-05 only;
  - `scripts/demo/compare_live.py`, which lists only direct children holding `input.txt` [scripts/demo/compare_live.py:103-104], and `corpus/` itself holds none.

  `probe_vehu.sh` and `setup_vehu.sh` print one more `corpus` entry when they list this folder, which is display only.
- **Capture hashes.** The `find` line of Part B step 7 lists the corpus files too. The corpus round compares that listing without its `tests/golden/ifcap/corpus/` lines (`SPEC.md` K20). This README is excluded by name, so editing it changes no listed hash.

### C3. The two worlds

- **World A** is `tests/fixtures/ifcap/*.zwr`, loaded by `store::load_fixtures`. It serves GC-01 to GC-05, which still PASS, and the bridge.
- **World B** is world A plus `tests/fixtures/ifcap-corpus/corpus.zwr`, built only by `corpus::world_b`. Every corpus case runs on it, the CR cases included.
- `corpus.zwr` never enters `tests/fixtures/ifcap/`, and `load_fixtures` never reads the sibling folder.
- `corpus.zwr` holds only the `^PRC(442)` orders 991001 to 991200 and their `B` and `C` index nodes. It changes no header and adds no reference entry, and every reference resolves in world A.
- Every corpus order meets the fixture profile of Part B step 1. The generator asserts it before it writes, and the gate asserts it again.

### C4. Generator

From `rust/ifcap-po-inquiry/`:

```text
cargo run --offline --locked --bin corpus_gen
cargo run --offline --locked --bin corpus_gen -- --check
```

- **No argument.** It generates the corpus and, only after the whole generation succeeds, writes the 233 files: `corpus.zwr`, `manifest.tsv` and the 231 `input.txt` files. It never writes a recording file.
- **`--check`** writes nothing. It compares the 233 files byte for byte with the committed ones. It prints `check: 233 files equal` (exit 0), or one `differs: <path>` line per differing or missing file followed by `check: <k> of 233 files differ` (exit 1).
- Both modes print the summary first, beginning with `specified: 200 CP, 26 CE (16 IN, 10 BOUNDARY), 5 CR, 50 multi-item orders`. Its `generated:` line counts the classes after the gate.
- Any other argument prints `usage: corpus_gen [--check]` on standard error and exits 2. A generation failure writes nothing, prints `corpus_gen: <cause>` on standard error and exits 1.
- **Sources.** The generator uses a fixed seed, `corpus::SEED`, and the standard library only. It generates from `tests/fixtures/ifcap/*.zwr` and `SPEC.md` alone, never from a capture. `--check` also reads the committed generated files, each `input.txt` through `golden::read_corpus_input`.
- **Outcomes.** A Rust halt where `SPEC.md` says the branch is implemented is recorded as a finding, never patched. A CP variant that halts at a not-implemented branch is dropped and listed, in the summary and in `REPORT.md` part 3.
- Generated files are never edited by hand: change the generator and run it again.
- `cargo test --offline --locked` also runs `tests/corpus.rs`, the offline gate, which needs no Docker.

### C5. The team's order

The prerequisite is the snapshot `vehu-snap:gc-base` of the five-case kit, made by `scripts/kit/load_orders.sh` and `scripts/kit/save_state.sh`. Then, from `rust/ifcap-po-inquiry/`:

```text
bash tests/golden/ifcap/corpus/scripts/load_corpus.sh
bash tests/golden/ifcap/corpus/scripts/record_corpus.sh
bash tests/golden/ifcap/corpus/scripts/check_corpus.sh
cargo test --offline --locked
```

- Then commit the recording: `BASE.txt`, the transcripts, `meta.txt`, `run-summary.txt` and `SHA256SUMS`. `captures-work/corpus/` is never committed.
- Each script changes to the crate folder itself, so it can be started from any directory. The fence shows the commands from the crate folder.
- `load_corpus.sh` and `record_corpus.sh` take three environment overrides: `NAME` (default `vehu`), `BASE_IMAGE` (default `worldvista/vehu:202609`) and `SOURCE_REVISION` (default `dd3fd0f3124c8178657d9fff4173c324da30f7ae`). `NAME` names a container the scripts remove and replace. Each script stops before its first removal unless no container has that name or the one that has it was started from `vehu-snap:gc-base` or `vehu-snap:corpus-base` (or an image made from one), and stops rather than act on `NAME` once `NAME` no longer names the container the script started.
- On Apple silicon each `docker run` carries `--platform linux/amd64` through the kit's `vc_platform_args`.
- The two Docker scripts source `scripts/kit/vehu_common.sh` unedited, and stop with `FAIL:` when it is missing. `check_corpus.sh` sources nothing and runs no Docker command.

### C6. The scripts

Each script prints each step as `== <n>. <name>`, numbered as below. Paths are as in C2.

- **Failure kinds:**
  - **Fatal.** A missing file or image, an id mismatch, a failed `docker` command, a readiness timeout, a start command that is not this README's, an empty or erroring export, or a load or read-back error. The script prints `FAIL: <cause>` and exits 1 at once.
  - **Collected.** Export before versus after and run 1 versus run 2 in `record_corpus.sh`, and every problem in `check_corpus.sh`. They are written out after every step has run, and the script then exits 1.
  - **Paging quarantine.** A classification, never by itself an exit 1.
- **Recording evidence is kept as it is.** A non-zero `docker exec`, or a transcript line beginning `%YDB-E-`, is still recorded byte for byte. No script skips, retries or edits a case.

**`load_corpus.sh`** loads world B into a throwaway container. It is not a recording run, so it may run M and wait for readiness with the kit's M probe, `vc_wait_ready`.

1. **Prerequisites.** Require `tests/fixtures/ifcap-corpus/corpus.zwr` with at least one data line, and the image `vehu-snap:gc-base`. Only the data lines, those beginning `^`, are loaded: the two header lines are skipped. Before anything is copied, stop if a file the script reads or writes, or a folder above it inside the crate folder, is a symbolic link or of the wrong kind.
2. **Throwaway container.** Restore container `NAME` from `vehu-snap:gc-base`, once any container already named `NAME` is shown to come from `vehu-snap:gc-base` or `vehu-snap:corpus-base` (C5), and wait up to 600 s for it.
3. **Corpus orders must be absent.** Stop if any order 991001 to 991200 already exists, found by one `$D` loop.
4. **Load.** Load the data lines of `corpus.zwr` by the `scripts/kit/load_orders.sh` method [scripts/kit/load_orders.sh:94]: each line prefixed with `set ` (one trailing space) and piped into `mumps -dir`. Stop on `%YDB-E` or `%GTM-E`.
5. **Read back.** Read `^PRC(442)` back with `zwrite`, and stop unless every loaded line returns byte for byte.
6. **Freeze.** `docker stop` and `docker commit` the container as `vehu-snap:corpus-base`, and take its id.
7. **Base export.** Start a fresh container from the snapshot, and export the Part B step 1 scope to `captures-work/corpus/base-export.zwr`. It must hold a data line and no M error. Hash it with `shasum -a 256`.
8. **`BASE.txt`.** Write it, then remove the container.

**`record_corpus.sh`** records every case in one batch, twice:

1. **Prerequisites.** Require `BASE.txt` with its `snapshot_id` and `export_sha256` values, `manifest.tsv` with its header and valid case rows, and every case's `input.txt`. Require the image `vehu-snap:corpus-base`, with an id equal to `BASE.txt` `snapshot_id`, and the original image `BASE_IMAGE` in the local Docker, for its digest. Before any container work, stop if a file the script reads or writes, or a folder above it inside the crate folder, is a symbolic link or of the wrong kind.
2. **Container for run 1.** Restore from `vehu-snap:corpus-base`, once any container already named `NAME` is shown to come from `vehu-snap:gc-base` or `vehu-snap:corpus-base` (C5). Readiness is probed from the shell only: `test -f /home/vehu/etc/env` every 5 s for up to 600 s (`SPEC.md` O-16), because a recording run runs no M command other than the start and export commands. The probe proves the shell and the environment file, not the database: the export check of step 4 and the two-run identity are the guard.
3. **Start command.** Detect it with the kit's `vc_detect_inquiry`, and stop unless it is Part B step 1's.
4. **Export before.** Export the scope once to `captures-work/corpus/export-before.zwr`, and stop if the hash differs from `BASE.txt` `export_sha256`. Then remove the run files of any earlier run, so an interrupted run leaves transcripts with no listing, which the corpus reader rejects.
5. **Run 1.** Every case in manifest order, as `docker exec -i <NAME> su - vehu -c "<start command>" < <id>/input.txt > <id>/transcript.txt`, with standard error to `captures-work/corpus/run1/` and each exit status kept.
6. **Export after.** Export once more, to `captures-work/corpus/export-after.zwr`, and compare it with the export before by `cmp`.
7. **Run 2.** Restore a fresh container, probe it the same way, and run every case again into `captures-work/corpus/run2/`. Compare each transcript pair with `cmp`.
8. **Paging detector.** Apply it (C7).
9. **`run-summary.txt`.** Write it.
10. **`meta.txt`.** Write it.
11. **`SHA256SUMS`.** Write it, last.
12. **Result.** Remove the container. Exit 0, with `RESULT: RECORDED <n> CASES`, only when the exports and the two runs are identical, whatever is quarantined and whatever exit statuses the cases recorded. Otherwise list the differences, print `RESULT: DIFFERENCES FOUND` and exit 1.

One export pair serves the whole run: per-case exports of 231 cases would add over 500 MB.

**`check_corpus.sh`** makes offline file checks, with no container and no M command, by the integrity rules of the corpus reader (C2). It writes nothing. It rejects every symbolic link it finds at the paths it inspects, and opens a file only after finding it a regular file, not a link. These checks, like its size measurements, are not atomic with the opens they guard, and, unlike the corpus reader, it makes no check after a read that the path still holds the file it found, so it assumes a stable tree. It collects every problem:

1. **Corpus folder.** `tests`, `tests/golden`, `tests/golden/ifcap` and `corpus/` are each a folder and not a symbolic link, and `corpus/` can be listed. This runs before anything is read; if it fails, every later step prints `skipped: … (counted in step 1)` and reads nothing.
2. **Manifest.** `manifest.tsv`, within 1,048,576 bytes, has its header, each case id is one of the 231 corpus ids (`CP-001` to `CP-200`, `CE-01` to `CE-26`, `CR-01` to `CR-05`), listed once, and every corpus id is listed.
3. **Case files.** Each manifest case has `input.txt` and a non-empty (`test -s`) `transcript.txt`. Each of the two that is a regular file holds at most 1,048,576 bytes and is UTF-8, checked by one `wc -c` and one `grep` over all of them.
4. **Inventory.** One walk of `corpus/`, within 4,096 entries and 8 folder levels: it holds only folders, each listed in full, and regular files, and no file holds `\r`. A folder past either budget is reported and not listed. A file is searched for `\r` only once it is measured within its budget: 65,536 bytes for a run file or `BASE.txt`, and 1,048,576 for any other, the largest budget of any corpus file. A file over it, or one that cannot be measured, is reported as not searched. Every measurement is `wc -c` given the file as an argument, which takes its size without reading it through.
5. **Run files.** `meta.txt`, `run-summary.txt` and `SHA256SUMS` exist as regular files.
6. **`meta.txt`.** Within 65,536 bytes and UTF-8. Each of its five keys appears once, and no other key appears. Each non-blank line must hold `=`, and the key is the text before the first `=`.
7. **`run-summary.txt`.** Within 65,536 bytes and UTF-8. It holds `exports=IDENTICAL`, `runs=IDENTICAL` and an `export_sha256=` of 64 lower-case hex digits, once each, and one `<id><TAB><RECORDED|QUARANTINED>` line for each manifest case and for no other id; the exit-status fields are not read. A `DIFF<TAB><subject>` line must name `export` or a manifest case, and is a problem, naming its line, when the first `exports=` and `runs=` are both `IDENTICAL`, since `record_corpus.sh` writes one only beside a `DIFFERENT` identity line. Any other line is a problem.
8. **`SHA256SUMS` verification.** Within 65,536 bytes and UTF-8. Every non-blank line is 64 lower-case hex digits, then two spaces or a space and `*`, then a path, listed once. The path is `BASE.txt`, `meta.txt`, `run-summary.txt`, or a manifest case's `input.txt` or `transcript.txt`. The lines that name a regular file reached through no symbolic link, and that an earlier check measured within its own budget (1,048,576 bytes for a case file, 65,536 for a run file or `BASE.txt`), go to `shasum -a 256 -c` inside `corpus/`, which must confirm each one. Any other listed file is reported as not hashed. A listed `BASE.txt` holds at most 65,536 bytes and is UTF-8, checked before it can be handed over. So a file over its budget, or one that cannot be measured, is reported and neither searched nor hashed.
9. **Every transcript listed.** Each `transcript.txt` the step 4 walk found, in any folder, is listed in `SHA256SUMS`, and each folder step 4 could not list is reported again. No second walk is made.

It then prints one `PROBLEM: <cause>` line per problem, showing a newline or carriage return as `\n` or `\r` and every other byte outside printable ASCII as `\xNN`, then `CORPUS CHECK PASS <n> CASES` (exit 0) or `CORPUS CHECK FAIL <k> PROBLEMS` (exit 1). A PASS does not cover the corpus reader's input rules (`input.txt` is not empty, and its last input begins with `^`), or the equivalence harness's comparison of each committed `input.txt` with the generated model. `cargo test` applies both.

### C7. Paging detector

`record_corpus.sh` step 8 applies it to the cases of class IN, CP and CR, read from `manifest.tsv` column 2.

- Let a be the count of `ENTER '^' TO HALT: ` (one trailing space) in a transcript, and b the count of `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ` (one trailing space). When a > 2b, the transcript shows a paging prompt and the case is `QUARANTINED` in `run-summary.txt`.
- The second string contains the first, so each found order normally gives a = 2b.
- BOUNDARY cases are exempt, because CE-25's `^` at the first pause legitimately skips the second.

### C8. Verdicts

`tests/equivalence.rs` judges every corpus case on world B, comparing raw bytes with no normalisation. There are five verdicts: NOT COMPARED, QUARANTINED, MATCH, MISMATCH and BOUNDARY.

**Precedence.** The first rule that applies wins:

1. A BOUNDARY-class case is `BOUNDARY`.
2. A failed integrity check, of the recording run or of the case itself, makes the case `NOT COMPARED`.
3. No `transcript.txt` makes the case `NOT COMPARED`.
4. A case `run-summary.txt` marks `QUARANTINED` is `QUARANTINED`.
5. Otherwise the Rust session runs with the committed inputs, and the case is `MATCH` when its transcript equals the capture byte for byte, or `MISMATCH`, with the first differing line attributed as in `REPORT.md` part 2. A `%YDB-E-` line in a capture, or a Rust session error, is a MISMATCH like any other difference.

- `MATCHED n OF m IN-SCOPE CORPUS CASES`: m counts the CP and IN cases whose verdict is MATCH or MISMATCH, those that are not quarantined and have a usable capture (one that passed the recording-run and case integrity checks), and n counts those that MATCH. A NOT COMPARED case, including one whose capture is present but whose recording-run or case integrity check failed (precedence rule 2), and a QUARANTINED case count in neither. With no recording it reads `MATCHED 0 OF 0 IN-SCOPE CORPUS CASES`.
- **CR world labels.** Each CR capture is compared byte for byte with the raw `GC-0N/transcript.txt` of the case it replays:
  - equal gives `WORLD-INDEPENDENT`;
  - different gives `WORLD-DEPENDENT`, with the first differing line;
  - a failed integrity check, no CR transcript or a GC capture that does not load gives `NOT COMPARED`.
- `REPORT.md` part 3 shows the results under `### Parity corpus`, after the capture-readiness tables: the recording-run line, the count table, the `MATCHED` line, the per-case table, then `#### Class moves`, `#### Findings`, `#### Dropped variants`, `#### BOUNDARY map` and `#### CR world labels`.
- No verdict fails `cargo test`: only an integrity failure does. Correcting mismatches is a later round.

### C9. Part A conventions

- **Carried over unchanged:**
  - `\n` line ends only, and no `\r`;
  - `docker exec -i`, never `-t`;
  - one input per line, each followed by `\n`, split by `split_input_lines`;
  - the transcript is standard output only, with standard error kept apart, never cut and never normalised.
- **Different:**
  - two files per case, `input.txt` and `transcript.txt`: no `returned.txt`, no exports and no `normalisation.txt`;
  - no normalisation rules;
  - the last input begins with `^` rather than being exactly `^`, because CE-02 exits with `^FOO`;
  - one export pair per run, kept in `captures-work/corpus/`, not per case;
  - a run-level `meta.txt` with five keys instead of the eight `META_KEYS`;
  - no READY/INCOMPLETE readiness checks: the integrity rules of C2 take their place.

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

