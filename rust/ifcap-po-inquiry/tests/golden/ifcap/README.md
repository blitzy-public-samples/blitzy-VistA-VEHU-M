# Golden captures: PRCH DISPLAY PO (EN3^PRCHRPT)

This folder holds the golden captures of OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`) recorded on `worldvista/vehu:202609`, one directory per case, `GC-01/` to `GC-05/`. This README is for the capture owner, who records them, and for every reviewer: it states the conventions the capture readers enforce. It is documentation only, and no code reads it.

Each convention below belongs to the file named with it, and this README restates it. Where this README and the owning file differ, the owning file governs.

| Convention | Owner |
|---|---|
| `split_input_lines`, `parse_normalisation`, `normalise`, `parse_meta`, `Meta`, `validate_context`; the 400 and 502 error texts | `src/bridge.rs` |
| `STUB_CONTEXT` | `src/inquiry.rs` |
| `Store::from_zwr`, `load_fixtures` | `src/store.rs` |
| `evaluate_case`, `comparison_report` | `tests/equivalence.rs` |
| `gc_02_end_to_end_compare` | `tests/bridge.rs` |
| Fixture lines and their field order | `tests/fixtures/ifcap/MANIFEST.txt` |
| Contracts PO-01 to PO-41 and their branch IDs, resolutions R1 to R6, open items O-01 to O-08, conflicts C1 to C17 | `SPEC.md` |
| `EXIT_INPUT` | `frontend/app.js` |

**Conventions of this README.**

- Paths are relative to the crate folder `rust/ifcap-po-inquiry/`, except those that start with `Packages/`.
- Source is cited as `[path:line]`, with repository-root paths at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`.
- Byte notation: `\n` is a newline (0x0A), `<BEL>` is `$C(7)` (0x07) and `<TAB>` is a tab (0x09). Spaces inside code spans are literal, and each trailing space is also stated in words.
- Status words have the meanings given in section 10 and are never merged: NOT COMPARED, FAIL, PASS, BLOCKED, NOT RUN, PENDING.
- "n/e" marks a branch that is not exercised and not implemented. The Rust side returns `UnsupportedBranch` with its branch ID when it reaches one, and also when it reaches a BLOCKED branch.

## 1. Owner, deadline and status

- **Owner.** Rainman Kemp prepares the test data and records all five captures on `worldvista/vehu:202609`. From the same snapshot he exports `tests/fixtures/ifcap/cases.zwr`, adds its line to `tests/fixtures/ifcap/MANIFEST.txt`, and loads the captures into `GC-01/` to `GC-05/`.
- **Deadline.** November 13, 2026, 5:00 p.m. Eastern.
- **Status.** Assigned; captures pending. Until the captures exist, every case is NOT COMPARED, and overall acceptance stays PENDING (B1, B2).
- **Directories.** The run wrote nothing in this folder except this README: no `GC-0N/` directory, no `.gitkeep` and no placeholder file (conflict C2). A missing directory reads as all seven capture files missing, so its case is NOT COMPARED. The owner creates the directories.

## 2. Cases

The user's examples, verbatim:

- GC-01: an existing order with one line (2 × $125.50).
- GC-02: an existing order with three lines (3 × $19.95, 2 × $125.50, 1 × $0.99).
- GC-03: a syntactically valid order identifier confirmed absent from the fixture database.
- GC-04: `INVALID!` at the order-selection prompt, then the supported exit input.
- GC-05: an existing cancelled order with retained line-item history.
- The fixture totals, $251.00 for GC-01 and $311.84 for GC-02, are sanity checks only. The captures decide.

Notes from source:

- **GC-05 status message.** Status `Cancelled Order` is `^PRCD(442.3,45,0)`, whose supply status order (field .02) is 45 [Packages/IFCAP/Globals/442.3+PURCHASE ORDER STATUS.zwr:26] [Packages/VA FileMan/Globals/DD.zwr:518035]. `EN30` writes `<BEL>` and ` >>> Status makes this record non-specifiable here.` (one leading space) only when that order is below 10 [Packages/IFCAP/Routines/PRCHRPT.m:73], so for 45 no such line is predicted. The order must still pass the lookup screen of section 5, including its MOP condition.
- **GC-05 profile risk.** The profile of section 5 requires no node 6 (field 50 AMENDMENT [Packages/VA FileMan/Globals/DD.zwr:514981]), no node 11 (field .6 PARTIAL [Packages/VA FileMan/Globals/DD.zwr:514467]) and no item partial receipts (item node 3, field 20 DATE RECEIVED [Packages/VA FileMan/Globals/DD.zwr:516311]). A cancelled order with retained history may carry any of them. If it does, the case leaves the profile and will FAIL. Record that outcome as it is, and never alter data to make the order fit.

