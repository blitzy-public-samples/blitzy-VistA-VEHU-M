# Parity testing method

## 1. Principle
The MUMPS original, `PRCH DISPLAY PO` (`EN3^PRCHRPT`) on `worldvista/vehu:202609`, is the oracle. The Rust port is equivalent for a case only when its transcript, return value and unchanged store equal the case's capture after that capture's `normalisation.txt`. Rules come from source; golden content only judges.

## 2. Method
- (a) Choose each case's inputs and records (`tests/golden/ifcap/README.md` Part B step 1).
- (b) Run the original once per case with `docker exec -i` and save the seven capture files: `input.txt`, `transcript.txt`, `returned.txt`, `db-before.zwr`, `db-after.zwr`, `meta.txt`, `normalisation.txt`.
- (c) Run it twice to find variable fields; only those go into `normalisation.txt`.
- (d) Load the fixtures plus `cases.zwr` into the Rust `Store`.
- (e) Run `inquire` on the same inputs.
- (f) Compare normalised transcripts, `returned`, the Store against its pre-call clone, and `meta.txt` `context` against `STUB_CONTEXT`; write the verdict to `REPORT.md`.

## 3. Cases
`""` is an empty line; settling checks are `REPORT.md` part 2 verdicts.

| Case | `input.txt` lines | The original must show | Settling check |
|---|---|---|---|
| GC-01 | `""`, `GC1001`, `""`, `""`, `^` | One line, quantity 2 at $125.50, total $251.00, status Ordered and Obligated | PASS |
| GC-02 | `""`, `GC2002`, `""`, `""`, `^` | Three lines, 3 × $19.95, 2 × $125.50 and 1 × $0.99, total $311.84 | PASS |
| GC-03 | `""`, `GC3003`, `^` | A valid identifier confirmed absent: no-match `<BEL> ??`, then the re-asked prompt | PASS |
| GC-04 | `""`, `INVALID!`, `^` | The same no-match response, then exit | PASS |
| GC-05 | `""`, `GC5005`, `""`, `""`, `^` | An existing cancelled order (`Cancelled Order`) with its two retained line items | PASS |

## 4. Verdicts
- **NOT COMPARED:** the case folder, its `input.txt` or its `transcript.txt` is absent.
- **PASS:** the case is READY, normalised transcripts are equal, `returned` is equal, the store is unchanged and `validate_context` accepts the context.
- **FAIL:** any other recorded case, including one that is unreadable or not READY (conflict K12).
- READY and INCOMPLETE describe captures. READY means all six `golden::CHECKS` pass: `seven_files_present`, `meta_keys_present`, `input_ends_with_exit`, `normalisation_parses`, `db_before_equals_after`, `no_carriage_return`. INCOMPLETE lists each failed check.
- `tests/equivalence.rs` fails only on a FAIL or a failed harness check, so without captures `cargo test` exits 0. Acceptance needs five PASS.

## 5. Layers
- Unit tests in `src/` show the code follows this run's reading of source; not parity.
- `tests/equivalence.rs` gives parity for each READY case.
- `tests/bridge.rs` shows the bridge contract holds; not parity.
- Its `end_to_end_real_captures` gives parity through the bridge once READY captures exist.
- Checks by hand cover the screen.
- Live mode is outside equivalence.

## 6. What does not prove parity
- The crate's earlier 158 tests (109 unit, 27 in `tests/bridge.rs`, 22 in `tests/equivalence.rs`) passed while all five cases were NOT COMPARED.
- The rewritten suites likewise pass without captures.
- A passing `cargo test` without captures proves nothing about equivalence.

## 7. Limits
- Parity covers this workflow, the five cases and `worldvista/vehu:202609` only.
- Unimplemented branches are outside equivalence and end the Rust session after the last prompt written.
- O-01 (HOME device `IOF`/`IOSL`) is open, so GC-01, GC-02 and GC-05 FAIL once captured, until it is answered.
- Live mode is unverified (O-07); replay is the delivered MUMPS backend.

## 8. How to run
From `rust/ifcap-po-inquiry/`, exactly as in `RUNNING.md` step 2: `cargo build --offline --locked`, then `cargo test --offline --locked`, then `cargo run --offline --locked --bin bridge`. Details: `RUNNING.md`; captures: `tests/golden/ifcap/README.md`.