## 3. Files per case

Each `GC-0N/` directory holds exactly these seven files:

| File | Content | Read by |
|---|---|---|
| `input.txt` | The typed inputs, one per line | Bridge replay mode; `tests/equivalence.rs`; `tests/bridge.rs` (GC-02) |
| `transcript.txt` | The original's standard output | Bridge replay mode; `tests/equivalence.rs`; `tests/bridge.rs` (GC-02) |
| `returned.txt` | The return value, zero bytes for none | Bridge replay mode; `tests/equivalence.rs`; `tests/bridge.rs` (GC-02) |
| `db-before.zwr` | ZWR export of the section 8 scope immediately before the session | `tests/equivalence.rs` |
| `db-after.zwr` | The same export immediately after the session | `tests/equivalence.rs` |
| `meta.txt` | Seven `key=value` lines describing the run | Bridge live mode; `tests/equivalence.rs` |
| `normalisation.txt` | Normalisation rules, expected empty | Bridge replay mode; `tests/equivalence.rs`; `tests/bridge.rs` (GC-02) |

### `input.txt`

- One input per line, with `\n` line endings and no carriage return. The sequences are in section 4.
- `split_input_lines` in `src/bridge.rs` splits the file on `\n`:
  - one trailing `\n` ends the last line and adds no empty input;
  - inner empty lines are inputs, each one a RETURN;
  - an empty file holds no input;
  - a carriage return is part of its line and is never stripped.
- End the file with exactly one `\n` after the final `^`. A second trailing `\n` adds an extra empty input.
- No input may contain a carriage return or a line feed. The bridge rejects such a request element with 400 (`INPUT <n> CONTAINS A CARRIAGE RETURN OR LINE FEED`). Live mode writes each input followed by one `\n` to the original's standard input, which is the framing of this file.

### `transcript.txt`

- The exact standard-output bytes of the original session, with `\n` line endings and **no echo of typed input**. That is why the capture runs under `docker exec -i` without `-t` (section 7): a pseudo-terminal echoes input and can rewrite line endings.
- `<BEL>` stays byte 0x07. Standard error is not part of the transcript.
- The file must be UTF-8. Otherwise the bridge answers 502 `CAPTURE FILE <path>: NON-UTF8`.
- The capture command must add nothing of its own to standard output, so that the file holds only what the session writes. Output the command cannot suppress is recorded as an O-07 item for the team. It is never cut out by hand, and never by normalisation.

### `returned.txt`

- The return value with one trailing newline removed. Zero bytes means `None`.
- **Every case is zero bytes**, with no newline either: `EN3^PRCHRPT` is reached by `DO` and is not an extrinsic function [Packages/IFCAP/Routines/PRCHRPT.m:70-75].

### `db-before.zwr` and `db-after.zwr`

- ZWR exports of the section 8 scope, taken immediately before and immediately after the session, each in the form section 9 requires.
- They must be identical. The harness loads both with `Store::from_zwr` and compares them; on FAIL it names the first differing node.
- **Baseline identity.** Every `db-before.zwr` node must be present, with an identical value, in the store that `load_fixtures` builds from `tests/fixtures/ifcap/`.

### `meta.txt`

The format belongs to `parse_meta` and `Meta` in `src/bridge.rs`:

- `key=value` lines, each split at its first `=`. The value is kept exactly, so write no space around `=`.
- Exactly the seven keys below, each once. A missing, repeated or unknown key, or a line without `=`, makes the file malformed.
- Whitespace-only lines are ignored, and one trailing carriage return per line is dropped.

| Key | Value |
|---|---|
| `image_digest` | The digest of the `worldvista/vehu:202609` image the capture ran on (O-05) |
| `source_revision` | `dd3fd0f3124c8178657d9fff4173c324da30f7ae` when the runtime routines match that repository commit; otherwise the revision actually found, never this commit by default (O-05) |
| `snapshot_id` | The identifier of the database snapshot. It must equal the `snapshot_id` that every `MANIFEST.txt` line carries (section 9) |
| `record_ids` | The case's order: both its IEN and its full PO number (`500-…`). The key is required in every case. GC-03 and GC-04 select no order, and `parse_meta` accepts an empty value |
| `entry_point` | `EN3^PRCHRPT` |
| `context` | A JSON object whose values are all strings, with no repeated key (section 6) |
| `docker_exec_args` | A JSON array of strings (below) |

A template, with each `<…>` to be replaced by the recorded value:

```text
image_digest=<image digest>
source_revision=<revision>
snapshot_id=<snapshot id, equal to MANIFEST.txt>
record_ids=<IEN and full PO number>
entry_point=EN3^PRCHRPT
context={"DUZ":".5","IOF":"<IOF>","IOSL":"<IOSL>","STATION":"500"}
docker_exec_args=["<argument>","<argument>"]
```

**`docker_exec_args`.** Live mode runs `docker` with the argument vector `exec`, `-i`, `<container>`, followed verbatim by the elements of `docker_exec_args`. `<container>` comes only from the bridge's `VEHU_CONTAINER` setting (O-07), never from a capture or a request. Because the elements follow the container name, Docker options such as `--user vehu` cannot appear in them. If the capture needed such options, record that as an O-07 item for the team to confirm. This README sets no workaround.

### `normalisation.txt`

The format belongs to `parse_normalisation` and `normalise` in `src/bridge.rs` (AAP 0.5.4):

- The file is split on `\n`, and nothing is trimmed. A zero-length line is blank and ignored.
- Every other line holds exactly one `<TAB>`, separating two literal, non-empty delimiters: open, then close. A carriage return before the line end belongs to the close delimiter, so use `\n` line endings.
- A line with no tab, more than one tab, or an empty delimiter is malformed. The bridge answers 502 `MALFORMED NORMALISATION <path> line <n>`, with the 1-based line number, and the harness reports FAIL.
- Rules apply in file order, each to the result of the one before. A rule scans left to right and replaces the text from each open delimiter through the next close delimiter after it, both delimiters included, with `<NORM>`. Scanning resumes after the replaced text. An open delimiter with no later close delimiter leaves the rest of the text unchanged.
- Both sides of a comparison get the same rules, and no other transformation exists.
- **The file is expected to be empty** (zero bytes), and an empty file is valid. A rule may cover only run-time timestamps, internal record numbers and other variable fields, each documented in the rule's own case. It must leave order IDs, status, quantities, prices, amounts and error text intact. Each rule, and the original text it replaced, is reported in `REPORT.md`.

### How the readers use the captures

- **Replay mode.** For `/api/mumps/session` and `/api/compare/session`, the bridge scans `GC-01` to `GC-05` in order for an `input.txt` whose `split_input_lines` equals the request's inputs.
  - A directory whose `input.txt` is missing does not match. One whose `input.txt` is unreadable or not UTF-8 does not match either, and is listed.
  - With no match, the answer is 502 `NOT RECORDED`, or `NOT RECORDED (unreadable: <path>, <path>)` when such directories were listed.
  - In the matching directory, a missing, unreadable or non-UTF-8 `transcript.txt`, `returned.txt` or `normalisation.txt` gives 502 `CAPTURE FILE <path>: MISSING`, `CAPTURE FILE <path>: UNREADABLE` or `CAPTURE FILE <path>: NON-UTF8`. An empty `returned.txt` or `normalisation.txt` is valid.
  - A matching directory is used whole or not at all.
- **Compare precedence.** A MUMPS-side failure gives 502 first. Otherwise a Rust-side `CorruptStore` gives 500 and an `UnsupportedBranch` gives 422. Only then is the answer 200, whose `match` is `true` only when the transcripts and the return values are both equal after normalisation. Transcript-only agreement is never a match. While O-01 and O-03 stay open, `SPEC.md` predicts that the Rust side stops at `UnsupportedBranch("PO-12.b1")` for GC-01, GC-02 and GC-05, and at `UnsupportedBranch("PO-09.b14")` for GC-03 and GC-04, so compare on a captured sequence answers 422.
- **Live mode.** For the same two endpoints, the bridge reads `context` and `docker_exec_args` from the `meta.txt` of the first `GC-0N` directory that has one.
  - A malformed `meta.txt`, or none in any directory, gives 502 `MALFORMED META <path>`. With none present, `<path>` is where `GC-01/meta.txt` would be.
  - A `context` that differs from `STUB_CONTEXT` gives 502 `CONTEXT MISMATCH <key>: expected <value>, actual <value>` (section 6).
  - In either case nothing runs.
  - The live return value is unavailable (O-06), so live `returned` and compare `match` are `null`.
- **Replay mode never reads `meta.txt`**, and live mode reads nothing else from a capture.
- **Hashing.** This README sits outside `GC-0N/`, so the bridge never scans it, and `sha256sum tests/golden/ifcap/GC-0*/*` never hashes it.

### Who reads captures

- Only these read golden content (AAP 0.10.4):
  - the replay, compare and live adapters in `src/bridge.rs`;
  - `tests/equivalence.rs`;
  - `gc_02_end_to_end_compare` in `tests/bridge.rs`.
- `inquire` and `Store` never read captures. The A5 static search of `src/` checks this.
- Nothing in the crate writes into `GC-0N/`. The synthetic tests in `tests/bridge.rs` create their own temporary directories under `std::env::temp_dir()`, each named with `synthetic` and marked by `SYNTHETIC.txt`, and never touch this folder. Synthetic fixtures are routing tests, never parity evidence.
- A capture that disagrees with a source-derived contract keeps its case FAIL and reopens that contract and the baseline investigation (AAP 0.10.3). A capture never silently overrides source, and normalisation is never widened to force a match.

## 4. Input sequences (conflict C9)

**Why the request's sequence is corrected.** The request's two-line `input.txt`, an identifier line and then an exit line, does not end a session cleanly in source. Every session first answers the station prompt, and a found order pauses twice and then re-prompts. Source governs, as `SPEC.md` (section "Source corrections and interface departures") and `REPORT.md` record.

**The prompts the inputs answer.** Each prompt is shown with its exact bytes. None is followed by a newline: the READ happens on the same line.

| Prompt, exact bytes | Trailing space | Written just before it | Answer | Source |
|---|---|---|---|---|
| `Select STATION NUMBER ('^' TO EXIT): ` followed by the default `500// ` | The prompt ends with one space after the colon; the default ends with one space after `//` | `\n\n\n` | An empty line, accepting the default 500 | [Packages/IFCAP/Routines/PRCFSITE.m:8, 14, 18] [Packages/VA FileMan/Routines/DIC.m:25] [Packages/VA FileMan/Routines/DIC11.m:15-17] |
| `P.O./REQ.NO.: ` | One space after the colon | `\n\n` [Packages/IFCAP/Routines/PRCHPAT.m:83], then `\n` [Packages/VA FileMan/Routines/DIC.m:25] | The order's `C` value, or `^` | [Packages/IFCAP/Routines/PRCHRPT.m:12] [Packages/IFCAP/Routines/PRCHPAT.m:81] [Packages/VA FileMan/Routines/DIC11.m:15-17] |
| `ENTER '^' TO HALT: ` | One space after the colon | `\n`, then spaces to column 8 | An empty line | [Packages/IFCAP/Routines/PRCHDP2.m:10] |
| `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ` | One space after the colon | `\n` and `<BEL>` | An empty line | [Packages/IFCAP/Routines/PRCHDP2.m:41] |

Wherever these prompts are quoted below, they keep the trailing space stated here.

Echoes between the prompts, as `SPEC.md` resolves them (open item O-02 is closed by its resolutions R1 to R3, and a capture that disagrees reopens it):

- **Station default accepted (R1).** `      CAMP MASTER`: six leading spaces, no trailing space and no newline [Packages/VA FileMan/Globals/DD.zwr:488724] [Packages/Kernel/Globals/4+INSTITUTION.zwr:494]. The plan's prediction, `  500  CAMP MASTER`, is superseded by R1.
- **Unique order match (R2).** Two spaces and the full `.01` value, then two spaces before each identifier's output [Packages/VA FileMan/Globals/DD.zwr:514202-514204]:
  - `Z.01`: the order date as `MM-DD-YY`;
  - `Z.02`: the MOP abbreviation, then three spaces and the status name;
  - `Z.03`: `\n`, 13 spaces, `FCP: ` (one trailing space), the first word of the fund control point (field 1), `     $ ` (five spaces, `$`, one space) and the total with two decimals. No newline follows.
- **No match (R3).** `<BEL> ??\n\nP.O./REQ.NO.: `, ending with the prompt's one trailing space, then the next READ [Packages/VA FileMan/Routines/DIC.m:25, 63-65, 80-81].

**First line of every case.** An empty line answers the station prompt. The site file holds six sites [Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr:3], more than one, so the prompt always appears [Packages/IFCAP/Routines/PRCFSITE.m:9, 13, 18]. Its default is 500, the first primary station [Packages/IFCAP/Routines/PRCFSITE.m:14] [Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr:290].

**Identifier.** The value typed is the common-numbering part after `500-`, the index `C` value `$P(X,"-",2)` [Packages/VA FileMan/Globals/DD.zwr:514319] [Packages/IFCAP/Routines/PRCHPAT.m:80]. Typing the full `500-…` number is predicted to find nothing (AAP 0.3.6 B; `SPEC.md` R5).

**GC-01, GC-02 and GC-05: five lines.**

1. Empty, the station default.
2. The order's `C` value.
3. Empty, answering `ENTER '^' TO HALT: `. Any answer containing `^` would halt the display before the line items [Packages/IFCAP/Routines/PRCHDP2.m:10].
4. Empty, answering `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: `.
5. `^`, at the order prompt that `G EN30` asks again [Packages/IFCAP/Routines/PRCHRPT.m:75].

**GC-03: three lines.**

1. Empty, the station default.
2. A `C` value that is confirmed absent from the snapshot and meets these rules:
   - it fits the `C` part of the `.01` input transform, `Z?3UN3N`: three characters, each an uppercase letter or a digit, then three digits [Packages/VA FileMan/Globals/DD.zwr:514312];
   - it contains at least one letter (`SPEC.md` C9). The plan gave this rule to keep the IEN lookup from being taken. `SPEC.md` R5 found that `IX^DIC` never tries the value as an IEN at this prompt [Packages/VA FileMan/Routines/DIC.m:84-85] [Packages/VA FileMan/Routines/DIC5.m:22], and still requires the letter;
   - the snapshot meets the GC-03 and GC-04 conditions of section 5.

   This README prescribes no specific value. The owner chooses it and records it in `input.txt`. How the pointed-to files' own date and set-of-codes transforms treat the value is open item O-03 (`SPEC.md` R4).
3. `^`.

**GC-04: three lines.** Empty, `INVALID!`, `^`.

**The files, byte for byte:**

| Case | `input.txt` |
|---|---|
| GC-01, GC-02, GC-05 | `\n<C value>\n\n\n^\n` |
| GC-03 | `\n<C value>\n^\n` |
| GC-04 | `\nINVALID!\n^\n` (12 bytes) |

**Paging.** If the original pages (O-01: `IOSL`) [Packages/IFCAP/Routines/PRCHDP2.m:31, 54], record one more empty line for each extra `ENTER '^' TO HALT: ` prompt it writes.

**Exit input.** The exit input is `^` (the standard VistA up-arrow exit), decided here. Source confirms it: `^` at the order prompt ends the FileMan lookup with no output [Packages/VA FileMan/Routines/DIC11.m:23] [Packages/VA FileMan/Routines/DIC.m:45], and it is the same constant as `EXIT_INPUT = "^"` in `frontend/app.js`, which the front-end's `EXIT` button sends as an ordinary input.

## 5. Predicted-path fixture profile

The predicted coverage in `SPEC.md` assumes this profile (AAP 0.4.4; `SPEC.md`, section "Source corrections and interface departures", which owns the list). Each item keeps a case off the n/e branch named after it. A record outside the profile reaches that branch, and the Rust side returns `UnsupportedBranch` with its ID; the case then FAILs and its contract reopens. The branch IDs are those of `SPEC.md`.

**Every case.**

- `^PRC(442,0)` is present with piece 2 exactly `442I`, as in the repository export [Packages/IFCAP/Globals/442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3], and at least one `^PRC(442)` subscript collates after `"A["` (PO-09.b15, b16). The eleven repository fixtures do not hold `^PRC(442,0)`, so `cases.zwr` must (section 8).

**A found-order case (GC-01, GC-02, GC-05): the order and its records.**

- **Lookup screen.** The order passes it: station 500 in node 0 piece 1, and a MOP in node 0 piece 2 below 10 or equal to 25 [Packages/IFCAP/Routines/PRCHRPT.m:9] [Packages/IFCAP/Routines/PRCHPAT.m:81]. No other order shares its `C` value or has a `C` value beginning with it, and no later index holds a subscript beginning with it (PO-09.b9, b10).
- **Nodes present.** Nodes 0 and 1 are present (PO-10.b1 to b3).
- **Excluded nodes and field values:**
  - no node 11 (PO-10.b5, PO-34.b2);
  - the vendor is not EDI, or field 116 is `n` (PO-10.b6);
  - field 65 is not `RMPR` (PO-10.b7);
  - node 23 piece 7 is empty (PO-11.b2);
  - field .08 is not `Y` (PO-23.b2);
  - no node 5 (PO-18.b2), 3 (PO-25.b5), 15 (PO-25.b7), 4 (PO-31.b2) or 13 (PO-32.b2);
  - no node 6 (PO-32.b3, PO-33.b2, PO-38.b1).
- **Line items.** At least one line item (PO-25.b3). Every item has:
  - a zero node, and a node 2 whose EDI status codes are empty (PO-29.b1, b3);
  - description text that wraps to one line at width 33 under the PO-15 rules, so at most 32 characters in all (PO-27.b2, b3; PO-28.b3), with no `|` (PO-15.b2) and no `| IN ` (PO-28.b2);
  - no contract number (PO-20.b1);
  - no partial receipts (PO-27.b5).
- **Delivery schedule.** None under the order number in `^PRC(442.8,"B")` (PO-36.b3). The repository 442.8 export holds records 56 and 57 and no `B` index [Packages/IFCAP/Globals/442.8+DELIVERY SCHEDULE ORDER.zwr:3-5].
- **Left free and implemented.** The profile does not constrain these, and the port implements them: the MOP-4 and direct-delivery ship-to; the order-type labels; the `SIMPLIFIED` vendor; a verbal or confirmation copy; the status-order message; the estimated-shipping line; purchase-card item columns.

**GC-03 and GC-04: the snapshot.**

- No order has a `C` value, and no later index has a subscript, that begins with the typed value.
- No vendor or other pointed-to entry matches the value and is referenced by an order that passes the lookup screen (PO-09.b11).

## 6. Context

- **Value.** `meta.txt` `context` is `{"DUZ":".5","IOF":…,"IOSL":…,"STATION":"500"}`, with the HOME-device values recorded under section 7 in place of each `…`, each as a JSON string.
- **Why four keys.** The user example `{"DUZ":".5","STATION":"500"}` is extended with `IOF` and `IOSL` because the display reads them [Packages/IFCAP/Routines/PRCHDP1.m:12] [Packages/IFCAP/Routines/PRCHDP2.m:31, 42].
- **The constant.** `STUB_CONTEXT` in `src/inquiry.rs` is `[("DUZ", Some(".5")), ("IOF", None), ("IOSL", None), ("STATION", Some("500"))]`. `None` marks an unresolved value.
- **The comparison.** `validate_context` in `src/bridge.rs` compares the capture's `context` with `STUB_CONTEXT` key by key:
  - first each `STUB_CONTEXT` key in its order (`DUZ`, `IOF`, `IOSL`, `STATION`), then each capture key that `STUB_CONTEXT` lacks, in capture order;
  - a missing key, an extra key, a different value, or an unresolved constant fails, and the first mismatch is reported as `CONTEXT MISMATCH <key>: expected <value>, actual <value>`;
  - for an absent side the message shows `UNRESOLVED` (the expected value of an unresolved constant, which matches nothing), `MISSING` (the actual value of a key the capture lacks) or `ABSENT` (the expected value of an extra capture key);
  - no capture value overrides a constant.
- **Expected failures until O-01 closes.** `IOF` and `IOSL` stay unresolved until then, so **every capture FAILs with `CONTEXT MISMATCH IOF: expected UNRESOLVED, actual <value>`** until a recorded correction to `STUB_CONTEXT` lands. Live mode answers 502 with the same text and runs nothing. On the Rust side, GC-01, GC-02 and GC-05 also reach the BLOCKED `PO-12.b1`, and GC-03 and GC-04 the BLOCKED `PO-09.b14` while O-03 stays open (`SPEC.md`, section "Source verification").
- **No control point** is needed: the path reads none and prompts for none (conflict C11).

## 7. Capture-command preconditions (O-01, O-07)

The exact capture command is the team's to confirm (O-07). This section states what source requires of it and claims no command as verified.

- **Sign-on.** Use `D DUZ^XUP(.5)`, the user POSTMASTER [Packages/Kernel/Routines/ZOSVGUT1.m:13] [Packages/Kernel/Globals/200+NEW PERSON.zwr:4, 12]. `DUZ^XUP` sets `DUZ` and its subscripts but no `DTIME` and no `IO*` variable [Packages/Kernel/Routines/XUP.m:54-64], so the command must also set:
  - `U="^"`;
  - `DT`, today's date in FileMan format. Without it `^PRCFSITE` calls `NOW^%DTC`, the n/e branch PO-03.b2 [Packages/IFCAP/Routines/PRCFSITE.m:7];
  - `DTIME`, the READ timeout [Packages/IFCAP/Routines/PRCHDP2.m:10, 41] [Packages/VA FileMan/Routines/DIC11.m:17]. The Rust READ never times out, so no READ may time out during a capture.

  `SPEC.md` O-07 also records that `DIC`, `DO` and `DST` are undefined at option entry.
- **HOME device (O-01).** `D ^%ZIS` runs with `IOP="HOME"` [Packages/IFCAP/Routines/PRCHDP1.m:12].
  - It uses the home-device variables that sign-on saves under `^XUTL("XQ",$J)` when they exist [Packages/Kernel/Routines/ZIS.m:28, 82-86]. Otherwise it looks the principal device up, and can write a warning that begins `HOME DEVICE (` to it when the device is not in the DEVICE file [Packages/Kernel/Routines/ZIS.m:31-36].
  - `IOF` and `IOSL` come from those saved variables, or from pieces 2 and 3 of node 1 of the terminal type [Packages/Kernel/Routines/ZIS2.m:55-60] [Packages/Kernel/Routines/ZIS1.m:103]. An empty `IOF` becomes `#` [Packages/Kernel/Routines/ZIS1.m:106, 113].
  - Fix the HOME device, and record its terminal type and its `IOF` and `IOSL` values. The candidates are in `Packages/Kernel/Globals/3.2+TERMINAL TYPE.zwr` (`^%ZIS(2,…)`) and `Packages/Kernel/Globals/3.5+DEVICE.zwr` (`^%ZIS(1,…)`). Closing O-01 also needs the first capture to confirm the bytes `W @IOF` writes (`SPEC.md` O-01).
- **Entry.** Then `D EN3^PRCHRPT`. Nothing the command runs before or after it may write to standard output (section 3).
- **Invocation.** Run with `docker exec -i`, and **never `-t`**. The plan's research found the WorldVistA batch pattern `docker exec -i --user vehu <container> … '. /home/vehu/etc/env && yottadb -run …'` (AAP 0.4.6). It is not verified here. Feed `input.txt` on standard input, and save standard output byte for byte as `transcript.txt`. Keep standard error apart.
- **Record.** Put the arguments that follow the container name in `meta.txt` `docker_exec_args` (section 3), and the context in `context` (section 6). Options placed before the container name, such as `--user vehu`, cannot be recorded there and are an O-07 item.

## 8. Export scope (`db-before.zwr`, `db-after.zwr`, `cases.zwr`)

- **Include** the case's business globals and their pointer targets:
  - the `^PRC(442,0)` file header (section 5);
  - the `^PRC(442,ien,…)` record, with its `C` entry and its entries in the other `^PRC(442)` indexes;
  - the referenced `^PRC(440)` vendor;
  - `^PRC(440.2)` and `^DPT`, when field 5.3 is set;
  - the `^VA(200)` buyer or agent;
  - the `^PRC(411)`, `^PRCD(442.3)`, `^PRCD(442.5)`, `^PRC(442.4)`, `^PRCD(420.5)`, `^PRCD(420.8)`, `^DIC(5)` and `^DIC(49)` nodes the case reads.
- **Excluded**, and stated here as excluded:
  - `^DISV`, which FileMan writes on every successful lookup [Packages/VA FileMan/Routines/DIC2.m:65];
  - `^TMP`, `^UTILITY` and `^XUTL`, which are scratch.

  None of them is business data, and leaving them out keeps the before and after exports comparable.
- **No orders in the repository.** The repository snapshot holds no purchase order: the `^PRC(442,0)` count is 0 [Packages/IFCAP/Globals/442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3]. The five orders reach the fixtures only through `cases.zwr`. It must therefore hold every `db-before.zwr` node that the eleven repository fixtures lack, `^VA(200)` and `^PRC(442)` included.
- **Runtime values.** Where the runtime snapshot holds a different value for a node the repository fixtures also hold, `cases.zwr` carries the runtime value. It loads last, so that value wins (section 9).

## 9. `cases.zwr` and its `MANIFEST.txt` line

- **Loader rules.** Export `cases.zwr` in ZWR form, UTF-8, in a form `Store::from_zwr` in `src/store.rs` accepts:
  - header lines before the first line that starts with `^` are ignored, and so are blank lines;
  - every other line is `^name(subscripts)=value`. A malformed line is an error naming the file and its 1-based line;
  - a repeated key replaces the earlier value, and one trailing carriage return per line is dropped;
  - an unquoted subscript must be a canonical number: no leading zero, no trailing zero after a decimal point, no leading `+`. A quoted subscript is a string, except that a quoted canonical number is stored as a number;
  - inside quotes, `""` stands for one `"`;
  - a value is a quoted string or a canonical number;
  - `$C(n)` and `_` concatenation are rejected. ZWRITE writes a value holding control characters in that form, so such a node makes the file unloadable and every case then FAILs. Report the node to the team, and do not edit the export.
- **Load order.** `load_fixtures` loads every `*.zwr` file of `tests/fixtures/ifcap/` in byte order of name, with `cases.zwr` last, so the runtime export wins on repeated keys.
- **Manifest line.** Add one line to `tests/fixtures/ifcap/MANIFEST.txt`, ending in `\n`, with four `<TAB>`-separated fields in the order of the existing lines: file name, origin, `snapshot_id`, SHA-256 hex digest. An existing line, and the line to add:

  ```text
  411.zwr<TAB>repository Packages/IFCAP/Globals/411+ADMIN. ACTIVITY SITE PARAMETER.zwr<TAB>git:dd3fd0f3124c8178657d9fff4173c324da30f7ae<TAB><64 hex digits>
  cases.zwr<TAB>runtime export<TAB><snapshot_id><TAB><64 hex digits>
  ```

  The digest is the first field of `sha256sum tests/fixtures/ifcap/cases.zwr`.
- **Snapshot identity.** Every `MANIFEST.txt` line, the `cases.zwr` line included, and every `meta.txt` must carry one `snapshot_id`. The repository fixtures carry `git:dd3fd0f3124c8178657d9fff4173c324da30f7ae`. Whether the runtime database matches that commit is open item O-05: record the identifier the snapshot really has. A mismatch then FAILs baseline identity.
- **Consistency.** A `MANIFEST.txt` that lacks the line for any required fixture, `cases.zwr` included, is inconsistent, and a fixture whose digest differs from its line fails too. Either gives FAIL.

## 10. Harness outcomes (AAP 0.8.3)

`evaluate_case` in `tests/equivalence.rs` gives each case one of four outcomes:

- **NOT COMPARED.** A required file is missing:
  - the case's seven capture files;
  - the eleven repository fixtures: `411.zwr`, `411.2.zwr`, `420.5.zwr`, `420.8.zwr`, `440.zwr`, `442.3.zwr`, `442.4.zwr`, `442.5.zwr`, `442.8.zwr`, `5.zwr`, `49.zwr`;
  - `cases.zwr`;
  - `MANIFEST.txt`.

  The missing-file list is reported. A file that is present never gives NOT COMPARED.
- **FAIL.** The capture differs; or it is malformed, inconsistent, baseline-mismatched or has a context mismatch; or the Rust side reaches a BLOCKED or n/e branch. The report names the `TAG^ROUTINE`, the contract and branch IDs, and the first differing line or byte.
- **PASS.** A complete capture matches after normalisation on the transcript, on `returned` and on both database checks.
- **BLOCKED.** A tool, the toolchain or a contract value is unavailable.

Two more words apply outside the harness:

- **NOT RUN.** A manual or live check was not executed; the reason is given.
- **PENDING.** Overall acceptance, until all five cases PASS and the manual (B3, B4) and live checks are complete.

The outcomes are never merged. The equivalence suite exits non-zero on any FAIL, and NOT COMPARED does not fail it. Exit 0 never means acceptance.

**Who reports.**

- `comparison_report` in `tests/equivalence.rs` writes the per-case results into `REPORT.md`, between `<!-- COMPARISON BEGIN -->` and `<!-- COMPARISON END -->`, and nowhere else.
- `gc_02_end_to_end_compare` in `tests/bridge.rs` posts GC-02's inputs to `/api/compare/session` and is reported apart from the synthetic tests. It prints NOT COMPARED when GC-02's prerequisites are absent.
