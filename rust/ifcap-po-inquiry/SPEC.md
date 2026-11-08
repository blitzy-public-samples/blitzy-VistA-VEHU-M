# SPEC — IFCAP purchase-order display (PRCH DISPLAY PO) Rust port

This document is the source-contract gate for porting OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`) from MUMPS on YottaDB to the standard-library-only Rust 1.99.1 crate `rust/ifcap-po-inquiry/`. The port is read-only: it reproduces the original's prompts, messages, field order, formatting and return behaviour, and writes no data. Every finding below is cited to a line read at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`; where source cannot settle a fact, the fact is an open item (O-NN) and its branch is OPEN or BLOCKED. `src/inquiry.rs`, `src/bridge.rs`, the tests and `REPORT.md` follow this document, which supersedes the plan wherever it records a field with evidence. Captures are owned by Rainman Kemp, due November 13, 2026, 5:00 p.m. Eastern.

**Citation key.** Routine file names are unique in the repository: `PRC*.m` are `Packages/IFCAP/Routines/`, `DI*.m` are `Packages/VA FileMan/Routines/`, and `Z*.m` and `XUP.m` are `Packages/Kernel/Routines/`. `DD.zwr`, `1+FILE.zwr`, `0.11+INDEX.zwr` and `0.31+KEY.zwr` are `Packages/VA FileMan/Globals/`; `19+OPTION.zwr`, `4+INSTITUTION.zwr`, `200+NEW PERSON.zwr`, `3.2+TERMINAL TYPE.zwr` and `3.5+DEVICE.zwr` are `Packages/Kernel/Globals/`; `411+…`, `442+…`, `442.5+…` and `442.8+…` exports are `Packages/IFCAP/Globals/` (the 411 export is `411+ADMIN. ACTIVITY SITE PARAMETER.zwr`).

**Status vocabulary.** Contract and branch evidence is SOURCE-RESOLVED, RUNTIME-VERIFIED, BLOCKED, or OPEN (waiting on a named open item; never passes the implementation gate). Nothing is RUNTIME-VERIFIED in this run: no capture exists. Byte notation: `\n` is a newline and `<BEL>` is `$C(7)` (byte 0x07); spaces are literal inside backticks.

## Source verification

| Field | Finding and evidence | Status |
|---|---|---|
| Baseline | Repository `github.com/blitzy-public-samples/blitzy-VistA-VEHU-M`, commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae` ("VistA patches for September 2026"); clean tree at planning; `Packages/` unchanged since (section "Source-unchanged check"). Runtime image digest: NOT AVAILABLE TO THE RUN | Commit SOURCE-RESOLVED; digest OPEN (O-05) |
| Menu option | `PRCHQ1` is a real OPTION entry, IEN 10295, type R, "New RFQ" [19+OPTION.zwr:81374-81381], not the purchase-order inquiry. Selected: `PRCH DISPLAY PO`, IEN 1901, type R [19+OPTION.zwr:14687] | SOURCE-RESOLVED |
| Entry point | `EN3^PRCHRPT`. Option zero node verbatim `^DIC(19,1901,0)="PRCH DISPLAY PO^Purchase Order Display^^R^11740^^^^^^^"` [19+OPTION.zwr:14687]; action verbatim `^DIC(19,1901,25)="EN3^PRCHRPT"`; entry action `^DIC(19,1901,20)=""` (empty) [19+OPTION.zwr:14690-14691] | SOURCE-RESOLVED |
| Source location | `Packages/IFCAP/Routines/PRCHRPT.m` lines 70-75 [PRCHRPT.m:70-75] | SOURCE-RESOLVED |
| Dependencies | PO-02 to PO-41 (section "Contracts"), including FileMan `DICM0` and `DICQ1` and `^%DT` called at [PRCFSITE.m:15]. The only unresolved dynamic target is `@IOF` [PRCHDP1.m:12] | IFCAP SOURCE-RESOLVED; O-01, O-03, O-04 open |
| Data dependencies | The 29 files and subfiles of section "FileMan definitions read", plus the two pointer-target files of its source-correction supplement, 445 `^PRCP(445,` and 440.5 `^PRC(440.5,`, read only by the PO-09 pointer-candidate rule (R6). Written or killed by the original, none of it business data: `^DISV` [DIC2.m:65]; `^UTILITY($J,"W")` and `^TMP($J,"W")` [PRCHDP1.m:53-54] [PRCHDP2.m:95]; `K ^TMP($J,"PRCHDP6")` [PRCHDP2.m:25]; `K ^TMP($J,"MDEL")` [PRCHDP4.m:8, 13]. `^XUTL("XQ",$J)` is read by `^%ZIS` for the HOME device [ZIS.m:28]; its `IO*` nodes are written by `SAVEVAR^%ZIS` at sign-on [ZIS.m:82-86], not on this path | SOURCE-RESOLVED |
| Capture mapping | GC-01, GC-02, GC-05: PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b8), PO-10 to PO-40, then PO-09 (b3) at the closing `^`. GC-03, GC-04: PO-01 to PO-05, PO-41, PO-06 to PO-08, PO-09 (b14), then PO-09 (b3). In this run the Rust side stops at the first BLOCKED branch each case reaches: PO-12.b1 for GC-01, GC-02 and GC-05, PO-09.b14 for GC-03 and GC-04 | Predicted; OPEN until runtime |

**Closure criteria still unmet.** The workflow has not executed in the matching VEHU environment; the HOME device context (O-01) is unresolved; the image digest (O-05) is unrecorded. The Source verification finding stays OPEN.

## Workflow selection

**What `PRCHQ1` is.** OPTION IEN 10295 `PRCHQ1^New RFQ`, type R [19+OPTION.zwr:81374]. Its description imports a 2237 into a Request for Quotation [19+OPTION.zwr:81376-81378]; its entry action is `S:'$$ESIG^PRCHQ1(DUZ) XQUIT=""` [19+OPTION.zwr:81379]; its routine is `EN2^PRCHQ2` [19+OPTION.zwr:81380], the "Entry point for generating a new RFQ" [PRCHQ2.m:14]. `PRCHQ1.m` holds file 444 input and output transforms and `ESIG`, every line under a label, with no top-level executable code [PRCHQ1.m:1-97]. **Decision: `PRCHQ1` is substituted** because it is a write path; the substitution is listed in `REPORT.md`.

| Candidate option (IEN) | Action | Decision and reason |
|---|---|---|
| `PRCH DISPLAY PO` (1901) | `EN3^PRCHRPT`, empty entry action [19+OPTION.zwr:14687-14691] | **Selected.** "Display Purchase Order information on the current device" [19+OPTION.zwr:14689]. On the Purchase Orders Menu `PRCH PURCHASE` (IEN 1883) [19+OPTION.zwr:14455, 14462] and four other menus [19+OPTION.zwr:14087, 30656, 30785, 80700]. It selects one order and displays it without writing business data |
| `PRCH DSPL PO` (1803) | `EN3^PRCHRPT` [19+OPTION.zwr:13967-13970] | Rejected: same routine, labelled "Display Purchase Order/Requisition", only on menu 4166 [19+OPTION.zwr:30330] |
| `PRCH DISPLAY DELIVERY ORDER` (10270) | Entry action `S PRCHDELV=1`, then `EN3^PRCHRPT` [19+OPTION.zwr:81148-81153] | Rejected: `PRCHDELV` restricts the lookup screen to delivery orders [PRCHRPT.m:11] |
| `PRCSPOS` (1851) | `POS^PRCSP1B` [19+OPTION.zwr:14214-14218] | Rejected: a Control Point status display with a print-or-display question [19+OPTION.zwr:14216-14217] |
| `PRCSOTR` (1785), `PRCH INQUIRE PURCHASE CARD` (10248), `PRCFA FMS PO (MO/SO) INQUIRY` (9524) | `OTR^PRCSP1`; purchase-card and FMS inquiries [19+OPTION.zwr:13865-13869, 80998, 73452] | Rejected: reports across many orders or another file |
| `PRCHQ1` (10295) | `EN2^PRCHQ2` | Rejected (above) |

**Entry point and prompt.** The order prompt text `P.O./REQ.NO.: ` is set in `PO^PRCHRPT` [PRCHRPT.m:12], passed as `DIC("A")` [PRCHPAT.m:81] and written by FileMan [DIC11.m:15-17]. The outermost tag the option enters on the way to that prompt is `EN3^PRCHRPT` [PRCHRPT.m:70], so it is the root contract PO-01 and maps to `inquire`.

**Dynamic targets.**
- `D @PRCHQ` [PRCHRPT.m:43] is reached only from `EN0` [PRCHRPT.m:20], which `EN3` does not call.
- `G @DIPGM` [DIC.m:21, 38] needs `^DD(442,0,"DIC")` or `^DD(411,0,"DIC")` [DIC2.m:44-47]; neither exists in the file headers [DD.zwr:514199-514238] [DD.zwr:488720-488768].
- `G @^("LOOK")` [DICM.m:10] needs a `LOOK` node; files 442 and 411 have none (the only `LOOK` nodes in `DD.zwr` are the soundex nodes of 120.82, 787.5 and 19620.12 [DD.zwr:256062, 657203, 812429]).
- XECUTEd screen and identifier code resolves to literals (section "Flagged hard-MUMPS features").
- `@IOF` [PRCHDP1.m:12] [PRCHDP2.m:42] is the only unresolved target (O-01).

## Original structure

This section restates source and adds no decision; it agrees with section "Contracts".

### Call tree

Call order from the entry point to `ASK1^PRCHDP2` (PO-38), the last tag that writes order output [PRCHDP2.m:41]. `Q^PRCHDP2` (PO-39) and `K^PRCHRPT` (PO-40) only kill variables; the `W @IOF` at line 94 is commented out [PRCHDP2.m:94-95]. "Reached at" is the line that calls the tag, or that the tag follows by fall-through or `G`.

```text
TAG^ROUTINE           ID     Role                                             Reached at
EN3^PRCHRPT           PO-01  option entry; maps to inquire                    [19+OPTION.zwr:14691]
  ST^PRCHRPT          PO-02  sets PRCF("X")="SP"                              [PRCHRPT.m:71]
    ^PRCFSITE         PO-03  site context, station prompt (stub)              [PRCHRPT.m:6]
      DUZ^PRCFSITE    PO-04  user context (stub)                              [PRCFSITE.m:5]
      ^%DT            PO-41  today's date for the fiscal year, no output      [PRCFSITE.m:15]
      ^DIC (411)      PO-05  station lookup (stub)                            [PRCFSITE.m:18]
  EN30^PRCHRPT        PO-06  selection loop (falls through; G EN30)           [PRCHRPT.m:71, 75]
    PO^PRCHRPT        PO-07  prompt text, screen                              [PRCHRPT.m:72]
      EN3^PRCHPAT     PO-08  lookup set-up on index C                         [PRCHRPT.m:13]
        IX^DIC        PO-09  order-selection lookup (stub)                    [PRCHPAT.m:83]
    ^PRCHDP1          PO-10  page-1 header                                    [PRCHRPT.m:74]
      ^%ZIS           PO-12  HOME device (stub, BLOCKED)                      [PRCHDP1.m:12]
      FTYP^PRCHFPNT   PO-11  facility type                                    [PRCHDP1.m:25]
      FOB^PRCHDP1     PO-13  FOB, proposal, authority, cost center (falls)    [PRCHDP1.m:52]
        DIWP^PRCUTL   PO-14  word-wrap wrapper, saves DA                      [PRCHDP1.m:53]
          ^DIWP       PO-15  word-wrap at width 16 (repro)                    [PRCUTL.m:34]
        %XY^%RCR      PO-16  copy ^UTILITY($J,"W") to ^TMP($J,"W") (repro)    [PRCHDP1.m:54]
        TY^PRCHDP1    PO-17  order type from the source code                  [PRCHDP1.m:57]
      DIS^PRCHDP1     PO-18  delivery date, buyer or agent, terms (falls)     [PRCHDP1.m:57]
        DT^PRCHDP1    PO-19  date as M/D/YYYY                                 [PRCHDP1.m:58]
      CON^PRCHDP1     PO-20  three FCP rows, dashes (falls)                   [PRCHDP1.m:60]
        DAT^PRCHDP1   PO-22  order date, row 1; calls DT (PO-19)              [PRCHDP1.m:61]
        APP^PRCHDP1   PO-21  appropriation, row 2                             [PRCHDP1.m:61]
        EST^PRCHDP1   PO-23  estimated flag, row 2                            [PRCHDP1.m:61]
        TOT^PRCHDP1   PO-24  order total, row 3                               [PRCHDP1.m:61]
      ^PRCHDP2        PO-25  halt prompt, item loop (G)                       [PRCHDP1.m:64]
        HDR^PRCHDP2   PO-26  form feed, column header (BLOCKED)               [PRCHDP2.m:10]
        ITEM^PRCHDP2  PO-27  one line item, once per item                     [PRCHDP2.m:11]
          PIPECK^PRCHDP2   PO-28  pipe-IN check                               [PRCHDP2.m:46]
          DIWP^PRCUTL      PO-14  description wrap at width 33                [PRCHDP2.m:47]
            ^DIWP          PO-15  word-wrap (repro)                           [PRCUTL.m:34]
          %XY^%RCR         PO-16  copy (repro)                                [PRCHDP2.m:48]
          EDISTAT^PRCHUTL  PO-29  EDI status check                            [PRCHDP2.m:58]
          ASK^PRCHDP2      PO-30  paging check (BLOCKED)                      [PRCHDP2.m:60]
        COM^PRCHDP2   PO-31  comments check (G)                               [PRCHDP2.m:14]
        PT^PRCHDP2    PO-32  transaction numbers, amendments; kills scratch   [PRCHDP2.m:19]
          AMENDS^PRCHDP6   PO-33  amendment history check                     [PRCHDP2.m:23]
        ASK2^PRCHDP2  PO-34  delivery schedule, receiving reports (falls)     [PRCHDP2.m:25]
          EN^PRCHDP4       PO-35  online flag                                 [PRCHDP2.m:26]
          EN2^PRCHDP4      PO-36  delivery-schedule lookup (falls)            [PRCHDP4.m:5]
          EXDELV^PRCHDP4   PO-37  newline; kills ^TMP($J,"MDEL") (G)          [PRCHDP4.m:10]
        ASK1^PRCHDP2  PO-38  END OF DISPLAY prompt; last output (G)           [PRCHDP2.m:26]
        Q^PRCHDP2     PO-39  kills locals and scratch; no output (G)          [PRCHDP2.m:41]
    K^PRCHRPT         PO-40  cleanup, then G EN30                             [PRCHRPT.m:74]
```

### Data reads

| Contracts | File, global root | Nodes and pieces read | Access method |
|---|---|---|---|
| PO-03, PO-04, PO-41 | 411 `^PRC(411,0)`, `^PRC(411,"AC","Y")`, `^PRC(411,site,0)`; 200 `^VA(200,DUZ,20)`, `.13`, `400` | Header piece 4; primary station; site parameters; signature name. PO-41 reads nothing | Direct (`$D`, `$O`, `$G`, `$P`) [PRCFSITE.m:8-14, 26, 35-39]; stubbed, nothing read from `Store` |
| PO-05 | 411 through FileMan; `^DIC(4)` through the identifier | Entry 500 | Generic FileMan (`DIC`) [PRCFSITE.m:18]; stubbed |
| PO-07, PO-08 | 442 `^PRC(442,PO,0)` piece 2, `^PRC(442,DA,7)`; 442.3 `^PRCD(442.3,s,0)` piece 2 | MOP; supply status order | Direct [PRCHRPT.m:14] [PRCHPAT.m:83] |
| PO-09 | 442 `^PRC(442,0)` (file header), `^PRC(442,"C",X,ien)`, each later `^PRC(442)` subscript, `^PRC(442,ien,0)`, `(1)`, `(7)`; 442.5; 442.3; for pointer transforms, the pointed-to file's `B` index and the parent index under each candidate | Candidates, screen, identifiers | Generic FileMan (`DIC`, `DIC3`, `DIC4`, `DICM`, `DICM0`, `DICQ1`); stubbed over `Store` |
| PO-10 | 442 nodes 0, 1, 7, 11, 12, 23, 24; 411 nodes 0, 1, 3; 440 nodes 0, 2, 3; 440.2 and `^DPT` only when field 5.3 is set; 5; 49; 442.5; 442.3 | Header, ship-to, vendor, status | Direct [PRCHDP1.m:4-44] |
| PO-11 | 411 node 0 piece 7; 442 node 23 piece 7; 411.2 | Facility type | Direct [PRCHFPNT.m:44-51] |
| PO-13, PO-17 | 442 node 0 piece 5; node 1 pieces 6, 7, 8, 14; node 14 with 442.4; 420.8; node 23 piece 11 | FOB, proposal, authority, cost center, type, buyer or agent | Direct [PRCHDP1.m:53-57, 71] |
| PO-18 to PO-24 | 442 node 0 pieces 3, 4, 10, 15; node 1 pieces 10, 15; node 5; item index `2,"AC"`; node 7 piece 3; 200 `^VA(200,agent,0)` | Dates, agent, discounts, FCP block, total | Direct [PRCHDP1.m:58-68] |
| PO-25 to PO-30 | 442 node `6,0`, order-level node 3 (discount entries, presence tested by `$O(^PRC(442,D0,3,n))` [PRCHDP2.m:12]; an entry is PO-25.b5, n/e), node 0 piece 13, node 15; items `2,n,0`, `2,n,1,k,0`, `2,n,2`, `2,n,3,"AC"`, `2,n,4`; node 22 `"B"`; node 23 pieces 8 and 11; 420.5 | Line items | Direct [PRCHDP2.m:9-66] [PRCHUTL.m:73] |
| PO-31 to PO-37 | 442 nodes 4, 13, 6, 11; node 0 piece 1 (the PO number; unguarded read, a `CorruptStore` site) [PRCHDP4.m:8]; 442.8 `^PRC(442.8,"B",PONO)` | Comments, transactions, amendments, receipts, PO number, delivery schedule | Direct [PRCHDP2.m:19-26] [PRCHDP6.m:127] [PRCHDP4.m:8-9] |
| PO-15, PO-16, PO-32, PO-36, PO-37, PO-39 | `^UTILITY($J,"W")`, `^TMP($J,"W")`, `^TMP($J,"PRCHDP6")`, `^TMP($J,"MDEL")` | Word-wrap scratch; kills of amendment and delivery scratch | Scratch state in `Locals`, never `Store` [PRCHDP2.m:25, 95] [PRCHDP4.m:8, 13] |

### Literals the five cases can print

Every literal on an exercised branch or on a branch the fixture profile permits. Stored data values are not listed; literals written only by n/e branches are left out.

| Literal text | `TAG^ROUTINE` | Line |
|---|---|---|
| `\n` | `DIVFY^PRCFSITE` | [PRCFSITE.m:8] |
| `\n`, `\n`, `Select STATION NUMBER ('^' TO EXIT): `, `500// ` | `SI^PRCFSITE`, `ASK^DIC`, `PR1^DIC11` | [PRCFSITE.m:18] [DIC.m:25] [DIC11.m:15-17] |
| Station echo `      CAMP MASTER` (six spaces; no newline) | `WO^DIC2`, `^DD(411,0,"ID",.1)` | [DIC2.m:10, 32] [DIC1.m:55-56] [DD.zwr:488724] [4+INSTITUTION.zwr:494] |
| `\n\n`, `\n`, `P.O./REQ.NO.: ` | `EN3^PRCHPAT`, `ASK^DIC`, `PR1^DIC11` (text from `PO^PRCHRPT`) | [PRCHPAT.m:83] [DIC.m:25] [DIC11.m:17] [PRCHRPT.m:12] |
| `<BEL> ??`, then `\n`, then `\n` and the re-asked prompt | `DING^DIC`, `F^DIC`, `ASK^DIC` | [DIC.m:63-65, 80-81, 25] |
| Order echo: `  ` and the full .01 value; then, through `WOV^DICQ1`, `  ` before each identifier: `Z.01` (`MM-DD-YY`), `Z.02` (MOP abbreviation, then `   ` and the status name), `Z.03` (`\n`, column 13, `FCP: `, `     $ `, total) | `WO^DIC2`, `WOV^DICQ1`, `^DD(442,0,"ID")` | [DIC2.m:10, 25, 32] [DICQ1.m:86-88] [DD.zwr:514202-514204] |
| `<BEL>`, ` >>> Status makes this record non-specifiable here.` | `EN30^PRCHRPT` | [PRCHRPT.m:73] |
| `PC ORDER: `, `DELIVER ORDER: `, `REQUISITION: `, `PURCHASE ORDER: `; `STATUS: `; `M.O.P.: `; `LAST PARTIAL RECD.: `; `REQUESTING SERVICE: `; `VENDOR:`; `SHIP TO: ` | `^PRCHDP1` | [PRCHDP1.m:15-21] |
| `, ` and two spaces in address rows; `ACCT # `; `FMS Vendor Code: `; ` VERBAL PURCHASE ORDER`, ` CONFIRMATION COPY`; `DELIVERY HOURS:`; `DELIVERY LOCATION: `; 80 × `_` | `^PRCHDP1` | [PRCHDP1.m:27, 29-31, 37-38, 43-44] |
| `FOB POINT: `, `ORIGIN`, `DESTINATION`, `\|`, `PROPOSAL: `, `\|AUTHORITY: `, `COST CENTER: `, `\|  `, `,` between authorities, `TYPE: `, `\|BUYER:`, `\|AGENT:` | `FOB^PRCHDP1` | [PRCHDP1.m:53-57] |
| `PURCHASE ORDER`, `DELIVERY & PURCHASE ORDER`, `DELIVERY ORDER` | `TY^PRCHDP1` | [PRCHDP1.m:71] |
| `DELIVER ON/BEFORE `, `\|`, `CONTRACT: `, `\|  ` and a space between the agent's name parts, `DISCOUNT TERM: ` | `DIS^PRCHDP1` | [PRCHDP1.m:58-60] |
| `/` between date parts | `DT^PRCHDP1` | [PRCHDP1.m:69] |
| `\|` columns, `APP: ` with `-`, `DATE: `, `TOTAL: `, 80 × `-` | `CON`, `APP`, `DAT`, `TOT^PRCHDP1` | [PRCHDP1.m:61-67] |
| `\n`, column 8, `ENTER '^' TO HALT: ` | `^PRCHDP2` | [PRCHDP2.m:10] |
| `EST. SHIPPING AND/OR HANDLING` | `^PRCHDP2` | [PRCHDP2.m:13] |
| `UNIT`, `TOTAL`, `ITEM`, `DESCRIPTION`, `QTY`, `UNIT`, `COST`, `COST`, 80 × `-` | `HDR^PRCHDP2` | [PRCHDP2.m:42] |
| `STK#: `, `NSN:  `, `FOOD GROUP: `, `QTY PREV RCVD: `, `Items per ` and `: `, `BOC: `, `FMS LINE: `, `CONTRACT: ` | `ITEM^PRCHDP2` | [PRCHDP2.m:56-64] |
| `\n` | `EXDELV^PRCHDP4` | [PRCHDP4.m:13] |
| `\n<BEL>`, `END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: ` | `ASK1^PRCHDP2` | [PRCHDP2.m:41] |

### Validation check order at the order prompt

1. On reading, `PR1^DIC11` [DIC11.m:18-31]: timeout [18]; a character outside `.ANP` [19]; a number with more than 25 integer or 24 fraction digits [21]; `^` [23]; more than 250 characters [24]; `?` [26]; a backtick value or a single space [27]; empty [29-31]; a value beginning with `^` sets `DUOUT` [33].
2. `RTN^DIC` [DIC.m:42-49]: `?` [42]; no value or `DUOUT` [45]; backtick [47]; an all-digit value calls `BYIEN2^DIC5` [48], which returns at once because `IX^DIC` defines `DF` [DIC.m:84-85] [DIC5.m:22]; spacebar [49].
3. The search, `SEARCH^DIC3` [DIC3.m:8-79]: a value longer than the index length (30) skips straight to step 4 [DIC3.m:13]; otherwise exact matches on `C` [DIC3.m:15-20, 82-107], then partial matches on `C` [DIC3.m:34-50], each under the screen [DIC3.m:94, 124]; then every later `^PRC(442)` subscript whose `$D` is 10 [DIC3.m:59-69] [DIC4.m:76-90]; then selection [DIC3.m:71-72].
4. With no candidate, `^DICM`: the transform loop over `^DD(442,0,"IX")` from `C` onward [DICM.m:13-18, 20-40] [DICF2.m:118-127], then the uppercase, comma and long-input retries [DICM.m:42-66], then `BAD^DIC1` [DICM.m:71, 85-87] [DIC1.m:157-159].
5. The failure echo and re-ask [DIC.m:63-65, 80-81, 25].
6. After a selection, the supply-status check [PRCHRPT.m:73].

### Display field order and formatting

- **Page 1** [PRCHDP1.m:15-63]: order label and number, `STATUS: ` at column 37; `M.O.P.: ` and `LAST PARTIAL RECD.: ` at 37; `REQUESTING SERVICE: ` at 37; `VENDOR:` with the name at 9 and `SHIP TO: ` at 48; seven address rows, vendor lines at column 9 and ship-to lines at 57 [PRCHDP1.m:41]; the `V(8)`/`S(8)` row [PRCHDP1.m:42]; `DELIVERY LOCATION: ` at 38 [PRCHDP1.m:43]; 80 underscores; the FOB, cost-center, type and delivery rows with `|` at columns 29 and 57 and proposal lines at 40 [PRCHDP1.m:53-59]; discount terms [PRCHDP1.m:60]; three FCP rows with `|` at 29 and 57 [PRCHDP1.m:61]; 80 dashes [PRCHDP1.m:63].
- **Each item** [PRCHDP2.m:49-66]: `$J(+$P(PRCHDI0,U,1),3)` at column 2, from piece 1 of the item's zero node (442.01 field .01 LINE ITEM NUMBER [DD.zwr:515986]), not the item's IEN subscript [PRCHDP2.m:49]; first description line at 7 [PRCHDP2.m:50]; `$J(qty,5)` at 40; unit at 47 [PRCHDP2.m:51]; unit cost at 52 [PRCHDP2.m:52]; `$J(total,7,2)` at 67 [PRCHDP2.m:53]; then the detail lines [PRCHDP2.m:56-64].
- **Dates:** `DT` prints `M/D/YYYY` without zero padding: `Y\100#100`, `Y#100\1`, `Y\10000+1700`, evaluated left to right [PRCHDP1.m:69]; `LAST PARTIAL RECD.` prints `MM/DD/YY` [PRCHDP1.m:17]; identifier `Z.01` prints `MM-DD-YY` [DD.zwr:514202].

### Amounts

The display does no arithmetic on amounts; it formats stored values with `$JUSTIFY`, reproduced on exact decimal strings with no floating point.
- Order total, field 91 at `0;15`, type `RNJ10,2` [DD.zwr:515190]: `$J(x,8,2)` [PRCHDP1.m:67].
- Item total cost, field 15 at `2;1`, type `RNJ9,2` [DD.zwr:516287]: `$J(x,7,2)` [PRCHDP2.m:53].
- Unit cost, field 5 at `0;9`, type `RNJ12,4XO` [DD.zwr:516145], by the count of stored fraction digits `$L($P(x,".",2))` [PRCHDP2.m:52]: more than 3 gives `$J(x,5,4)`; exactly 3 gives `$J(x,6,3)`; the value `N/C` prints `    N/C`; otherwise `$J(x,7,2)`.
- Estimated shipping, field 13 at `0;13` [DD.zwr:514683]: `$J(x,7,2)` at column 58 [PRCHDP2.m:13].
- Lookup echo: `$J(total,0,2)` [DD.zwr:514204].
- Rounding: to `d` decimals, half away from zero by adding 5 at the first discarded digit and truncating; a leading `0` for magnitudes below 1; left-padded with spaces to width `n`, never truncated. Negative amounts are rejected by the input transforms [DD.zwr:516287] [DD.zwr:515190] and are not exercised. FileMan stores canonical numbers, so `125.50` is held as `125.5`.
- Sanity examples only (the captures decide): GC-01 2 × $125.50 gives item total ` 251.00` and order total `  251.00`; GC-02 gives item totals `  59.85`, ` 251.00`, `   0.99`, unit costs `  19.95`, ` 125.50`, `   0.99`, and order total `  311.84`.

### How the entry resolves and how the display reaches its data

- **Entry.** `EN3` calls `ST`, which sets `PRCF("X")="SP"` and calls `^PRCFSITE` [PRCHRPT.m:70-71, 5-6]; `^PRCFSITE` calls `DUZ`, `^%DT` and the station `^DIC` [PRCFSITE.m:5, 15, 18]. `EN30` calls `PO`, which calls `EN3^PRCHPAT`; that writes `!!` and calls `IX^DIC` with `D="C"` and `DIC(0)="QEAMZ"` [PRCHRPT.m:72, 9-13] [PRCHPAT.m:79-83]. A selection runs `^PRCHDP1`, which jumps to `^PRCHDP2` [PRCHDP1.m:64], then `K`, then `G EN30` re-prompts [PRCHRPT.m:74-75].
- **Exit.** `^` at the order prompt ends the lookup with no output [DIC11.m:23] [DIC.m:45]; `EN3^PRCHPAT` quits on `+Y<0` [PRCHPAT.m:83]; `EN30` quits because `PRCHPO` is undefined [PRCHRPT.m:72]. `EN3` is reached by `DO`, not as an extrinsic, so `returned` is `None` for every case.
- **Data.** Vendor `^PRC(440,+$P(^PRC(442,D0,1),U,1),0)` and its pieces, with state abbreviations from `^DIC(5)` [PRCHDP1.m:18-28]; status `^PRCD(442.3,+^PRC(442,D0,7),0)` piece 1, first 34 characters [PRCHDP1.m:15]; items `$O(^PRC(442,D0,2,n))` with `^(n,0)`, `^(n,2)` and description lines `^(n,1,k,0)` [PRCHDP2.m:11, 47-53]. FileMan generic code runs only in the two lookups, the word-wrap (`^DIWP`, widths 16 and 33 [PRCHDP1.m:53] [PRCHDP2.m:44]), and `EN^DIQ` [PRCHDP1.m:9] and `Y^DIQ` [PRCHUTL.m:76-77], both on n/e branches.
- **Lookup search** (binding detail in section "Resolution decisions binding src/inquiry.rs"): read checks; `BYIEN2^DIC5` returns at once for `IX^DIC` [DIC5.m:22]; exact then partial matches on `C`, an entry counting only if its zero node exists and it passes the screen [DIC3.m:94, 110-126]; with `DIC(0)="QEAMZ"` an exact match is not selected at once because `E` is set and `O` is not [DIC3.m:28]; every later `^PRC(442)` subscript with `$D` equal to 10 is searched on the raw value [DIC3.m:59-69], except that an index on a pointer field is skipped when the value is numeric [DIC4.m:84-86]; one candidate after the whole search is selected [DIC3.m:71], several give the choose list [DIC3.m:72]. With none, `^DICM` loops over the indexes [DICM.m:13-18] [DICF2.m:101-127]; for a pointer field `P^DICM0` runs a nested lookup on the pointed-to file, starting at its `B` index [DICM0.m:40-41], screened so that a pointed-to entry counts only when a parent order is filed under it in the parent index, exists, and passes the parent screen [DICM0.m:25-37]. Then the retries [DICM.m:42-66] and exhaustion [DICM.m:71, 85-87] [DIC1.m:157-159] [DIC.m:63-65, 80-81].
- **The full number is not found.** `500-A12345` finds nothing: index `B` collates before `C` and the continuation never visits it [DIC3.m:63]; `.01` needs no transform; the value has no lowercase letter or comma and is not longer than 30 characters.
- **Nested misses write nothing of their own.** The nested lookup runs with `A` and `Q` stripped [DICM.m:107], so its `DING` and re-ask do not run [DIC.m:63, 80]; it returns `Y=-1` with the outer flags restored [DICM0.m:51-55] [DICM.m:123-133]. Whether a date transform inside it writes a date echo is O-03.

## FileMan definitions read

**Selection rule.** A file or subfile is in scope when a Ported, Stub or Repro contract reads one of its nodes; only the fields that contract reads are cited, never a whole dictionary. The rule gives 29 files and subfiles: the 21 in the table, whose fields are read, and 8 multiples of file 442 whose presence alone is tested on a predicted branch. It also gives the 2 pointer-target files of the source-correction supplement below, 445 and 440.5, which only the PO-09 stub's pointer-candidate rule reads (R6). "Definition" cites the file's range in `DD.zwr`; the numbers in parentheses are the `^DD(file,field,0)` lines, each read at the cited line.

| File (global root) | Definition | Fields read (DD line) | Read by |
|---|---|---|---|
| 442 (`^PRC(442,`) | [DD.zwr:514199-515950]; header nodes exported twice (the identifiers again at 514258-514260) | .01 PURCHASE ORDER NUMBER `0;1` (514312) with cross-references `B` (514315-514317) and `C`, type MUMPS, set `$P(X,"-",2)` (514318-514320); .02 MOP `0;2` (514331); .08 ESTIMATED ORDER? `7;3` (514379); .1 P.O. DATE `1;15` (514394); .5 SUPPLY STATUS `7;1` (514433); 1 FCP `0;3` (514505); 1.4 APPROPRIATION `0;4` (514551); 2 COST CENTER `0;5` (514557); 5 VENDOR `1;1` (514589); 5.2 REQUESTING SERVICE `1;2` (514606); 5.3 DIRECT DELIVERY PATIENT `1;12` (514610); 5.4 SHIP TO `1;3` (514615); 5.6 DELIVERY LOCATION `1;11` (514624); 6 VERBAL `1;4` (514632); 6.2 CONFIRMATION COPY `1;5` (514636); 6.4 F.O.B. POINT `1;6` (514639); 7 DELIVERY DATE `0;10` (514650); 8 SOURCE CODE `1;7` (514659); 8.1 `1;14` (514666); 8.2 PROPOSAL `1;8` (514672); 13 EST. SHIPPING `0;13` (514683); 16 BUYER `1;10` (514742); 31 SUBSTATION `23;7` (514881); 46 PURCHASE CARD NUMBER `23;8` (514943); 48 TYPE OF ORDER `23;11` (514968); 64 FREE TEXT VENDOR `24;2` (515068); 65 INTERFACE PACKAGE PREFIX `24;3` (515073); 91 TOTAL AMOUNT `0;15` (515190); 116 SEND EDI? `12;16` (515424); identifiers `Z.01` to `Z.03` (514202-514204); index list (514205-514231) | PO-06 to PO-11, PO-13, PO-17 to PO-25, PO-27 |
| 442.01 ITEM, node 2 (field 40, 514888) | [DD.zwr:515951-516592] | .01 LINE ITEM NUMBER `0;1` (515986); 1 DESCRIPTION, word-processing subfile 442.05 (516024); 2 QUANTITY `0;2` (516079); 3 UNIT OF PURCHASE `0;3` (516096); 3.1 PACKAGING MULTIPLE `0;12` (516100); 3.5 BOC `0;4` (516110); 4 CONTRACT/BOA # `2;2` (516132) with index `AC` (516134); 5 ACTUAL UNIT COST `0;9` (516145); 9 VENDOR STOCK NUMBER `0;6` (516187); 9.5 NSN `0;13` (516208); 11 QUANTITY PREVIOUSLY RECEIVED `2;8` (516239); 12 EDI STATUS CODE 1 `2;9` (516265); 13 EDI STATUS CODE 2 `2;11` (516276); 15 TOTAL COST `2;1` (516287); 41 FOOD GROUP `4;12` (516400) | PO-20, PO-25, PO-27 to PO-29 |
| 442.05 DESCRIPTION (word processing) | [DD.zwr:516735-516748] | .01 `0;1`, type W (516742) | PO-27, PO-28 |
| 442.12 PURCHASE METHOD, node 14 (field 8.3, 514677) | [DD.zwr:517616-517634] | .01 `0;1`, pointer to 442.4 (517623) | PO-13 |
| 442.041 BOC, node 22 (field 41, 514892) | [DD.zwr:516690-516734] | .01 SUBACCOUNT `0;1` (516699) and its `B` index; 2 FMS LINE `0;3` (516723) | PO-27 |
| 442.3 (`^PRCD(442.3,`) | [DD.zwr:517985-518070] | .01 NAME `0;1` (518024); .02 SUPPLY STATUS ORDER `0;2` (518035) | PO-06, PO-08, PO-09, PO-10 |
| 442.5 (`^PRCD(442.5,`) | [DD.zwr:518125-518204] | .01 NAME `0;1` (518146); 1 ABBREVIATION `0;2` (518161) | PO-09, PO-10 |
| 442.4 (`^PRC(442.4,`) | [DD.zwr:518071-518124] | 1 ABBREVIATION `0;2` (518097) | PO-13 |
| 442.8 (`^PRC(442.8,`) | [DD.zwr:518483-518630] | .01 ORDER NUMBER (PAT) `0;1` (518518), whose `B` index PO-36 reads | PO-36 |
| 440 (`^PRC(440,`) | [DD.zwr:509904-510732] | .01 NAME `0;1` (510047); 1 to 4 ORDERING ADDRESS1-4 `0;2`-`0;5` (510118, 510123, 510128, 510133); 4.2 CITY `0;6` (510138); 4.4 STATE `0;7`, pointer to 5 (510143); 4.6 ZIP `0;8` (510148); 5 PHONE `0;10` (510172); 5.1 ACCOUNT NO. `2;1` (510177); .2 EDI VENDOR? `3;2` (510094); 34 FMS VENDOR CODE `3;4` (510465); 35 ALT-ADDR-IND `3;5` (510481) | PO-10 |
| 440.2 (`^PRC(440.2,`) | [DD.zwr:510875-510965] | .01 `0;1`, pointer to 2 (510900); 1 STREET ADDR.1 to 6, node 0 pieces 2-7 (510909-510938), formatted into the ship-to lines with the state through file 5 [PRCHDP1.m:11, 36-37] | PO-10, only when 442 field 5.3 is set |
| 2 (`^DPT(`) | Field .01 only [DD.zwr:11081] | .01 NAME `0;1` (11081) | PO-10, same condition |
| 411 (`^PRC(411,`) | [DD.zwr:488720-489289] | .01 `0;1` (488769) with `B` index and the pre-lookup node 7.5 (488792); .1 INSTITUTION NAME `0;10` (488796), read by the identifier `^DD(411,0,"ID",.1)` (488724, exported again at 488748); .5 FACILITY TYPE `0;7` (488809); 21 PRIMARY STATION `0;2` and its `AC` index (488856-488860), read only as `^PRC(411,"AC","Y")` [PRCFSITE.m:14]; 31 PRINT FMS VENDOR ID `0;19` (488925); node 3, read whole and not displayed (its field 15 at 488823); the file's `ACT` node (488721) | PO-03 to PO-05, PO-10, PO-11 |
| 411.01 RECEIVING LOCATION, node 1 (field 1, 488814) | [DD.zwr:489290-489369] | .01 to 7, node 0 pieces 1-8 (489297-489333) | PO-10 |
| 411.2 (`^PRC(411.2,`) | [DD.zwr:489615-489653] | .01 `0;1` (489632); 1 LONG FACILITY NAME `0;2` (489640) | PO-11 |
| 420.5 (`^PRCD(420.5,`) | [DD.zwr:496069-496180] | .01 NAME `0;1` (496146) | PO-27 |
| 420.8 (`^PRCD(420.8,`) | [DD.zwr:496634-496712] | .01 CODE `0;1` (496661) | PO-17 |
| 5 (`^DIC(5,`) | [DD.zwr:43753-44267] | 1 ABBREVIATION `0;2` (44198) | PO-10 |
| 49 (`^DIC(49,`) | [DD.zwr:88042-88391] | .01 NAME `0;1` (88181) | PO-10 |
| 200 (`^VA(200,`) | [DD.zwr:344722-352033] | .01 NAME `0;1` (349371), read by PO-18 for the buyer or agent; 20.2 `20;2` (350238), 20.3 `20;3` (350250), .132 `.13;2` (349644) and 400 SUPPLY EMPLOYEE (351120), read by the `DUZ` stub | PO-04, PO-18 |
| 4 (`^DIC(4,`) | [DD.zwr:37266-38572] | .01 NAME `0;1`, type `R` (38111), through the station identifier | PO-05 |

**Presence-only multiples of 442** (their fields are read only on n/e branches): 442.03 DISCOUNT, node 3 [DD.zwr:514730]; 442.04 COMMENTS, node 4 [DD.zwr:514819]; 442.06 PROMPT PAYMENT TERMS, node 5 [DD.zwr:514680]; 442.07 AMENDMENT, node 6 [DD.zwr:514981]; 442.11 PARTIAL, node 11 [DD.zwr:514467]; 442.14 2237 REFERENCE NUMBER, node 13 [DD.zwr:514364]; 442.15 ADMINISTRATIVE CERTIFICATIONS, node 15 [DD.zwr:514843]; 442.08 DATE RECEIVED, item node 3 [DD.zwr:516311].

**Source-correction supplement: pointer targets read by the PO-09 stub.** The plan's 29 omit two files that the pointer-candidate rule (R6) reads, each the target of a pointer index of file 442 after `C`. The global root is the file's `^DIC(file,0,"GL")` node.

| File (global root) | Definition | Fields read (DD line) | Reached from file 442 (DD line) | Read by |
|---|---|---|---|---|
| 445 GENERIC INVENTORY (`^PRCP(445,`) [1+FILE.zwr:32559, 32562] | [DD.zwr:524949-525524] | .01 INVENTORY POINT `0;1`, type `RFX` (525062), with cross-reference `B`, set `^PRCP(445,"B",$E(X,1,30),DA)` (525064-525066) | Index `G` (514222) on 442.14 field 11 INVENTORY/DISTRIBUTION POINT `0;11`, type `P445'` (517723), which files `^PRC(442,"G",$E(X,1,30),DA(1),DA)`, the order before the subrecord (517725-517727); `^DD(445,0,"PT",442.14,11)` (524976) | PO-09 (R6) |
| 440.5 PURCHASE CARD INFORMATION (`^PRC(440.5,`) [1+FILE.zwr:31961, 31965] | [DD.zwr:511910-512303] | .01 PURCHASE CARD NUMBER `0;1`, type `RFXI` (511963), with cross-reference `B`, set `^PRC(440.5,"B",$E(X,1,30),DA)` (511965-511967) | Index `M` (514228) on 442 field 46 PURCHASE CARD NUMBER `23;8`, type `R*P440.5'XO` (514943); `^DD(440.5,0,"PT",442,46)` (511933) | PO-09 (R6) |

The stub reads each of the two only when `$O(^PRC(442,0))` and `$O(^PRC(442,index,0))`, with `index` `G` or `M`, are above 0 [DICM0.m:26, 32], and every read is guarded: the `B` index, `^PRCP(445,"B",v,e)` or `^PRC(440.5,"B",v,e)`, for `v` equal to `X` and then each later subscript beginning with `X`, walking `e` in collation order up to the first subscript whose numeric interpretation is zero [DIC3.m:15-50, 83]; `$D` of the entry's zero node and of its `-9` node, the entry counting only with a zero node and no `-9` node [DIC3.m:94, 110]; the zero node's value, which `$P(^(0),U)` reads [DIC3.m:110], so a zero node holding only descendants is `CorruptStore`; then each parent `p` under `^PRC(442,"G",e,p)` or `^PRC(442,"M",e,p)`, up to the first that is not above 0, and its zero node `^PRC(442,p,0)` for the R5 screen, `CorruptStore` likewise [DICM0.m:32, 36]. No repository fixture holds `^PRCP(445)` or `^PRC(440.5)`: over the repository fixtures alone `ix_dic` stops at PO-09.b15 (R5 step 1) before these reads, and over a `cases.zwr` without them they find nothing. The other four pointer targets are in the table above, and R6 reads their `B` indexes the same way: 440 (510050-510052), 442.5 (518148-518150), 200 (349374-349376) and 442.3 (518026-518028).

**Traditional cross-references only.** `0.11+INDEX.zwr` holds 1785 `^DD("IX","BB",…)` nodes and none for file 442 or 411, and no `^DD("IX",n,0)` entry whose file piece is 442 or 411, so both lookups take the traditional cross-reference path [DICUIX.m:18-19]. Neither file has a primary key: `0.31+KEY.zwr` holds no `^DD("KEY","AP",442` or `^DD("KEY","AP",411` node, so `ADDKEY` adds nothing to the echo [DIC3.m:130-139].

**Citation notes.** File 2's "definition" is its .01 line only, because no other field of file 2 is read. The 442.8 `B` index is cited through field .01 (518518); the 442.8 export holds no `B` node [442.8+DELIVERY SCHEDULE ORDER.zwr:3-5].

## Contracts

One contract per `TAG^ROUTINE` reached from `EN3^PRCHRPT`. Every location is at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae` (written `@dd3fd0f3` below). IDs are stable identifiers, not call order; section "Original structure" gives the call order. Kinds: **Ported** (logic converted), **Stub** (an out-of-slice dependency reproduced only for the fixed context and the exercised outcomes), **Repro** (a generic FileMan or Kernel utility reproduced only for the forms the cases exercise). Every Rust item except `inquire` is a private function in `src/inquiry.rs`, and every private signature is `fn name(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>` unless stated. Each **Rust item** gives the function's path and its declaration exactly as `src/inquiry.rs` writes it; a parameter whose name begins with `_` is one that function does not use, and it keeps the type and position this convention, or the contract's formal arguments, give it. No contract on this path has a return value: each is called by `DO`. Branch IDs are defined in section "Unexercised and BLOCKED branches"; every n/e or BLOCKED branch returns `Halt::Error(InquiryError::UnsupportedBranch("<branch ID>"))` and writes nothing further. No contract writes business data, and none writes `Store`.

**Test names.** T-SESSION: `input_exhaustion_returns_partial_transcript`, `leftover_input_ignored`, `store_unchanged_after_session`. T-LOOKUP: `help_at_order_prompt_is_unsupported_branch`, `absent_identifiers_follow_no_match_path`, `numeric_value_naming_no_ien_follows_no_match_path`, `lowercase_value_whose_retry_fails_follows_no_match_path`, `candidates_all_rejected_by_screen_follow_no_match_path`, `unreferenced_vendor_with_matching_name_is_not_a_candidate`, `referenced_vendor_whose_orders_fail_screen_is_not_a_candidate`, `referenced_vendor_with_qualifying_order_is_unsupported_branch`. T-DEVICE: `found_order_stops_at_device_gate`. T-EST: `estimated_flag_decides_est_branch`. GC-FOUND: `gc_01_standard_order`, `gc_02_multiple_lines_and_decimals`, `gc_05_cancelled_order`. GC-MISS: `gc_03_missing_order`, `gc_04_invalid_input`. All unit tests are in `src/inquiry.rs`; the GC tests are in `tests/equivalence.rs`.

**`CorruptStore` sites.** An unguarded read of a missing node on an exercised path, where the original raises an M `UNDEF` error, returns `CorruptStore(<global reference from Store::format_ref>)`. On this path there are three: the item zero node `^(PRCHDI,0)` [PRCHDP2.m:11], the authority entry `^(Y,0)` under node 14 [PRCHDP1.m:56], and `^PRC(442,D0,0)` in `EN2^PRCHDP4` [PRCHDP4.m:8]. All three lie after the BLOCKED PO-12.b1, so none is reachable in this run. Every other read on the path is guarded by `$G`, `$D` or `$S`.

### PO-01 EN3^PRCHRPT

- **ID and location:** PO-01; `Packages/IFCAP/Routines/PRCHRPT.m` @dd3fd0f3; lines 70-71; tag `EN3`, routine `PRCHRPT`.
  ```text
  EN3 ;DISPLAY P.O.
   D ST
  ```
- **Rust item:** `crate::inquiry::inquire`, `pub fn inquire(store: &Store, inputs: &[String]) -> Result<InquiryOutput, InquiryError>`; Ported.
- **Inputs:** `store`, borrowed immutably; `inputs`, one typed line each without its terminator, consumed in order by READs. Precondition, as at option entry: every M local the path uses is undefined (`PRC`, `PRCF`, `PRCHPO`, `DIC`, `DO`, `DST`); context is `STUB_CONTEXT` only.
- **Reads:** none itself.
- **Branches:** PO-01.b1 (`D ST`, then fall into `EN30`).
- **Outputs:** `InquiryOutput { transcript, returned: None }`; `returned` is `None` because `EN3` is reached by `DO` [19+OPTION.zwr:14691]. `Halt::InputExhausted` becomes `Ok` with the transcript written so far; `Halt::Error(e)` becomes `Err(e)` and discards the transcript. Lines left over after `EN30` quits are ignored.
- **Side effects:** none.
- **Dependencies and coverage:** calls `st_prchrpt` (PO-02), then `en30_prchrpt` (PO-06). All five cases (predicted). Tests: T-SESSION, T-LOOKUP, T-DEVICE, GC-FOUND, GC-MISS. SOURCE-RESOLVED.

### PO-02 ST^PRCHRPT

- **ID and location:** PO-02; `Packages/IFCAP/Routines/PRCHRPT.m` @dd3fd0f3; lines 5-7; tag `ST`.
  ```text
  ST S PRCF("X")="SP"
   D ^PRCFSITE
   Q
  ```
- **Rust item:** `crate::inquiry::st_prchrpt`, `fn st_prchrpt(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** none. **Reads:** none.
- **Branches:** PO-02.b1 (set `PRCF("X")="SP"`, call `^PRCFSITE`).
- **Outputs:** none itself. **Side effects:** `PRCF("X")="SP"`, killed again at `OUT^PRCFSITE` [PRCFSITE.m:29].
- **Dependencies and coverage:** called from PO-01 [PRCHRPT.m:71]; calls `prcfsite` (PO-03). All five. Tests: T-SESSION, GC-FOUND, GC-MISS. SOURCE-RESOLVED.

### PO-03 ^PRCFSITE

- **ID and location:** PO-03; `Packages/IFCAP/Routines/PRCFSITE.m` @dd3fd0f3; lines 4-29; unlabeled entry `DIVFY`.
  ```text
  DIVFY ;CHECK FOR STATION AND FY
   D DUZ G:'% Q
  ```
- **Rust item:** `crate::inquiry::prcfsite`, `fn prcfsite(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Stub.
- **Inputs:** `PRCF("X")="SP"` from PO-02; `DT`, `U` and `DTIME` are capture-command preconditions; `PRC` undefined.
- **Reads (original):** `^PRC(411,0)` piece 4 = 6 [PRCFSITE.m:8-9] [411 export:3]; `$O(^PRC(411,"AC","Y",0))` = 500 [PRCFSITE.m:14] [411 export:290]; `^VA(200,.5,400)` [PRCFSITE.m:12]; `^PRC(411,500,0)` [PRCFSITE.m:26] [411 export:4]. **The stub reads nothing from `Store`:** it fixes the snapshot's site context.
- **Branches:** PO-03.b1 to PO-03.b9. Exercised: b4 (no output) and b6 (RETURN accepts 500). b5 and b8 are unreachable because `PRCF("X")="SP"` has no `A`, `F` or `Q`; b1, b2, b3, b7 and b9 are n/e.
- **Outputs:** in order: `duz_prcfsite`; `\n` [PRCFSITE.m:8]; `pct_dt` (b4) [PRCFSITE.m:15]; `\n` [PRCFSITE.m:18]; then `dic` (PO-05). The `MDIV` flag holds because the site file has six entries [PRCFSITE.m:13], so the station prompt is written.
- **Side effects:** `PRC("SITE")=500` after the lookup [PRCFSITE.m:18]. `PRC("FY")` and `PRC("QTR")` are not modelled (section "Stubs"). `OUT` kills `%DT`, `DIC`, `PRCF("X")`, `A`, `B`, `X`, `Y` [PRCFSITE.m:29].
- **Dependencies and coverage:** called from PO-02 [PRCHRPT.m:6]; calls PO-04, PO-41, PO-05. All five. Tests: T-SESSION, GC-FOUND, GC-MISS. SOURCE-RESOLVED: the station bytes are resolved line by line in section "Resolution decisions binding src/inquiry.rs", item R1.

### PO-04 DUZ^PRCFSITE

- **ID and location:** PO-04; `Packages/IFCAP/Routines/PRCFSITE.m` @dd3fd0f3; lines 35-40; tag `DUZ`.
  ```text
  DUZ K PRCFDEL,PRC("PER") S %=1 I $D(DUZ)#2,+DUZ>0 S PRC("PER")=+DUZ
  ```
- **Rust item:** `crate::inquiry::duz_prcfsite`, `fn duz_prcfsite(_s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt>`; Stub.
- **Inputs:** `DUZ` = `.5` from `STUB_CONTEXT`.
- **Reads (original):** `^VA(200,.5,20)` [PRCFSITE.m:37] piece 2 (field 20.2 [DD.zwr:350238]) = `POSTMASTER` [200+NEW PERSON.zwr:12], present, so `D A` never writes [PRCFSITE.m:38]; piece 3 (field 20.3 [DD.zwr:350250]) empty; pieces 2 and 3 are copied into `PRC("PER")` [PRCFSITE.m:39]. The node's other pieces, including the confidential electronic signature code (field 20.4 [DD.zwr:350262, 350284]), decide nothing on this path and are not reproduced. `^VA(200,.5,.13)` [PRCFSITE.m:39]. A missing node 20 makes `X` empty through `$S('$D(^VA(200,+PRC("PER"),20)):"",1:^VA(200,+PRC("PER"),20))` [PRCFSITE.m:37], so piece 2 is empty and `D A` runs (PO-04.b3, n/e) [PRCFSITE.m:38]; a missing `.13` node makes piece 4 of `PRC("PER")` empty instead of `.13` piece 2 (field .132 [DD.zwr:349644]) through `$S($D(^VA(200,+PRC("PER"),.13)):$P(^(.13),"^",2),1:"")` [PRCFSITE.m:39]. The stub reads nothing from `Store`.
- **Branches:** PO-04.b1 (exercised), b2 and b3 (n/e).
- **Outputs:** none. **Side effects:** `%=1`; `PRC("PER")` is not modelled because no output on the path reads it (its only other reader, `^VA(200,+PRC("PER"),400)` [PRCFSITE.m:12], sets `PRC("SP")`, which only the unreachable `A` flag reads [PRCFSITE.m:16, 19]).
- **Dependencies and coverage:** called at [PRCFSITE.m:5]. All five. Tests: T-SESSION. SOURCE-RESOLVED.

### PO-05 ^DIC (file 411)

- **ID and location:** PO-05; call site `Packages/IFCAP/Routines/PRCFSITE.m` @dd3fd0f3 line 18; FileMan `DIC.m` lines 8-69, `DIC11.m` 15-32, `DIC5.m` 21-32, `DIC2.m` 8-66; tag `DIC`.
  ```text
  SI I $D(PRC("MDIV")),PRCF("X")["S"!(PRCF("X")["B") W ! S DIC("A")="Select STATION NUMBER ('^' TO EXIT): ",DIC("B")=PRC("SITE"),DIC=411,DIC(0)="AEQMZ",DIC("S")="I +Y<1000000" D ^DIC K DIC G:+Y<0 Q S PRC("SITE")=+Y
  ```
- **Rust item:** `crate::inquiry::dic`, `fn dic(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Stub.
- **Inputs:** `DIC=411`, `DIC(0)="AEQMZ"`, `DIC("A")`, `DIC("B")=500`, `DIC("S")="I +Y<1000000"` [PRCFSITE.m:18].
- **Reads (original):** `^PRC(411,0)` (file attributes `411I`) [411 export:3]; `^DD(411,0,"ID",.1)` [DD.zwr:488724]; `^PRC(411,"B",500,500)` [411 export:292]; `^PRC(411,500,0)` piece 10 [411 export:4]; `^DIC(4,500,0)` piece 1 [4+INSTITUTION.zwr:494]; `^DD(4,.01,0)` piece 2 [DD.zwr:38111]. The stub reads nothing from `Store`.
- **Branches:** PO-05.b1 (RETURN, exercised), PO-05.b2 (any other answer, n/e).
- **Outputs:** `\n` [DIC.m:25]; `Select STATION NUMBER ('^' TO EXIT): 500// ` [DIC11.m:15-17]; one READ (no input left: `Halt::InputExhausted`); an empty line writes `      CAMP MASTER` (six spaces, no newline) and selects 500; any non-empty line returns `UnsupportedBranch("PO-05.b2")`. Byte trace: section "Resolution decisions binding src/inquiry.rs", item R1.
- **Side effects (original, not reproduced):** `^DISV(DUZ,"^PRC(411,")` [DIC2.m:65]; `Y`, `Y(0)`, `Y(0,0)` [DIC2.m:64].
- **Dependencies and coverage:** called from PO-03 [PRCFSITE.m:18]. All five. Tests: T-SESSION, T-LOOKUP, GC-FOUND, GC-MISS. SOURCE-RESOLVED (O-02 station part closed by R1).

### PO-06 EN30^PRCHRPT

- **ID and location:** PO-06; `Packages/IFCAP/Routines/PRCHRPT.m` @dd3fd0f3; lines 72-75; tag `EN30`.
  ```text
  EN30 D PO Q:'$D(PRCHPO)
   I X<10 W $C(7)," >>> Status makes this record non-specifiable here."
   S D0=PRCHPO D ^PRCHDP1,K
  ```
- **Rust item:** `crate::inquiry::en30_prchrpt`, `fn en30_prchrpt(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported. The `G EN30` [PRCHRPT.m:75] is a loop.
- **Inputs:** `X` and `PRCHPO` from PO-08.
- **Reads:** none itself.
- **Branches:** PO-06.b1 (`PRCHPO` undefined: return), PO-06.b2 (selection: message when `X<10`, display, `K`, loop).
- **Outputs:** `<BEL> >>> Status makes this record non-specifiable here.` when `X<10`, compared numerically; `X` is the 442.3 supply status order or `""`, and `""` compares as 0, so an order with no node 7, no status entry or status order 0 also prints it [PRCHRPT.m:73] [PRCHPAT.m:83].
- **Side effects:** `D0=PRCHPO` [PRCHRPT.m:74].
- **Dependencies and coverage:** calls PO-07, PO-10, PO-40. b1 all five; b2 GC-01, GC-02, GC-05. Tests: T-SESSION, T-DEVICE, GC-FOUND, GC-MISS. SOURCE-RESOLVED.

### PO-07 PO^PRCHRPT

- **ID and location:** PO-07; `Packages/IFCAP/Routines/PRCHRPT.m` @dd3fd0f3; lines 9-15; tag `PO`.
  ```text
  PO S PRCHP("S")="$P(^(0),U,2)<10!($P(^(0),U,2)=25)"
   I $G(PRCHPC) S PRCHP("S")="$P(^(0),U,2)=25" ; <<< Patch 72
   I $G(PRCHDELV) S PRCHP("S")="$P($G(^(23)),U,11)=""D"""
  ```
- **Rust item:** `crate::inquiry::po_prchrpt`, `fn po_prchrpt(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHPC` and `PRCHDELV` undefined on this option (empty entry action [19+OPTION.zwr:14690]), so their SETs never fire.
- **Reads:** `^PRC(442,+PRCHPO,0)` piece 2 (field .02 [DD.zwr:514331]), guarded by `$D` [PRCHRPT.m:14].
- **Branches:** PO-07.b1.
- **Outputs:** none. **Side effects:** `PRCHP("S")`, `PRCHP("A")="P.O./REQ.NO.: "` [PRCHRPT.m:12]; `PRCHNRQ=1` when MOP is 8 [PRCHRPT.m:14].
- **Dependencies and coverage:** called from PO-06; calls PO-08 [PRCHRPT.m:13]. All five. Tests: T-LOOKUP, GC-FOUND, GC-MISS. SOURCE-RESOLVED.

### PO-08 EN3^PRCHPAT

- **ID and location:** PO-08; `Packages/IFCAP/Routines/PRCHPAT.m` @dd3fd0f3; lines 78-84; tag `EN3`.
  ```text
  EN3 ;LOOK UP PO IN FILE 442
   K PRCHPO,PRCHNEW,DA,DIC,D0,DQ Q:'$D(PRC("SITE"))  S DIC="^PRC(442,",DIC(0)="QEAMZ"
   S D=$S($G(PRCHPC)=1:"APCS",$G(PRCHPC)=2:"APCP",$G(PRCHDELV):"APCD",1:"C")
  ```
- **Rust item:** `crate::inquiry::en3_prchpat`, `fn en3_prchpat(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRC("SITE")=500`; `PRCHP("A")`, `PRCHP("S")` from PO-07.
- **Reads:** `^PRC(442,DA,7)` and `^PRCD(442.3,status,0)` piece 2 (field .02 [DD.zwr:518035]) through `$G` [PRCHPAT.m:83].
- **Branches:** PO-08.b1 (n/e), PO-08.b2 (no selection: quit), PO-08.b3 (selection).
- **Outputs:** `\n\n` before the lookup [PRCHPAT.m:83].
- **Side effects:** kills `PRCHPO`, `DA`, `DIC`, `D0` [PRCHPAT.m:79]; `D="C"`; `DIC("A")="P.O./REQ.NO.: "`; `DIC("S")="I +$P(^(0),U,1)=PRC(""SITE""),$P(^(0),U,2)<10!($P(^(0),U,2)=25)"` [PRCHPAT.m:81]; on selection `PRCHPO=DA=+Y` and `X` = the status order, with `X=0` replaced by `""` [PRCHPAT.m:83].
- **Dependencies and coverage:** called from PO-07 [PRCHRPT.m:13]; calls PO-09 [PRCHPAT.m:83]. b2 all five; b3 GC-01, GC-02, GC-05. Tests: T-LOOKUP, T-DEVICE, GC-FOUND, GC-MISS. SOURCE-RESOLVED.

### PO-09 IX^DIC (file 442, index C)

- **ID and location:** PO-09; `Packages/VA FileMan/Routines/DIC.m` @dd3fd0f3; lines 84-86, then the lookup routines each branch cites (`DIC.m`, `DIC0.m`, `DIC1.m`, `DIC11.m`, `DIC2.m`, `DIC3.m`, `DIC4.m`, `DIC5.m`, `DICM.m`, `DICM0.m`, `DICM1.m`, `DICF2.m`, `DICQ1.m`, `DICUIX.m`); tag `IX`.
  ```text
  IX N DINDEX,DF
   S (DF,DINDEX,DINDEX("START"))=D
   G EN
  ```
- **Rust item:** `crate::inquiry::ix_dic`, `fn ix_dic(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Stub over `Store` (lookup rules over fixture data, never a list of known identifiers).
- **Inputs:** `DIC="^PRC(442,"`, `DIC(0)="QEAMZ"`, `D="C"`, `DIC("A")`, `DIC("S")` from PO-08; `PRC("SITE")=500`.
- **Reads:** `^PRC(442,0)` piece 2 = `442I` [442 export:3] [DIC1.m:41-46]; `^DD(442,0,"ID","Z.01")`-`("Z.03")` [DD.zwr:514202-514204]; `^PRC(442,"C",X,ien)` and later `^PRC(442)` subscripts; `^PRC(442,ien,0)` pieces 1-3, 15 (screen and identifiers); `^PRC(442,ien,1)` piece 15; `^PRC(442,ien,7)`; `^PRCD(442.5,mop,0)` piece 2; `^PRCD(442.3,s,0)` piece 1; for the pointer rule (R6), `$O(^PRC(442,0))` and, for each pointer index D, F, G, M, MCH and ST in turn, `$O(^PRC(442,index,0))` [DICM0.m:26, 32], the pointed-to root's `B` index (`^PRC(440,`, `^PRCD(442.5,`, `^PRCP(445,`, `^PRC(440.5,`, `^VA(200,`, `^PRCD(442.3,` respectively), `$D` of each matching entry's zero and `-9` nodes and the zero node's value [DIC3.m:94, 110], then `^PRC(442,index,e,parent)` and the parent's zero node [DICM0.m:32, 36]; files 445 and 440.5 are cited in the source-correction supplement of section "FileMan definitions read". Every read is guarded.
- **Branches:** PO-09.b1 to PO-09.b16 (b15 and b16 appended by this document). Exercised: b3 (all five), b8 (GC-01, 02, 05), b14 (GC-03, 04, BLOCKED by O-03). b7 and b12 are unreachable.
- **Outputs:** `\n` [DIC.m:25]; `P.O./REQ.NO.: ` [DIC11.m:15-17]; one READ (no input left: `Halt::InputExhausted`); then the branch's output (b3: nothing; b8: the order echo of R2; b14: `UnsupportedBranch("PO-09.b14")` while BLOCKED, otherwise the bytes of R3).
- **Side effects (original, not reproduced):** `^DISV(DUZ,"^PRC(442,")` [DIC2.m:65]; `Y`, `Y(0)`, `Y(0,0)` [DIC2.m:53, 64].
- **Dependencies and coverage:** called from PO-08 [PRCHPAT.m:83]. Tests: T-LOOKUP (`help_at_order_prompt_is_unsupported_branch` gives PO-09.b4), GC-FOUND, GC-MISS; `tests/bridge.rs` `synthetic_422_unsupported_branch_end_to_end`. Status: b3 and b8 SOURCE-RESOLVED (R2); b14 BLOCKED (O-03, R3-R4). Binding rules: R2 to R6.

### PO-10 ^PRCHDP1

- **ID and location:** PO-10; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; lines 4-52; unlabeled entry.
  ```text
   S PRCHD0=$G(^PRC(442,D0,0)),PRCHD1=$G(^(1)),X=0
   N PRCHSIT1
   S PRCHDSIT=$P(PRCHD0,"-",1)
  ```
- **Rust item:** `crate::inquiry::prchdp1`, `fn prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported. The `G ^PRCHDP2` at the end of PO-20 [PRCHDP1.m:64] is a tail call.
- **Inputs:** `D0`, `PRCHPO`, `PRCHNRQ` (MOP 8, from PO-07).
- **Reads:** 442 node 0 (`PRCHD0`), node 1 (`PRCHD1`), node 7 piece 1 (status), node 11 (partials), node 12 pieces 14 and 16, node 23 pieces 7 and 11, node 24 pieces 2 and 3; 411 `^PRC(411,site,0)` (piece 19 [DD.zwr:488925]), node 3, and `^PRC(411,site,1,shipto,0)` pieces 1-8 [DD.zwr:489297-489333]; 440 node 0 pieces 1-8 and 10, node 2 piece 1, node 3 pieces 2, 4, 5 [DD.zwr:510047-510481]; 440.2 node 0 and `^DPT(patient,0)` piece 1 when field 5.3 is set [PRCHDP1.m:11]; 5 piece 2 [DD.zwr:44198]; 49 piece 1 [DD.zwr:88181]; 442.5 piece 1; 442.3 piece 1. All guarded by `$G` or `$D`.
- **Branches:** PO-10.b1 (node 0 empty: quit), b2 (node 1 empty, comments present: `EN^DIQ`), b3 (node 1 empty: quit), b4 (the page-1 header, BLOCKED by O-01 at line 12), b5 (node 11 partials), b6 (EDI block), b7 (prosthetics block).
- **Outputs, in order, on b4** (lines 13-44, reached only after PO-12):
  - `\n`, then `PC ORDER: ` when node 23 piece 11 is `P` or `S`, `DELIVER ORDER: ` when it is `D`, `REQUISITION: ` when `PRCHNRQ` is defined, else `PURCHASE ORDER: `; the .01 value; column 37 `STATUS: `; the 442.3 name truncated to 34 when node 7 is above 0 [PRCHDP1.m:13-15].
  - `\n`, `M.O.P.: `, the 442.5 name, column 37 `LAST PARTIAL RECD.: ` [PRCHDP1.m:16]; with no node 11 nothing follows (b5 n/e) [PRCHDP1.m:16-17].
  - `\n`, column 37 `REQUESTING SERVICE: `, the file 49 name [PRCHDP1.m:17].
  - `\n`, `VENDOR:`, column 9 the vendor name, or field 64 when the name is `SIMPLIFIED` and field 64 is set; column 48 `SHIP TO: ` and ship-to piece 1 [PRCHDP1.m:18-21].
  - `PRC("SITE")` is reset to the station part of the PO number [PRCHDP1.m:24]; `FTYP` (PO-11); the vendor rows `V(1)`-`V(7)` and ship-to rows `S(1)`-`S(7)` built by conditional SETs [PRCHDP1.m:25-40]; seven rows `\n`, column 9 `V(I)`, column 57 `S(I)` [PRCHDP1.m:41]; `\n`, `V(8)`, column 57 `S(8)`, `\n` [PRCHDP1.m:42]; column 38 `DELIVERY LOCATION: ` and field 5.6 then `\n` when set [PRCHDP1.m:43]; 80 underscores [PRCHDP1.m:44]. Then fall into `FOB` (PO-13).
- **Ship-to rule:** MOP not 4 and field 5.3 empty: `^PRC(411,PRCHSIT1,1,+shipto,0)`; otherwise, when field 5.3 is set, `^PRC(440.2,patient,0)` with piece 1 replaced by the patient name (first 21 characters) and `PRCHDS=1` [PRCHDP1.m:10-11]. `PRCHSIT1` is node 23 piece 7 when set, else the station part of the PO number [PRCHDP1.m:7].
- **Side effects:** locals `PRCHD0`, `PRCHD1`, `PRCHDSIT`, `PRCHDSHP`, `PRCHDS`, `PRCHDST`, `PRCHDHSP`, `PRCHTYPE`, `PRCHDV`, `V`, `S`, `X`, `I`, `PRC("SITE")`; `IOP="HOME"`, `%ZIS=""` [PRCHDP1.m:12].
- **Dependencies and coverage:** called from PO-06 [PRCHRPT.m:74]; calls PO-12, PO-11, falls into PO-13. GC-01, GC-02, GC-05. Tests: T-DEVICE, GC-FOUND. b1-b3, b5-b7 SOURCE-RESOLVED n/e; b4 BLOCKED (O-01).

### PO-11 FTYP^PRCHFPNT

- **ID and location:** PO-11; `Packages/IFCAP/Routines/PRCHFPNT.m` @dd3fd0f3; lines 41-53; tag `FTYP`.
  ```text
  FTYP ; RETURN FACILITY TYPE IN 'PRCHFTYP'
   N PRCSUB
  ```
- **Rust item:** `crate::inquiry::ftyp_prchfpnt`, `fn ftyp_prchfpnt(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRC("SITE")` (reset at [PRCHDP1.m:24]), `D0`; `PRCHFTP2` undefined on this path.
- **Reads:** `^PRC(411,site,0)` piece 7 (field .5 [DD.zwr:488809]), default 1; node 23 piece 7 (field 31 [DD.zwr:514881]); `^PRC(411.2,code,0)` piece 2 (field 1 [DD.zwr:489640]); all through `$G`.
- **Branches:** PO-11.b1 (exercised), PO-11.b2 (substation, n/e).
- **Outputs:** none. **Side effects:** `PRCHFTYP`; `X` overwritten with the facility-type code [PRCHFPNT.m:45]; `K PRCHFTP2`.
- **Dependencies and coverage:** called at [PRCHDP1.m:25]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-12 ^%ZIS

- **ID and location:** PO-12; `Packages/Kernel/Routines/ZIS.m` @dd3fd0f3 lines 28-37 and `ZIS1.m` lines 103-113; unlabeled entry.
  ```text
   I $D(IOP),IOP=$I!(IOP="HOME")!(0[IOP),$D(^XUTL("XQ",$J,"IO")) D HOME K %IS,%Y,%ZIS,%ZISB,%ZISV,IOP Q
  ```
- **Rust item:** `crate::inquiry::pct_zis`, `fn pct_zis(_s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt>`; Stub.
- **Inputs:** `IOP="HOME"`, `%ZIS=""`.
- **Reads (original):** `^XUTL("XQ",$J,"IO")` [ZIS.m:28]; the device and terminal-type entries that set `IOF`, `IOSL` [ZIS1.m:103-106, 113].
- **Branches:** PO-12.b1, BLOCKED (O-01).
- **Outputs:** the original writes whatever the HOME device set-up writes; unresolved. `pct_zis` always returns `UnsupportedBranch("PO-12.b1")`.
- **Side effects (original):** `IO*` variables.
- **Dependencies and coverage:** called at [PRCHDP1.m:12]. GC-01, 02, 05. Tests: T-DEVICE (asserts `UnsupportedBranch("PO-12.b1")` while O-01 is open), GC-FOUND. BLOCKED.

### PO-13 FOB^PRCHDP1

- **ID and location:** PO-13; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; lines 53-57; tag `FOB`.
  ```text
  FOB W !,"FOB POINT: ",$S("O"=$E($P(PRCHD1,U,6)):"ORIGIN","D"=$E($P(PRCHD1,U,6)):"DESTINATION",1:""),?29,"|","PROPOSAL: " S DIWL=1,DIWR=16,DIWF="",X=$P(PRCHD1,U,8) K ^UTILITY($J,"W") D DIWP^PRCUTL($G(DA))
  ```
- **Rust item:** `crate::inquiry::fob_prchdp1`, `fn fob_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHD0`, `PRCHD1`, `D0`, `DA`.
- **Reads:** node 1 pieces 6, 8, 14 (fields 6.4, 8.2, 8.1); node 14 entries and `^PRC(442.4,auth,0)` piece 2 [DD.zwr:517623, 518097]; node 0 piece 5 (field 2); node 23 piece 11 (field 48). `^(Y,0)` under node 14 is an unguarded read and a `CorruptStore` site [PRCHDP1.m:56].
- **Branches:** PO-13.b1 (the authority loop only formats, so it is not a decision).
- **Outputs:** `\n`, `FOB POINT: `, `ORIGIN`, `DESTINATION` or nothing; column 29 `|`, `PROPOSAL: `; the proposal wrapped at width 16 (PO-14, PO-15) and copied (PO-16); column 40 wrap line 1, column 57 `|AUTHORITY: `, `\n`, `COST CENTER: ` and field 2, column 29 `|`, column 40 wrap line 2, column 57 `|  `; the 442.4 abbreviation of each node-14 entry, separated by `,`; `\n`, `TYPE: ` and field 8.1, then `TY` (PO-17) and `X`, column 29 `|`, column 40 wrap line 3, column 57 `|BUYER:` when node 23 piece 11 is set, else `|AGENT:` [PRCHDP1.m:53-57]. Wrap lines beyond 3 are not displayed.
- **Side effects:** `DIWL`, `DIWR`, `DIWF`, `X`, `Y`, `I`; the word-wrap scratch in `Locals`.
- **Dependencies and coverage:** reached from PO-10 by fall-through [PRCHDP1.m:52]; calls PO-14, PO-16, PO-17; falls into PO-18. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-14 DIWP^PRCUTL

- **ID and location:** PO-14; `Packages/IFCAP/Routines/PRCUTL.m` @dd3fd0f3; lines 33-35; tag `DIWP` with formal `DA`.
  ```text
  DIWP(DA) ;call DIWP^PRCUTL($G(DA)) to save DA value
   D ^DIWP
   QUIT
  ```
- **Rust item:** `crate::inquiry::diwp_prcutl`, `fn diwp_prcutl(s: &mut Session, st: &Store, v: &mut Locals, da: &str) -> Result<(), Halt>`; Ported. Called as `D DIWP^PRCUTL($G(DA))` [PRCHDP1.m:53] [PRCHDP2.m:47]; `da` is by value, and the caller's `DA` is unchanged after the call.
- **Inputs:** `X`, `DIWL`, `DIWR`, `DIWF`. **Reads:** none.
- **Branches:** PO-14.b1.
- **Outputs:** none. **Side effects:** through PO-15.
- **Dependencies and coverage:** calls PO-15 [PRCUTL.m:34]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-15 ^DIWP

- **ID and location:** PO-15; `Packages/VA FileMan/Routines/DIWP.m` @dd3fd0f3; lines 28-77; unlabeled entry.
  ```text
   S:'$L(X) X=" "
   S DIWTC=X[($C(124)_"TAB") S:'$D(DN) DN=1
  ```
- **Rust item:** `crate::inquiry::diwp`, `fn diwp(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Repro.
- **Inputs:** `X` (one text line), `DIWL=1`, `DIWR` (16 or 33), `DIWF=""`; the scratch `^UTILITY($J,"W",DIWL)` (count) and `^UTILITY($J,"W",DIWL,n,0)` (lines) held in `Locals`.
- **Reads:** none from `Store`.
- **Branches:** PO-15.b1 (plain text, no `|`, no `TAB`), b2 (`|` window evaluation through `RCR^DIWW`, n/e), b3 (other `DIWF` letters and `TAB`, n/e).
- **Reproduction rules for b1**, each from the cited line:
  1. An empty `X` becomes one space [DIWP.m:28].
  2. With no line yet, the count becomes 1 and the current line `DIWI` starts empty [DIWP.m:31, 47]. Otherwise `DIWI` is the last stored line, so successive calls continue the same paragraph [DIWP.m:32]; if `X` begins with a space or is all punctuation, a new line is started first [DIWP.m:34, 67-68].
  3. Room is `Z = DIWR - DIWL + 1 - $L(DIWI)` [DIWP.m:59]. If the first word of `X` is longer than `Z`, the FULL rule applies: on an empty line the whole word is placed; then the line is stored, a new one begun, and the remaining text (on a non-empty line, the whole word) is wrapped again from this step [DIWP.m:59, 61-62].
  4. Otherwise the break position `%` is the largest `% <= Z` where character `%` of `X` is a space or lies beyond the end, moved one further when the next character is also a space [DIWP.m:59]. The chunk `$E(X,1,%-1)` is appended to `DIWI` and `X` becomes `$E(X,%+1,…)`; if text remains, the line is stored, a new line begun and step 3 repeats [DIWP.m:60, 62]. A line therefore holds at most `DIWR - DIWL` characters of text before any trailing space.
  5. When the last chunk is appended, one space is added, or two when the chunk ends with `.`, unless it ends with a space [DIWP.m:60].
  6. The current line is stored as line `count` [DIWP.m:43, 49].
- **Outputs:** none. **Side effects:** the scratch lines; `DN`, `DIWT`, `DIWI`, `DIWX`, `DIWTC`, `I`, `Z`, `%` set.
- **Dependencies and coverage:** called from PO-14 [PRCUTL.m:34]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-16 %XY^%RCR

- **ID and location:** PO-16; `Packages/VA FileMan/Routines/DIRCR.m` @dd3fd0f3 lines 24-30 (FileMan 22.2), and the Kernel copy `ZTRCR.m` lines 21-27 (FileMan 19.0); tag `%XY`.
  ```text
  %XY ;
   N %A,%B,%Q,%Z
   S %A=$$R(%X),%Q=""""""
  ```
- **Rust item:** `crate::inquiry::pct_xy_pct_rcr`, `fn pct_xy_pct_rcr(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Repro.
- **Inputs:** literal `%X="^UTILITY($J,""W"","` and `%Y="^TMP($J,""W"","` [PRCHDP1.m:54] [PRCHDP2.m:48].
- **Reads:** none from `Store`.
- **Branches:** PO-16.b1 (copy every descendant node of `^UTILITY($J,"W")` to the same subscripts under `^TMP($J,"W")`, after the caller's `K ^TMP($J,"W")`) [DIRCR.m:29].
- **Outputs:** none. **Side effects:** the `^TMP($J,"W")` scratch in `Locals`.
- **Dependencies and coverage:** GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED; which copy is installed is O-04, and both copy the subtree.

### PO-17 TY^PRCHDP1

- **ID and location:** PO-17; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; lines 71-72; tag `TY`.
  ```text
  TY S X=+$P(PRCHD1,U,7),X=$P($G(^PRCD(420.8,X,0)),U,1),X=$S(X=2:"PURCHASE ORDER",X="B":"DELIVERY & PURCHASE ORDER",X="":"",1:"DELIVERY ORDER")
   Q
  ```
- **Rust item:** `crate::inquiry::ty_prchdp1`, `fn ty_prchdp1(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHD1`. **Reads:** node 1 piece 7 (field 8 [DD.zwr:514659]); `^PRCD(420.8,code,0)` piece 1 [DD.zwr:496661].
- **Branches:** PO-17.b1 (`$SELECT` arms are not decisions).
- **Outputs:** none; sets `X` to the label, written by the caller [PRCHDP1.m:57]. `X=2` is a text comparison (M `=` compares strings), so it holds only when the code is the text `2` [PRCHDP1.m:71].
- **Side effects:** local `X` is overwritten three times: `+$P(PRCHD1,U,7)`, then piece 1 of the 420.8 entry, then the label [PRCHDP1.m:71]; `ty_prchdp1` sets the `Locals` field `x` to the label. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP1.m:57]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-18 DIS^PRCHDP1

- **ID and location:** PO-18; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; lines 58-60; tag `DIS`.
  ```text
  DIS W !,"DELIVER ON/BEFORE " S Y=$P(PRCHD0,U,10) D DT W ?29,"|","CONTRACT: "
   S Y=+$P(PRCHD1,U,10),Y=$P($G(^VA(200,Y,0)),U,1) W ?57,"|  ",$P(Y,",",2)," ",$P(Y,",",1)
  ```
- **Rust item:** `crate::inquiry::dis_prchdp1`, `fn dis_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHD0`, `PRCHD1`, `D0`.
- **Reads:** node 0 piece 10 (field 7 [DD.zwr:514650]); node 1 piece 10 (field 16 [DD.zwr:514742]); `^VA(200,buyer,0)` piece 1 [DD.zwr:349371] through `$G`; `$D(^PRC(442,D0,5,0))` (field 9.2 [DD.zwr:514680]).
- **Branches:** PO-18.b1 (no node 5), PO-18.b2 (discount terms, n/e).
- **Outputs:** `\n`, `DELIVER ON/BEFORE `, the date (PO-19), column 29 `|`, `CONTRACT: `; column 57 `|  `, the name part after the first `,`, a space, the part before it; `\n`, `DISCOUNT TERM: ` [PRCHDP1.m:58-60].
- **Side effects:** `Y`, `X`. **Dependencies and coverage:** reached by fall-through from PO-13; calls PO-19; falls into PO-20. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-19 DT^PRCHDP1

- **ID and location:** PO-19; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; lines 69-70; tag `DT`.
  ```text
  DT I Y W Y\100#100,"/",Y#100\1,"/",Y\10000+1700
   Q
  ```
- **Rust item:** `crate::inquiry::dt_prchdp1`, `fn dt_prchdp1(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `Y` (a FileMan date). **Reads:** none.
- **Branches:** PO-19.b1 (the `IF` governs only a WRITE).
- **Outputs:** when `Y` is non-zero, `month/day/year` unpadded, evaluated strictly left to right (`(Y\100)#100`, `(Y#100)\1`, `(Y\10000)+1700`); `3261028` gives `10/28/2026`.
- **Side effects:** none: `DT` only tests `Y` and writes the date, and sets no local and no scratch [PRCHDP1.m:69-70]; `dt_prchdp1` reads the `Locals` field `y` and changes no field. Business data: none.
- **Dependencies and coverage:** called at [PRCHDP1.m:58, 66]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-20 CON^PRCHDP1

- **ID and location:** PO-20; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; lines 61-64; tag `CON`.
  ```text
  CON K Y S X=0 F I=1:1:3 S:(X'="") X=$O(^PRC(442,D0,2,"AC",X)) D:X'=""  D:I=2 APP W ?29,"|" W:$D(Y) ?38,$S($D(^(Y)):$J(^(Y),3),1:"") W:X'="" ?41,X W ?57,"|" D DAT:I=1,EST:I=2,TOT:I=3 W ! K Y
   .S:$D(^(X)) Y=$O(^(X,0))
   F I=1:1:80 W "-"
  ```
- **Rust item:** `crate::inquiry::con_prchdp1`, `fn con_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported. `G ^PRCHDP2` [PRCHDP1.m:64] is a tail call to `prchdp2`.
- **Inputs:** `D0`, `PRCHD0`, `PRCHD1`.
- **Reads:** the item index `^PRC(442,D0,2,"AC",contract,item)` (442.01 field 4 [DD.zwr:516134]).
- **Branches:** PO-20.b1 (a contract number found: its count and number on the row, n/e), PO-20.b2 (none further, in each of the three rows).
- **Outputs on b2:** three rows: row 1 column 29 `|`, column 57 `|`, `DATE: ` and the order date (PO-22); row 2 `APP: …` (PO-21), column 29 `|`, column 57 `|`, the estimated flag (PO-23); row 3 column 29 `|`, column 57 `|`, `TOTAL: ` (PO-24); each row ends `\n`; then 80 dashes [PRCHDP1.m:61-63].
- **Side effects:** `K Y` before the rows and again at the end of each row, so the `Y` that `DAT` (PO-22) and `EST` (PO-23) set is killed [PRCHDP1.m:61]; `X` is set to 0, then to `$O(^PRC(442,D0,2,"AC",X))`, which is `""` on b2 [PRCHDP1.m:61]; the loop counter `I` runs 1 to 3, then 1 to 80 for the dashes, and is left at 80 [PRCHDP1.m:61, 63]. `con_prchdp1` sets the `Locals` field `y` to `None` before the rows and after each row, and `x` to `"0"` and then `""`; `I` is a Rust loop counter and is not kept. Scratch and business data: none.
- **Dependencies and coverage:** falls in from PO-18; calls PO-21 to PO-24; jumps to PO-25. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-21 APP^PRCHDP1

- **ID and location:** PO-21; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; line 65; tag `APP`.
  ```text
  APP W "APP: ",$P(PRCHD0,U,4),"-",$P($P(PRCHD0,U,3)," ",1) Q
  ```
- **Rust item:** `crate::inquiry::app_prchdp1`, `fn app_prchdp1(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHD0`. **Reads:** node 0 pieces 4 and 3 (fields 1.4, 1 [DD.zwr:514551, 514505]).
- **Branches:** PO-21.b1. **Outputs:** `APP: `, appropriation, `-`, first word of the FCP.
- **Side effects:** none: `APP` only writes [PRCHDP1.m:65]; `app_prchdp1` reads the `Locals` field `prchd0` and changes no field. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP1.m:61] for row 2. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-22 DAT^PRCHDP1

- **ID and location:** PO-22; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; line 66; tag `DAT`.
  ```text
  DAT W "DATE: " S Y=$P(PRCHD1,U,15) D DT Q
  ```
- **Rust item:** `crate::inquiry::dat_prchdp1`, `fn dat_prchdp1(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHD1`. **Reads:** node 1 piece 15 (field .1 [DD.zwr:514394]).
- **Branches:** PO-22.b1. **Outputs:** `DATE: ` and the date (PO-19). **Side effects:** `Y`.
- **Dependencies and coverage:** called at [PRCHDP1.m:61] for row 1. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-23 EST^PRCHDP1

- **ID and location:** PO-23; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; line 68; tag `EST`.
  ```text
  EST S Y=$S($D(^PRC(442,D0,7)):$P(^(7),U,3),1:"") Q:Y'="Y"  W "ESTIMATED" Q
  ```
- **Rust item:** `crate::inquiry::est_prchdp1`, `fn est_prchdp1(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`. **Reads:** node 7 piece 3 (field .08 [DD.zwr:514379]).
- **Branches:** PO-23.b1 (not `Y`: quit, nothing written), PO-23.b2 (`Y`: `ESTIMATED`, n/e).
- **Outputs:** none on b1. **Side effects:** `Y`.
- **Dependencies and coverage:** called at [PRCHDP1.m:61] for row 2. GC-01, 02, 05. Tests: T-EST (field .08 `Y` gives `UnsupportedBranch("PO-23.b2")`; any other value writes nothing and returns `Ok`), GC-FOUND. SOURCE-RESOLVED.

### PO-24 TOT^PRCHDP1

- **ID and location:** PO-24; `Packages/IFCAP/Routines/PRCHDP1.m` @dd3fd0f3; line 67; tag `TOT`.
  ```text
  TOT W "TOTAL: ",$J($P(PRCHD0,U,15),8,2) Q
  ```
- **Rust item:** `crate::inquiry::tot_prchdp1`, `fn tot_prchdp1(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHD0`. **Reads:** node 0 piece 15 (field 91 [DD.zwr:515190]).
- **Branches:** PO-24.b1. **Outputs:** `TOTAL: ` and `$J(total,8,2)`.
- **Side effects:** none: `TOT` only writes [PRCHDP1.m:67]; `tot_prchdp1` reads the `Locals` field `prchd0` and changes no field. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP1.m:61] for row 3. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-25 ^PRCHDP2

- **ID and location:** PO-25; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 9-18; unlabeled entry.
  ```text
   N PRCHAMNT,PRCHAMCT S PRCHAMNT=0 I $D(^PRC(442,D0,6,0)) S PRCHAMCT=$P(^PRC(442,D0,6,0),U,3),PRCHAMNT=1    ;PRC*5.1*221
   W !?8,"ENTER '^' TO HALT: " S PRCHDQ=0 R X:DTIME S:X["^" PRCHDQ=1 G ASK2:PRCHDQ D HDR
  ```
- **Rust item:** `crate::inquiry::prchdp2`, `fn prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported. `G ASK2` and `G COM` are calls to `ask2_prchdp2` and `com_prchdp2` that end this function.
- **Inputs:** `D0`, `PRCHD0`.
- **Reads:** `^PRC(442,D0,6,0)` piece 3 (guarded); items `$O(^PRC(442,D0,2,n))`, stopping at the first non-positive subscript, each `^(n,0)` (unguarded: `CorruptStore` site) and `^(n,2)` (guarded) [PRCHDP2.m:11]; node 3 (field 14 [DD.zwr:514730]); node 0 piece 13 (field 13); `$D(^PRC(442,D0,15,0))` (field 24 [DD.zwr:514843]).
- **Branches:** PO-25.b1 (`^` anywhere in the halt answer: `G ASK2`, n/e), b2 (otherwise: `HDR`, then `ITEM` once per item), b3 (no line item, n/e), b4 (`PRCHDQ` set after an item, n/e), b5 (discount entries, n/e), b6 (estimated-shipping line when field 13 > 0, then no node 15: `G COM`), b7 (certifications, n/e).
- **Outputs on b2/b6:** `\n`, column 8, `ENTER '^' TO HALT: `; one READ (no input left: `Halt::InputExhausted`); `HDR` (PO-26); per item `ITEM` (PO-27); when field 13 > 0: `\n`, column 2 `$J(N+1,3)`, column 7 `EST. SHIPPING AND/OR HANDLING`, column 58 `$J(field 13,7,2)`, where `N` is piece 1 of the last item's zero node [PRCHDP2.m:11, 13].
- **Side effects:** `PRCHAMNT`, `PRCHAMCT` (NEWed), `PRCHDQ`, `X`, `N`, `PRCHDI`, `PRCHDI0`, `PRCHDI2`.
- **Dependencies and coverage:** jumped to from PO-20 [PRCHDP1.m:64]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-26 HDR^PRCHDP2

- **ID and location:** PO-26; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 42-43; tag `HDR`.
  ```text
  HDR W:$Y>0 @IOF,!!?55,"UNIT",?70,"TOTAL",!,"ITEM",?15,"DESCRIPTION",?42,"QTY",?46,"UNIT",?55,"COST",?70,"COST",! F I=1:1:80 W "-"
   Q
  ```
- **Rust item:** `crate::inquiry::hdr_prchdp2`, `fn hdr_prchdp2(s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `$Y`, `IOF`. **Reads:** none.
- **Branches:** PO-26.b1, BLOCKED (O-01): `@IOF` is unresolved, and the postcondition `$Y>0` governs every argument of the WRITE, so the column header is written only when `$Y>0`; `$Y` after a form feed is part of O-01.
- **Outputs (original):** form feed, `\n\n`, column 55 `UNIT`, column 70 `TOTAL`, `\n`, `ITEM`, column 15 `DESCRIPTION`, column 42 `QTY`, column 46 `UNIT`, column 55 `COST`, column 70 `COST`, `\n`, then 80 dashes unconditionally. When `$Y>0`, `hdr_prchdp2` returns `UnsupportedBranch("PO-26.b1")` before writing anything; when `$Y=0` it writes the 80 dashes and returns `Ok`. In the port `$Y>0` always holds when it is called: its one caller, `prchdp2`, first writes the `!` of [PRCHDP2.m:10], and `Session` never resets `$Y`; the other call site, [PRCHDP2.m:31], is on the n/e PO-30.b2, which `ask_prchdp2` never runs.
- **Side effects:** the loop counter `I` of the dashes, left at 80 [PRCHDP2.m:42]. `hdr_prchdp2` changes no `Locals` field: through `form_feed` it returns `UnsupportedBranch("PO-26.b1")` before writing anything when `$Y>0`. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP2.m:10, 31]. GC-01, 02, 05. Tests: GC-FOUND. BLOCKED.

### PO-27 ITEM^PRCHDP2

- **ID and location:** PO-27; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 44-66; tag `ITEM`.
  ```text
  ITEM S DIWL=1,DIWR=33,DIWF="",PRCHDIW=0 K ^UTILITY($J,"W")
   N PURCTYPE,PURPIPE,PRCHI,PRCHJ S:$P($G(^PRC(442,D0,23)),"^",11)="S" PURCTYPE=1   ;PRC*5.1*221  
   D PIPECK S PRCHDIW=0   ;PRC*5.1*221
  ```
- **Rust item:** `crate::inquiry::item_prchdp2`, `fn item_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`, `PRCHDI` (item IEN), `PRCHDI0` (item node 0), `PRCHDI2` (item node 2), `PRCHDQ`, `IOSL`.
- **Reads:** description lines `^PRC(442,D0,2,PRCHDI,1,k,0)` (guarded) [PRCHDP2.m:47]; `^PRCD(420.5,unit,0)` piece 1 [PRCHDP2.m:51, 59]; item node 4 piece 12 [PRCHDP2.m:56]; `$D(^PRC(442,D0,2,PRCHDI,3,"AC"))` [PRCHDP2.m:57]; `$O(^PRC(442,D0,22,"B",boc,0))` and `^PRC(442,D0,22,n,0)` piece 3 [PRCHDP2.m:62-63]; node 23 pieces 8 and 11. Field lines: section "FileMan definitions read", 442.01 row.
- **Branches:** PO-27.b1 (a regular item), b2 (no description line, n/e), b3 (continuation description lines, `PRCHDCNT>1`, n/e), b4 (`PRCHDQ` set, n/e), b5 (partial receipts, n/e). Calls `PIPECK` (PO-28), `DIWP` per description line (PO-14), the copy (PO-16), `EDISTAT` (PO-29), and `ASK` (PO-30, BLOCKED) unconditionally [PRCHDP2.m:60].
- **Outputs on b1**, in order [PRCHDP2.m:49-65]: `\n`; unless `PURCTYPE`: column 2 `$J(+piece 1,3)`; column 7 the first wrapped description line; unless `PURCTYPE`: column 40 `$J(quantity,5)`, column 47 the 420.5 name, column 52 the unit cost (section "Amounts"); column 67 `$J(node 2 piece 1,7,2)`; conditional detail lines `\n` column 8 `STK#: ` (piece 6), `\n` column 8 `NSN:  ` (piece 13), `\n` column 8 `FOOD GROUP: ` (node 4 piece 12), `\n` column 8 `QTY PREV RCVD: ` with `$J(node 2 piece 8,5)`; `EDISTAT`; when not `PURCTYPE` and piece 12 is non-zero: `\n` if `ZZ` is 0, column 8 `Items per `, the 420.5 name, `: `, piece 12, `\n`; `ASK`; `\n` when `$X>1`; column 8 `BOC: ` and the first word of piece 4; when the BOC has an FMS line and node 23 piece 8 is empty: column 22 `FMS LINE: ` and the last three characters of `00` + 442.041 piece 3; column 40 `CONTRACT: ` and node 2 piece 2 when set; `\n`.
- **Side effects:** `PURCTYPE`, `PURPIPE`, `PRCHI`, `PRCHJ` (NEWed), `ZZ` (NEWed), `DIWL`, `DIWR`, `DIWF`, `PRCHDIW`, `PRCHDCNT`, `X`, `K`, `FMSLN`; scratch.
- **Dependencies and coverage:** called at [PRCHDP2.m:11]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED; unreachable in this run behind PO-12.b1, and `ASK` is BLOCKED.

### PO-28 PIPECK^PRCHDP2

- **ID and location:** PO-28; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 96-102; tag `PIPECK`.
  ```text
  PIPECK ;check for invalid pipe '|IN ' command in item description   ;PRC*5.1*221
   S PURPIPE=0,PRCH=0
   F PRCHI=1:1 S PRCH=$O(^PRC(442,D0,2,PRCH)),PRCHDIW=0 Q:'PRCH  D  Q:PURPIPE
  ```
- **Rust item:** `crate::inquiry::pipeck_prchdp2`, `fn pipeck_prchdp2(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`. **Reads:** every item's description lines `^PRC(442,D0,2,n,1,k,0)` (guarded), for every item of the order, not only the current one [PRCHDP2.m:98-99].
- **Branches:** PO-28.b1 (no line contains the literal `| IN `, a pipe, space, `IN`, space), b2 (one does: `Q:PURPIPE`, n/e), b3 (an item with no description line, n/e). The outer loop's "no item" outcome is unreachable from the only call site, `ITEM`.
- **Outputs:** none. **Side effects:** `PURPIPE`, `PRCH`, `PRCHI`, `PRCHJ`, `PRCHDIW`, and `X` (left as the last line scanned, overwritten by `ITEM` before use [PRCHDP2.m:47]).
- **Dependencies and coverage:** called at [PRCHDP2.m:46]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-29 EDISTAT^PRCHUTL

- **ID and location:** PO-29; `Packages/IFCAP/Routines/PRCHUTL.m` @dd3fd0f3; lines 65-80; tag `EDISTAT` with formals `D0`, `D1`, `LINECNT`.
  ```text
  EDISTAT(D0,D1,LINECNT) ;DISPLAY P.O.'S EDI STATUS & QUANTITY
  ```
- **Rust item:** `crate::inquiry::edistat_prchutl`, `fn edistat_prchutl(_s: &mut Session, st: &Store, _v: &mut Locals, d0: &str, d1: &str, _linecnt: &mut String) -> Result<(), Halt>`; Ported. Called as `D EDISTAT^PRCHUTL(D0,PRCHDI,.ZZ)` [PRCHDP2.m:58]: `d0` and `d1` by value, `_linecnt` by reference (`ZZ`, set to 0 by the caller); `_linecnt` is unused because only the n/e status lines (PO-29.b3) count into it [PRCHUTL.m:76-78].
- **Inputs:** formals `D0` and `D1` by value, bound to the caller's `D0` and `PRCHDI`, which `item_prchdp2` passes from the `Locals` fields `d0` and `prchdi` [PRCHDP2.m:58]; `LINECNT` by reference to the caller's `ZZ`, which `N ZZ S ZZ=0` defines first [PRCHDP2.m:58], so the default `S:'$D(LINECNT) LINECNT=0` never fires [PRCHUTL.m:72]. No inherited local is read. Precondition: `D0` is an order and `D1` one of its items [PRCHUTL.m:66-67].
- **Reads:** `$D(^PRC(442,D0,2,D1,2))` and its pieces 9-12 (fields 12, 13 [DD.zwr:516265, 516276]).
- **Branches:** PO-29.b1 (item node 2 absent, n/e), b2 (node 2 present, both EDI codes empty: quit), b3 (EDI status lines through `Y^DIQ`, n/e).
- **Outputs:** none on b2. **Side effects:** `X`, `Y`, `C` are NEWed [PRCHUTL.m:71]; `LINECNT` (`_linecnt`) unchanged on b2.
- **Dependencies and coverage:** GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-30 ASK^PRCHDP2

- **ID and location:** PO-30; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 31-32; tag `ASK`.
  ```text
  ASK I $Y+5>IOSL W !?8,"ENTER '^' TO HALT: " R X:DTIME S:X["^" PRCHDQ=1 D:'PRCHDQ HDR Q
   Q
  ```
- **Rust item:** `crate::inquiry::ask_prchdp2`, `fn ask_prchdp2(s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `$Y`, `IOSL`. **Reads:** none.
- **Branches:** PO-30.b1 (`$Y+5'>IOSL`: no prompt), BLOCKED (O-01, `IOSL` unresolved); PO-30.b2 (paging prompt and `HDR`, n/e). `ask_prchdp2` returns `UnsupportedBranch("PO-30.b1")` while `IOSL` is unresolved.
- **Outputs:** `ask_prchdp2` writes nothing. While `IOSL` is unresolved in `STUB_CONTEXT` it returns `UnsupportedBranch("PO-30.b1")` (BLOCKED, O-01); were `IOSL` resolved, it would return `Ok` on b1 and `UnsupportedBranch("PO-30.b2")` when `$Y+5>IOSL`. In the original, b1 writes nothing, and the n/e b2 writes `\n`, column 8 `ENTER '^' TO HALT: `, reads one line, and runs `HDR` (PO-26) unless the answer contains `^` [PRCHDP2.m:31]. That READ lies only on the n/e b2, so `ask_prchdp2` never reads and never meets input exhaustion. Return value: none (called by `DO`). No `CorruptStore` site.
- **Side effects:** none on b1. The n/e b2 sets `X` to the answer and `PRCHDQ=1` when it contains `^` [PRCHDP2.m:31]. `ask_prchdp2` changes no `Locals` field. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP2.m:60] (and on n/e paths at 18, 21, 54, 71, 76). GC-01, 02, 05. Tests: GC-FOUND. BLOCKED.

### PO-31 COM^PRCHDP2

- **ID and location:** PO-31; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 19-21; tag `COM`.
  ```text
  COM G:'$D(^PRC(442,D0,4,0)) PT K ^UTILITY($J,"W") S DIWL=1,DIWR=60,PRCHJ=0 F  S PRCHJ=$O(^PRC(442,D0,4,PRCHJ)) Q:PRCHJ=""  S X=^(PRCHJ,0) D DIWP^PRCUTL($G(DA))
  ```
- **Rust item:** `crate::inquiry::com_prchdp2`, `fn com_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`, the order on display, set at [PRCHRPT.m:74]; `com_prchdp2` takes it from the `Locals` field `d0`. The n/e b2 also passes `$G(DA)` [PRCHDP2.m:19]. Precondition: entered by `G COM` from PO-25 [PRCHDP2.m:14]. **Reads:** `$D(^PRC(442,D0,4,0))` (field 20 [DD.zwr:514819]).
- **Branches:** PO-31.b1 (no node 4: `G PT`), PO-31.b2 (comments, n/e). **Outputs:** none on b1.
- **Side effects:** none on b1: `G PT` is taken before the `K ^UTILITY($J,"W")` and the `DIWL`, `DIWR`, `PRCHJ` and `X` SETs, which belong with the `^TMP($J,"W")` copy to the n/e b2 [PRCHDP2.m:19-20]. `com_prchdp2` changes no `Locals` field. Scratch and business data: none.
- **Dependencies and coverage:** reached by `G COM` [PRCHDP2.m:14]; jumps to PO-32. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-32 PT^PRCHDP2

- **ID and location:** PO-32; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 22-25; tag `PT`.
  ```text
  PT I $O(^PRC(442,D0,13,0)) W !!?8,"V.A. TRANSACTION NUMBERS: " F PRCHI=0:0 S PRCHI=$O(^PRC(442,D0,13,PRCHI)) Q:'PRCHI  I $D(^PRCS(410,PRCHI,0)) W !?14,$P(^(0),U,1)
   D AMENDS^PRCHDP6
  ```
- **Rust item:** `crate::inquiry::pt_prchdp2`, `fn pt_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`, which `pt_prchdp2` takes from the `Locals` field `d0`; the n/e b3 also reads `PRCHDQ` [PRCHDP2.m:24]. Precondition: entered by `G PT` from PO-31 [PRCHDP2.m:19]. **Reads:** `$O(^PRC(442,D0,13,0))` (field .05 [DD.zwr:514364]); `$D(^PRC(442,D0,6,0))` (field 50 [DD.zwr:514981]).
- **Branches:** PO-32.b1 (no node 13; `AMENDS`; no node 6; `K ^TMP($J,"PRCHDP6")`), b2 (transaction numbers, n/e), b3 (amendment lines, n/e).
- **Outputs:** none on b1. **Side effects:** kills the `^TMP($J,"PRCHDP6")` scratch [PRCHDP2.m:25].
- **Dependencies and coverage:** reached from PO-31; calls PO-33; falls into PO-34. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-33 AMENDS^PRCHDP6

- **ID and location:** PO-33; `Packages/IFCAP/Routines/PRCHDP6.m` @dd3fd0f3; lines 126-152; tag `AMENDS`.
  ```text
  AMENDS ;SET UP AMENDMENT HISTORY
   Q:'$D(^PRC(442,D0,6))
  ```
- **Rust item:** `crate::inquiry::amends_prchdp6`, `fn amends_prchdp6(_s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`, inherited from the caller, not a formal [PRCHDP6.m:126-127]; `amends_prchdp6` takes it from the `Locals` field `d0`. **Reads:** `$D(^PRC(442,D0,6))`.
- **Branches:** PO-33.b1 (no node 6: quit), PO-33.b2 (amendment history, which begins `K ^TMP($J,"PRCHDP6")` [PRCHDP6.m:129], n/e).
- **Outputs:** none.
- **Side effects:** none on b1: `Q:'$D(^PRC(442,D0,6))` quits before the `NEW` and the `K ^TMP($J,"PRCHDP6")`, which belong to the n/e b2 [PRCHDP6.m:127-129]. `amends_prchdp6` changes no `Locals` field. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP2.m:23]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-34 ASK2^PRCHDP2

- **ID and location:** PO-34; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; line 26; tag `ASK2`.
  ```text
  ASK2 D:'PRCHDQ EN^PRCHDP4 G:'$O(^PRC(442,D0,11,0)) ASK1 W ! S %A="   Review a Receiving Report ",%B="",%=2 D ^PRCFYN G:%'=1 Q
  ```
- **Rust item:** `crate::inquiry::ask2_prchdp2`, `fn ask2_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHDQ`, set to 0 at [PRCHDP2.m:10] and not set again on the exercised path, and `D0`; `ask2_prchdp2` reads the `Locals` fields `prchdq` and `d0`. Precondition: entered by fall-through from PO-32 [PRCHDP2.m:25-26]. **Reads:** `$O(^PRC(442,D0,11,0))` (field .6 [DD.zwr:514467]).
- **Branches:** PO-34.b1 (`PRCHDQ` clear: `EN^PRCHDP4`; no node 11: `G ASK1`), b2 (receiving-report review, n/e), b3 (`PRCHDQ` set: `EN^PRCHDP4` skipped, n/e).
- **Outputs:** through PO-35 to PO-37.
- **Side effects:** none of its own on b1; what `EN^PRCHDP4` changes is under PO-35 to PO-37. The n/e b2 sets `%A`, `%B` and `%` [PRCHDP2.m:26]. `ask2_prchdp2` changes no `Locals` field itself. Scratch and business data: none.
- **Dependencies and coverage:** falls in from PO-32; calls PO-35; jumps to PO-38. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-35 EN^PRCHDP4

- **ID and location:** PO-35; `Packages/IFCAP/Routines/PRCHDP4.m` @dd3fd0f3; lines 4-5; tag `EN`.
  ```text
  EN ;ONLINE DISPLAY OF DELIVERY SCHEDULE
   S PRCHONL=1,PRCHQUIT=0
  ```
- **Rust item:** `crate::inquiry::en_prchdp4`, `fn en_prchdp4(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** none of its own: `EN` reads no local [PRCHDP4.m:4-5]; `D0` passes through to `EN2` (PO-36). Precondition: called only with `PRCHDQ` clear [PRCHDP2.m:26]. **Reads:** none: `EN` reads no global, no local input and no scratch [PRCHDP4.m:4-5].
- **Branches:** PO-35.b1 (sets `PRCHONL=1`, `PRCHQUIT=0`, falls into `EN2`).
- **Outputs:** none of its own; the fall-through into `EN2` (PO-36) reaches `EXDELV` (PO-37), which writes `\n`. No READ and no `CorruptStore` site of its own; `en_prchdp4` returns whatever `en2_prchdp4` returns. Return value: none (called by `DO`).
- **Side effects:** sets `PRCHONL=1` and `PRCHQUIT=0` [PRCHDP4.m:5]; `EN2` resets `PRCHQUIT` to `""` [PRCHDP4.m:8], and `EXDELV` kills both [PRCHDP4.m:13]. Neither is modelled in Rust: their only readers, `PAUSE` [PRCHDP4.m:41] and the schedule print [PRCHDP4.m:11, 25, 29], lie on the n/e PO-36.b3. `en_prchdp4` changes no `Locals` field. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHDP2.m:26]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-36 EN2^PRCHDP4

- **ID and location:** PO-36; `Packages/IFCAP/Routines/PRCHDP4.m` @dd3fd0f3; lines 7-11; tag `EN2`.
  ```text
  EN2 ;ENTRY POINT USED TO EITHER PRINT OR DISPLAY DELIVERY SCHEDULE
   Q:'$D(D0)  S PRCHPONO=$P(^PRC(442,D0,0),U,1),PRCHQUIT="" G:PRCHPONO="" EXDELV K ^TMP($J,"MDEL")
  ```
- **Rust item:** `crate::inquiry::en2_prchdp4`, `fn en2_prchdp4(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `D0`, set at [PRCHRPT.m:74], so `Q:'$D(D0)` never quits; `en2_prchdp4` takes it from the `Locals` field `d0`. `PRCHQUIT` and `PRCHPONO` are set here before any read [PRCHDP4.m:8]. Precondition: `^PRC(442,D0,0)` exists; otherwise `en2_prchdp4` returns `CorruptStore` [PRCHDP4.m:8].
- **Reads:** `^PRC(442,D0,0)` piece 1, unguarded (`CorruptStore` site); `^PRC(442.8,"B",PONO,n)` [DD.zwr:518518].
- **Branches:** PO-36.b1 (`D0` undefined, n/e unreachable: `D0` is set at [PRCHRPT.m:74]), b2 (empty PO number: `G EXDELV`, n/e), b3 (delivery-schedule entries: `BLD`, then print, n/e), b4 (`K ^TMP($J,"MDEL")`; no `B` entry, so no `^TMP($J,"MDEL")`: `G EXDELV`).
- **Outputs:** none on b4. **Side effects:** `PRCHPONO`, `PRCHQUIT`; kills the `MDEL` scratch.
- **Dependencies and coverage:** falls in from PO-35; jumps to PO-37. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-37 EXDELV^PRCHDP4

- **ID and location:** PO-37; `Packages/IFCAP/Routines/PRCHDP4.m` @dd3fd0f3; lines 13-14; tag `EXDELV`.
  ```text
  EXDELV W ! K ^TMP($J,"MDEL"),PRCHI,PRCHONL,PRCHPONO,PRCHQUIT,PRCHX,PRCHY,PRCHZ,I,J,K,X,Y,Z,L
   Q
  ```
- **Rust item:** `crate::inquiry::exdelv_prchdp4`, `fn exdelv_prchdp4(s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** none: `EXDELV` reads no local; it writes `\n` and kills [PRCHDP4.m:13]. Precondition: reached by `G EXDELV` from PO-36 [PRCHDP4.m:10]. **Reads:** none: no global is read, and the `^TMP($J,"MDEL")` scratch is killed, not read [PRCHDP4.m:13].
- **Branches:** PO-37.b1.
- **Outputs:** `\n`. **Side effects:** kills the listed locals (including `X`, `Y`, `I`, `J`, `K`) and the `MDEL` scratch.
- **Dependencies and coverage:** reached from PO-36 [PRCHDP4.m:10]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-38 ASK1^PRCHDP2

- **ID and location:** PO-38; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 33-41; tag `ASK1`.
  ```text
   W !,$C(7) G:PRCHDQ Q W "END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: " R X:DTIME G Q
  ```
- **Rust item:** `crate::inquiry::ask1_prchdp2`, `fn ask1_prchdp2(s: &mut Session, st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** `PRCHAMNT` (NEWed in PO-25), `PRCHDQ`. **Reads:** none from globals or scratch: `$G(PRCHAMNT)` [PRCHDP2.m:33] and `PRCHDQ` [PRCHDP2.m:41] are local inputs, which `ask1_prchdp2` reads from the `Locals` fields `prchamnt` and `prchdq`.
- **Branches:** PO-38.b1 (`PRCHAMNT=2`: the amendment warning, n/e), b2 (`PRCHDQ` set: `G Q`, n/e), b3 (the `END OF DISPLAY` read, then `G Q`). `PRCHAMNT=2` is set only inside the amendment code [PRCHDP6.m:121].
- **Outputs on b3:** `\n<BEL>END OF DISPLAY--PRESS RETURN OR ENTER '^' TO HALT: `; one READ (no input left: `Halt::InputExhausted`); its value is not used.
- **Side effects:** `X` receives the READ answer [PRCHDP2.m:41]; `ask1_prchdp2` sets the `Locals` field `x` to it. The n/e b1 only writes [PRCHDP2.m:34-39]. Scratch and business data: none.
- **Dependencies and coverage:** reached from PO-34; jumps to PO-39. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-39 Q^PRCHDP2

- **ID and location:** PO-39; `Packages/IFCAP/Routines/PRCHDP2.m` @dd3fd0f3; lines 94-95; tag `Q`.
  ```text
  Q ;W @IOF  ;REMOVE IF PROBLEM WITH KERNEL V6.5
   K I,J,K,N,DIC,DIWF,DIWL,DIWR,IOP,PRCHDI,PRCHD0,PRCHD1,PRCHFTYP,PRCHDSIT,PRCHDHSP,PRCHDSHP,PRCHDST,PRCHDS,PRCHDV,PRCHDQ,PRCHDI0,PRCHDI2,PRCHDIW,PRCHDCNT,PRCHI,PRCHJ,PRCHK,S,V,^TMP($J,"W"),^UTILITY($J,"W"),KK,JJ Q
  ```
- **Rust item:** `crate::inquiry::q_prchdp2`, `fn q_prchdp2(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** none: `Q` reads no local; it only kills [PRCHDP2.m:95]. Precondition: reached by `G Q` from PO-38 [PRCHDP2.m:41]. **Reads:** none: no global is read, and the `^TMP($J,"W")` and `^UTILITY($J,"W")` scratch is killed, not read [PRCHDP2.m:95].
- **Branches:** PO-39.b1 (kills locals and scratch; no output; the `W @IOF` is a comment).
- **Outputs:** none: the `W @IOF` is a comment [PRCHDP2.m:94]. No READ, so no input exhaustion, and no `CorruptStore` site; `q_prchdp2` returns `Ok(())`. Return value: none; `Q` ends the `D ^PRCHDP1` that PO-06 made [PRCHRPT.m:74].
- **Side effects:** kills the locals `I`, `J`, `K`, `N`, `DIC`, `DIWF`, `DIWL`, `DIWR`, `IOP`, `PRCHDI`, `PRCHD0`, `PRCHD1`, `PRCHFTYP`, `PRCHDSIT`, `PRCHDHSP`, `PRCHDSHP`, `PRCHDST`, `PRCHDS`, `PRCHDV`, `PRCHDQ`, `PRCHDI0`, `PRCHDI2`, `PRCHDIW`, `PRCHDCNT`, `PRCHI`, `PRCHJ`, `PRCHK`, `S`, `V`, `KK` and `JJ`, and the scratch `^TMP($J,"W")` and `^UTILITY($J,"W")` [PRCHDP2.m:95]. `q_prchdp2` clears their `Locals` counterparts: `n`; `dic_a`, `dic_b` and `dic_s` (the `DIC` subscripts); `diwf`, `diwl`, `diwr`; `prchdi`, `prchd0`, `prchd1`, `prchftyp`, `prchdsit`, `prchdshp`, `prchdst`, `prchds`, `prchdv`, `prchdq`, `prchdi0`, `prchdi2`, `prchdcnt`; `s_rows`, `v_rows`; and the wrap scratch `tmp_w_counts`, `tmp_w_lines`, `utility_w_counts` and `utility_w_lines` (the `^TMP($J,"W")` and `^UTILITY($J,"W")` counts and lines). The other killed locals have no `Locals` field. Business data: none.
- **Dependencies and coverage:** reached from PO-38 [PRCHDP2.m:41]; returns to PO-06. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-40 K^PRCHRPT

- **ID and location:** PO-40; `Packages/IFCAP/Routines/PRCHRPT.m` @dd3fd0f3; lines 106-108; tag `K`.
  ```text
  K K DEFPNT,ZTSK,ZTSAVE,ZTDTH,ZTRTN,PRCHREPR,PRCHNRQ,OK,PL,PRCHQ
   K X,Y,I
   Q
  ```
- **Rust item:** `crate::inquiry::k_prchrpt`, `fn k_prchrpt(_s: &mut Session, _st: &Store, v: &mut Locals) -> Result<(), Halt>`; Ported.
- **Inputs:** none: `K` reads no local [PRCHRPT.m:106-107]. Precondition: called after `D ^PRCHDP1` returns [PRCHRPT.m:74]. **Reads:** none: no global, no local input and no scratch is read [PRCHRPT.m:106-107].
- **Branches:** PO-40.b1 (kills, including `PRCHNRQ`, so the next order's label is decided afresh).
- **Outputs:** none: `K` writes nothing. No READ, so no input exhaustion, and no `CorruptStore` site; `k_prchrpt` returns `Ok(())`. Return value: none (called by `DO`).
- **Side effects:** kills the locals `DEFPNT`, `ZTSK`, `ZTSAVE`, `ZTDTH`, `ZTRTN`, `PRCHREPR`, `PRCHNRQ`, `OK`, `PL` and `PRCHQ`, then `X`, `Y` and `I` [PRCHRPT.m:106-107]. `k_prchrpt` clears the `Locals` fields `prchnrq`, `x` and `y`; the other killed locals have no `Locals` field. Scratch and business data: none.
- **Dependencies and coverage:** called at [PRCHRPT.m:74]; then `G EN30` [PRCHRPT.m:75]. GC-01, 02, 05. Tests: GC-FOUND. SOURCE-RESOLVED.

### PO-41 ^%DT

- **ID and location:** PO-41; call site `Packages/IFCAP/Routines/PRCFSITE.m` @dd3fd0f3 line 15; FileMan `DIDT.m` lines 8-25 and the Kernel copy `ZTDT.m` lines 3-14; tag `%DT`.
  ```text
   I '$D(PRC("FY")) S %DT="",X="T" D ^%DT S A=$E(Y,2,3),B=$E(Y,4,5),PRC("FY")=$E(100+$S(+B>9:A+1,1:A),2,3),PRC("QTR")=$S(B<4:2,B<7:3,B<10:4,1:1) S X=""
  ```
- **Rust item:** `crate::inquiry::pct_dt`, `fn pct_dt(_s: &mut Session, _st: &Store, _v: &mut Locals) -> Result<(), Halt>`; Stub; no formal arguments (it reads the inherited `%DT` and `X`).
- **Inputs:** `%DT=""`, `X="T"`. **Reads:** none.
- **Branches:** PO-41.b1: `%DT` has no `A` or `E` flag, so there is no prompt and no output [DIDT.m:11-12, 63, 67, 92] [ZTDT.m:3-4, 43, 46, 55]; `Y` receives today's date, which no output uses.
- **Outputs:** none. **Side effects:** none modelled (section "Stubs").
- **Dependencies and coverage:** called from PO-03. All five. Tests: T-SESSION. SOURCE-RESOLVED (runtime identity O-04).

## Resolution decisions binding src/inquiry.rs

This section closes the gate that open items O-02 and O-03 set on `prcfsite`, `dic` and `ix_dic`. `src/inquiry.rs` and its unit tests follow it exactly. Each decision is SOURCE-RESOLVED only where every byte is cited; otherwise its branch is BLOCKED and returns `UnsupportedBranch` with its branch ID.

### R1 Station default acceptance (PO-03, PO-05.b1)

| Step | Exact bytes or rule | Evidence |
|---|---|---|
| Newline at `DIVFY` | `\n` | [PRCFSITE.m:8] |
| Newline at `SI` | `\n` | [PRCFSITE.m:18] |
| Newline at `ASK` | `\n` | [DIC.m:22-25] |
| Prompt | `Select STATION NUMBER ('^' TO EXIT): ` | `DIC("A")` [PRCFSITE.m:18]; copied to `DIPRMT(1)` [DIC11.m:37-43]; written [DIC11.m:15-17] |
| Default | `500// ` | `DIC("B")=PRC("SITE")`, which is `$O(^PRC(411,"AC","Y",0))` = 500 [PRCFSITE.m:14, 18] [411 export:290]; `^PRC(411,"B",500)` exists, so `DIY(1)=500` [DIC1.m:11-17] [411 export:292]; field .01 is free text, so no external form is used [DIC1.m:18] [DICUIX2.m:53-60] [DD.zwr:488769]; `DIY_"// "` [DIC11.m:16] |
| READ | one input line, not echoed | [DIC11.m:17] |
| Empty answer | `DIVAL(1)=500`; `O` appended to `DIC(0)` | [DIC11.m:29-32] |
| Silent steps | nothing written: pre-lookup node 7.5 `S PRCRI=$P(^PRC(411,0),U,3)` writes nothing [DIC.m:36] [DD.zwr:488792] [DIC5.m:8-9]. `BYIEN2^DIC5` selects nothing: file 411 has no field .001 and `$O(^PRC(411,"A["))` is `"B"`, so it quits with `Y=-1` [DIC.m:48] [DIC5.m:22-31] [411 export:289-292]. The `B` exact match passes the screen `I +Y<1000000` [DIC3.m:20, 82-96, 109-126]; `DIY` is cut to the text after the typed value, here empty, because the index field list holds `.01` [DIC3.m:97-99] [DICUIX.m:92-95, 107-110] [DICUIX2.m:22]; with `O` set the single exact match is taken at once [DIC3.m:28-31] and passed to `C^DIC2` [DIC2.m:99-102] | as cited in the rule |
| `.01` echo | `  ` (two spaces; the remaining `.01` text is empty) | `WO` sets `DST="  "` and `WR` appends nothing and writes it [DIC2.m:10, 18-25, 32] |
| Identifier prefix | `  ` | `DIC("W")` is `N C,DINAME W "  " ` followed by the 155-character identifier, which fits the 224-character limit [DIC1.m:46-56] [DD.zwr:488724]; executed with the naked reference on `^PRC(411,500,0)` [DIC2.m:11-14] |
| Identifier `.1` | `  CAMP MASTER` | The identifier writes `"  "`, then `Y` = piece 1 of `^DIC(4,+$P(^PRC(411,500,0),U,10),0)` [DD.zwr:488724] [411 export:4] [4+INSTITUTION.zwr:494], unchanged by `Y^DIQ` for type `R` [DD.zwr:38111] [DIQ.m:154-160], then `$E(^PRC(411,500,0),0)`, which is empty |
| No confirmation | nothing | `DO(2)="411I"` has no `O` and `DIASKOK` is undefined [DIC2.m:54-58] [DIC0.m:26] [411 export:3]; the file `ACT` node writes nothing because `$P(Y,"^",3)` is empty [DIC2.m:59-60] [DICM1.m:102-110] [DD.zwr:488721] |

Result, SOURCE-RESOLVED: `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// `, one READ, and for an empty answer `      CAMP MASTER` (six spaces, no newline); `PRC("SITE")=500`. The plan's prediction `  500  CAMP MASTER` is superseded: the `.01` value is not echoed because the user typed nothing and `DIY` holds only the untyped remainder. A non-empty answer returns `UnsupportedBranch("PO-05.b2")`; the answer is classified inside the lookup, before `^PRCFSITE` sees its result, so PO-03.b7 is never the reported ID.

### R2 Unique order match (PO-09.b8)

| Step | Exact bytes or rule | Evidence |
|---|---|---|
| Selection | After the whole search exactly one candidate, found by the exact `C` match; `G^DIC2` writes `DST`, which is empty because the stored remainder `$P(DIX,X,2,99)` is empty and `DIYX(1)` is unset | [DIC3.m:71, 151-154] [DIC2.m:73-97] |
| `.01` echo | `  ` followed by the full `.01` value (for example `500-A12345`) | Index `C` is a MUMPS-type cross-reference [DD.zwr:514318], so its field list is `^^` [DICUIX.m:92-95, 109] and `DIY` keeps `$P(^PRC(442,ien,0),U)` [DIC3.m:100-105, 110]; `WO`/`WR` write `"  "_DIY` [DIC2.m:10, 25, 32] |
| Identifier mode | `WOV^DICQ1` | After `Z.01` `DIC("W")` is 103 characters; adding `Z.02` (136) gives 239 > 224, so `DIC("W")` becomes the `WOV` call [DIC1.m:52-56, 60] [DD.zwr:514202-514203] |
| Per identifier | For `Z.01`, `Z.02`, `Z.03` in that order: `  `, then the identifier's output, executed after `$E(^PRC(442,ien,0),0)` resets the naked reference to `^PRC(442,ien,0)` | [DICQ1.m:86-88] |
| `Z.01` | When node 1 exists: `$E(p,4,5)`, `-`, `$E(p,6,7)`, `-`, `$E(p,2,3)` of node 1 piece 15 (`MM-DD-YY`) | [DD.zwr:514202] |
| `Z.02` | Piece 2 of `^PRCD(442.5,+mop,0)` when that node has data; then, when `^PRC(442,ien,7)` and `^PRCD(442.3,+^(7),0)` exist, `   ` (three spaces) and the first 34 characters of the 442.3 name | [DD.zwr:514203] |
| `Z.03` | When node 0 exists: `\n`, padding to column 13, `FCP: `, the first space-delimited word of node 0 piece 3, `     $ ` (five spaces, `$`, space), `$J(node 0 piece 15,0,2)` | [DD.zwr:514204] |
| No confirmation | nothing | `DO(2)="442I"` [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3] has no `O`, `DIASKOK` is undefined [DIC2.m:54-58], and file 442 has no `ACT` node [DD.zwr:514199-514238] |

Result, SOURCE-RESOLVED: `  <.01>  <Z.01>  <Z.02>  \n` + 13 spaces + `FCP: <fcp>     $ <total>`, with no trailing newline, then `en3_prchpat` continues.

### R3 No-match sequence (PO-09.b14 bytes)

| Step | Exact bytes | Evidence |
|---|---|---|
| Exhaustion | `DS(0)="1^"` set by `BAD^DIC1` after `W` sends any file with a `^PRC(442)` subscript after `"A["` to `DD` | [DICM.m:71, 85-87] [DIC1.m:157-159] |
| `DING` | `<BEL> ??` | `DIC(0)` holds `Q` and `E`; `DUOUT` and `DDS` undefined [DIC.m:63-65, 80-81] |
| Newline | `\n` | `D^DIC0` writes nothing [DIC0.m:8-13]; `W:DIC(0)["E" !` [DIC.m:63] |
| Re-ask | `\n`, `P.O./REQ.NO.: ` | `G ASK`, `W !` [DIC.m:63, 25]; prompt [DIC11.m:15-17] |

Bytes SOURCE-RESOLVED: `<BEL> ??\n\nP.O./REQ.NO.: `, then one READ. The plan's "a newline and the re-asked prompt" omitted the `W !` at `ASK`. The branch stays BLOCKED until R4 closes.

### R4 Transforms (O-03)

- **Settled: no date, set-of-codes or variable-pointer transform runs on file 442's own indexes.** With no candidate, `^DICM` starts at the index the lookup began with, `C`, and moves forward through `^DD(442,0,"IX")` [DIC3.m:70, 79] [DICM.m:13-18] [DICF2.m:118-127]. The indexes after `C` are D (field 5, pointer to 440), E (1, free text), F (.02, pointer to 442.5), G (442.14 field 11, pointer to 445), H (.09, free text), I (102.5, free text), J (102.6, free text), K (102.4, free text), L (442.17 field .01, free text), M (46, pointer to 440.5), MAPP (.7, numeric), MCH (61, pointer to 200) and ST (.5, pointer to 442.3) [DD.zwr:514219-514231]; none has type `D`, `S` or `V` [DICM.m:32-33], and none has a node 7 [DICM.m:30]. PO-09.b12 is therefore unreachable.
- **Settled: a nested pointer miss writes nothing of its own.** `RS` strips `A` and `Q` [DICM.m:107] before `P^DICM0` copies the flags [DICM0.m:10, 15]; its `DING` quits without `Q` and its re-ask needs `A` [DIC.m:63, 80]; `RETRY` is skipped because `DICR(DICR,8)` is set when the outer lookup has no `L` [DICM.m:103] [DICM0.m:44]; the outer flags are restored with `Y=-1` [DICM0.m:51-55] [DICM.m:123-133]. The soundex echo [DICM.m:138] cannot arise: the only soundex `LOOK` nodes are in files 120.82, 787.5 and 19620.12 [DD.zwr:256062, 657203, 812429], none a pointer target of 442.
- **Open: date transforms inside the nested pointer lookups.** The nested lookup keeps `M` (`DIC(0)` becomes `EMZ`), so it searches every index of the pointed-to file (440, 442.5, 445, 440.5, 200, 442.3) and runs that file's own `^DICM` transforms. A date-valued index there converts the value through `^%DT` and, with `E` set, writes `   ` and the date [DICM1.m:13-23] whether or not a candidate follows. Settling this needs every later index of those six files and `%DT`'s treatment of the GC-03 and GC-04 values, and the GC-03 value is not yet chosen. A candidate the converted value finds there is a pointer candidate, PO-09.b11. **Status: OPEN (O-03). Effect: PO-09.b14 is BLOCKED.**
- **Open: set-of-codes transforms inside the nested pointer lookups.** `^DICM` sends an index whose field type contains `S`, and neither `P` nor `D`, to `S^DICM1` [DICM.m:33-34] [DICM1.m:8]. `S^DICM1` [DICM1.m:26-62] tests the value as an internal code [DICM1.m:30-32] and as a prefix of each external value [DICM1.m:34-39]. The nested lookup runs with `DIC(0)="EMZ"`, so `E` is set [DICM.m:107] [DICM0.m:10, 15], and `DDS` is undefined, as for R3's `DING`. For an index with no lookup or display transform [DICM1.m:76-82], an internal-code match writes `  (`, the code, three spaces, the external value and `)` [DICM1.m:32, 75, 83-84]; a single external match writes the same [DICM1.m:41-44]; several external matches write each match on its own line, then `CHOOSE 1-n: `, and READ an input line, re-asking after `<BEL>??` on an invalid answer [DICM1.m:45-52], which consumes one of the case's inputs; no match goes to `NO` and writes nothing [DICM1.m:40, 59]. This output comes before the lookup on the internal code [DICM.m:34, 109-133], so it is written whether or not a candidate follows. The pointed-to files do define set-typed indexes after `B`, where the nested lookup starts [DICM0.m:40]. For example, file 440.5's `ST` is on field 70 STATUS, type `S`, codes `E:EXCEPTION;P:POSTED` [DD.zwr:511917, 511930, 512209, 512211], and `^DICM` reaches it when nothing earlier in file 440.5 yields a candidate [DICM.m:17, 39] [DICF2.m:118-127, 137-143]. Settling this needs every set-typed index those nested lookups visit in the six pointed-to files, with its codes, and the GC-03 and GC-04 values, and the GC-03 value is not yet chosen. A candidate the internal code finds there is a pointer candidate, PO-09.b11. A set-of-codes candidate on file 442's own indexes would be PO-09.b12, which the first settled point makes unreachable. **Status: OPEN (O-03). Effect: PO-09.b14 is BLOCKED.**

### R5 Order-lookup rules for `ix_dic`

**Data provenance (decision).** The repository holds no purchase orders. The file 442 export's header `^PRC(442,0)="PROCUREMENT & ACCOUNTING TRANSACTIONS^442I^^0"` has an empty last-IEN piece 3 and an entry count of 0 in piece 4 [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3]. Its only other nodes, `^PRC(442,"AH",31,99999949)=""` and `^PRC(442,"AI",31,99999949)=""`, are index entries for a record the export does not hold: there is no `^PRC(442,99999949,…)` node [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:4-5]. The 442 export is therefore not a fixture. The `^PRC(442,0)` header, the five orders, their `C` index entries and every other file 442 node the five cases read reach the `Store` only through `tests/fixtures/ifcap/cases.zwr`, which the capture owner (Rainman Kemp) exports from the capture snapshot, together with every other record the cases need, and then adds its line to `tests/fixtures/ifcap/MANIFEST.txt`. The run generates no capture data. Until `cases.zwr` is supplied, a `Store` loaded from the eleven repository fixtures holds no `^PRC(442)` node, so `ix_dic` returns `UnsupportedBranch("PO-09.b15")` at step 1, before writing.

1. **File header** (PO-09.b15): `^PRC(442,0)` must exist with piece 2 exactly `442I` [442+PROCUREMENT & ACCOUNTING TRANSACTIONS.zwr:3]; otherwise the original returns `Y=-1` without a prompt [DIC0.m:22] or displays different identifiers [DIC1.m:46-49], and `ix_dic` returns `UnsupportedBranch("PO-09.b15")` before writing.
2. **Prompt and read:** `\n`, `P.O./REQ.NO.: `, one READ.
3. **Read checks, in source order** [DIC11.m:18-33]: a character outside printable ASCII (0x20-0x7E) or a number with more than 25 integer or 24 fraction digits gives PO-09.b2; exactly `^` gives PO-09.b3; more than 250 characters gives PO-09.b2; one or more `?` only gives PO-09.b4; a backtick followed only by digits and punctuation, or a single space, gives PO-09.b5; empty gives PO-09.b6; any other value beginning with `^` gives PO-09.b3 [DIC11.m:33] [DIC.m:45]. PO-09.b1 (timeout) cannot occur: the Rust READ never times out.
4. **No IEN attempt** (PO-09.b7 unreachable): `IX^DIC` defines `DF` [DIC.m:84-85], so `BYIEN2^DIC5` returns at once [DIC5.m:22] and an all-digit value goes to the search like any other.
5. **Search**, skipped when the value is longer than 30 characters [DIC3.m:13] [DICUIX.m:96-97]:
   - Candidates are IENs, kept in discovery order without repeats [DIC3.m:142, 147].
   - An IEN qualifies only if `^PRC(442,ien,0)` exists, `^PRC(442,ien,-9)` does not, and the screen passes: `+$P(node0,U,1)` equals 500, and piece 2 is numerically below 10 or equal to 25 [DIC3.m:94, 110, 124] [PRCHPAT.m:81] [PRCHRPT.m:9].
   - Exact: each `ien` under `^PRC(442,"C",X,ien)` in collation order, stopping at the first subscript whose numeric interpretation is zero (`I 'Y`) [DIC3.m:20, 83]; a quoted non-canonical subscript such as `"-01"` is numerically nonzero, so the walk continues past it.
   - Partial: each string subscript `S` of `^PRC(442,"C")` after `X` that begins with `X`, with its IENs as above [DIC3.m:36-49].
   - Later indexes: each subscript of `^PRC(442)` collating after `"C"` whose `$D` is 10, searched exact then partial on the raw value [DIC3.m:61-69]; an index on a pointer field (D, F, G, M, MCH, ST) is skipped when `X` is numeric [DIC4.m:84-86]; a subscript not named in `^DD(442,0,"IX")` has no field and is searched as free text [DICUIX.m:74-89] [DICUIX2.m:44, 53-60].
   - Outcome: one candidate found by the exact `C` step gives PO-09.b8 (R2); one candidate otherwise gives PO-09.b9; two or more give PO-09.b10.
6. **No candidate**, in this order of labels: PO-09.b11 when the pointer-candidate rule holds (R6); PO-09.b13 when `X` contains a lowercase letter and its uppercase form yields a candidate under step 5 or R6 [DICM.m:44-47]; PO-09.b16 when no `^PRC(442)` subscript collates after `"A["`, so the original scans the whole file [DICM.m:71-81]; otherwise PO-09.b14, BLOCKED: `UnsupportedBranch("PO-09.b14")` for every such input, with no identifier treated specially.

### R6 Pointer-candidate rule (PO-09.b11)

For each pointer index of file 442 after `C`, in `^DD(442,0,"IX")` order (D to 440 `^PRC(440,`, F to 442.5 `^PRCD(442.5,`, G to 445 `^PRCP(445,`, M to 440.5 `^PRC(440.5,`, MCH to 200 `^VA(200,`, ST to 442.3 `^PRCD(442.3,`): a pointed-to entry `e` whose `B` key equals `X` or is a string beginning with `X` is a candidate only if its own zero node exists and it has no `-9` node [DIC3.m:94, 110], and some `parent > 0` under `^PRC(442,index,e,parent)` has a zero node and passes the parent screen of R5 [DICM0.m:25-37, 40-41]. Files 445 and 440.5 are cited in the source-correction supplement of section "FileMan definitions read". Any qualifying entry gives PO-09.b11. Records without `^PRC(442)` entries skip the rule [DICM0.m:26].

**Labelling approximation (priority decision).** The original's nested lookup also searches the pointed-to file's other indexes, its IENs and its own transforms; the comma and long-input retries [DICM.m:48-66] are not evaluated. These omissions change only which of PO-09.b11, b13, b14 is named in an `UnsupportedBranch`, never a transcript, because all three are n/e or BLOCKED. If O-03 closes and PO-09.b14 becomes a transcript, R5 step 6 and this rule must first be made exact.

### Consequences and test assertions in the resulting state

| Item | State | Effect on `src/inquiry.rs` |
|---|---|---|
| R1 station bytes | SOURCE-RESOLVED | PO-05.b1 is implemented; every session starts with the R1 bytes |
| R2 unique-match bytes | SOURCE-RESOLVED | PO-09.b8 is implemented; a found order then runs PO-10, which stops at `UnsupportedBranch("PO-12.b1")` while O-01 is open |
| R3 no-match bytes | SOURCE-RESOLVED, recorded for later use | Not emitted while R4 is open |
| R4 file-442 own indexes | SOURCE-RESOLVED | PO-09.b12 is unreachable and has no return site: after the pointer route the search goes to the uppercase retry |
| R4 nested date transform | OPEN (O-03) | PO-09.b14 BLOCKED: every input reaching it returns `UnsupportedBranch("PO-09.b14")` |
| R4 nested set-of-codes transform | OPEN (O-03) | PO-09.b14 BLOCKED: every input reaching it returns `UnsupportedBranch("PO-09.b14")` |

Unit-test assertions in this state: `numeric_value_naming_no_ien_follows_no_match_path`, `lowercase_value_whose_retry_fails_follows_no_match_path`, `candidates_all_rejected_by_screen_follow_no_match_path`, `unreferenced_vendor_with_matching_name_is_not_a_candidate` and `referenced_vendor_whose_orders_fail_screen_is_not_a_candidate` assert `UnsupportedBranch("PO-09.b14")`, and would assert `Ok` with the R3 transcript once O-03 closes; `absent_identifiers_follow_no_match_path` asserts that two different absent identifiers give the same outcome, `UnsupportedBranch("PO-09.b14")`; `referenced_vendor_with_qualifying_order_is_unsupported_branch` asserts `UnsupportedBranch("PO-09.b11")` in both states; `found_order_stops_at_device_gate` asserts `UnsupportedBranch("PO-12.b1")` while O-01 is open; `help_at_order_prompt_is_unsupported_branch` asserts `UnsupportedBranch("PO-09.b4")`. Every synthetic store used by these tests holds `^PRC(442,0)` with piece 2 `442I` and at least one `^PRC(442,"C")` entry, or it reaches PO-09.b15 or PO-09.b16 first.

Transcripts that are `Ok` in this state: no input gives `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500// `; the inputs `""` and `^` (and any longer sequence beginning with them, the rest ignored) give `\n\n\nSelect STATION NUMBER ('^' TO EXIT): 500//       CAMP MASTER\n\n\nP.O./REQ.NO.: `; the single input `""` gives the same transcript, ended by input exhaustion.

## Mapping table

**Rule.** The root contract `EN3^PRCHRPT` maps to `inquire`. Every other contract maps to one private function in `src/inquiry.rs`, named `tag_routine` in lowercase, `routine` alone for an unlabeled entry, with a leading `%` written `pct_`; its doc comment carries the original `TAG^ROUTINE` and the contract ID. Signatures are in section "Contracts".

| ID | `TAG^ROUTINE` | Rust item | Kind | FileMan fields read (file field) |
|---|---|---|---|---|
| PO-01 | `EN3^PRCHRPT` | `inquire` | Ported | none |
| PO-02 | `ST^PRCHRPT` | `st_prchrpt` | Ported | none |
| PO-03 | `^PRCFSITE` | `prcfsite` | Stub | original only: 411 header, 21 (`AC`), node 0; 200 400 |
| PO-04 | `DUZ^PRCFSITE` | `duz_prcfsite` | Stub | original only: 200 20.2, 20.3, .132 |
| PO-05 | `^DIC` (411) | `dic` | Stub | original only: 411 .01 (`B`, 7.5), .1 (identifier); 4 .01 |
| PO-06 | `EN30^PRCHRPT` | `en30_prchrpt` | Ported | none (uses `X` from PO-08) |
| PO-07 | `PO^PRCHRPT` | `po_prchrpt` | Ported | 442 .02 |
| PO-08 | `EN3^PRCHPAT` | `en3_prchpat` | Ported | 442 .5; 442.3 .02 |
| PO-09 | `IX^DIC` | `ix_dic` | Stub | 442 header, .01 (`C`), .02, .1, 1, 91, .5, index list, identifiers; 442.5 1; 442.3 .01; `B` of 440, 442.5, 445, 440.5, 200, 442.3 |
| PO-10 | `^PRCHDP1` | `prchdp1` | Ported | 442 .01, .02, .5, 5, 5.2, 5.3, 5.4, 5.6, 6, 6.2, 31, 48, 64, 65, 116; 442.11 presence; 411 31, node 3; 411.01 .01-7; 440 .01, 1-4, 4.2, 4.4, 4.6, 5, 5.1, .2, 34, 35; 440.2 .01-6; 2 .01; 5 1; 49 .01; 442.5 .01; 442.3 .01 |
| PO-11 | `FTYP^PRCHFPNT` | `ftyp_prchfpnt` | Ported | 411 .5; 442 31; 411.2 1 |
| PO-12 | `^%ZIS` | `pct_zis` | Stub | none (device files, O-01) |
| PO-13 | `FOB^PRCHDP1` | `fob_prchdp1` | Ported | 442 6.4, 8.2, 8.1, 2, 48; 442.12 .01; 442.4 1 |
| PO-14 | `DIWP^PRCUTL` | `diwp_prcutl` | Ported | none |
| PO-15 | `^DIWP` | `diwp` | Repro | none |
| PO-16 | `%XY^%RCR` | `pct_xy_pct_rcr` | Repro | none |
| PO-17 | `TY^PRCHDP1` | `ty_prchdp1` | Ported | 442 8; 420.8 .01 |
| PO-18 | `DIS^PRCHDP1` | `dis_prchdp1` | Ported | 442 7, 16; 200 .01; 442.06 presence |
| PO-19 | `DT^PRCHDP1` | `dt_prchdp1` | Ported | none |
| PO-20 | `CON^PRCHDP1` | `con_prchdp1` | Ported | 442.01 4 (`AC`) |
| PO-21 | `APP^PRCHDP1` | `app_prchdp1` | Ported | 442 1.4, 1 |
| PO-22 | `DAT^PRCHDP1` | `dat_prchdp1` | Ported | 442 .1 |
| PO-23 | `EST^PRCHDP1` | `est_prchdp1` | Ported | 442 .08 |
| PO-24 | `TOT^PRCHDP1` | `tot_prchdp1` | Ported | 442 91 |
| PO-25 | `^PRCHDP2` | `prchdp2` | Ported | 442 13; 442.01 .01, 15; 442.03, 442.07, 442.15 presence |
| PO-26 | `HDR^PRCHDP2` | `hdr_prchdp2` | Ported | none |
| PO-27 | `ITEM^PRCHDP2` | `item_prchdp2` | Ported | 442.01 .01, 1, 2, 3, 3.1, 3.5, 4, 5, 9, 9.5, 11, 15, 41; 442.05 .01; 442.08 presence; 442.041 .01 (`B`), 2; 442 46, 48; 420.5 .01 |
| PO-28 | `PIPECK^PRCHDP2` | `pipeck_prchdp2` | Ported | 442.05 .01 |
| PO-29 | `EDISTAT^PRCHUTL` | `edistat_prchutl` | Ported | 442.01 12, 13 |
| PO-30 | `ASK^PRCHDP2` | `ask_prchdp2` | Ported | none |
| PO-31 | `COM^PRCHDP2` | `com_prchdp2` | Ported | 442.04 presence |
| PO-32 | `PT^PRCHDP2` | `pt_prchdp2` | Ported | 442.14, 442.07 presence |
| PO-33 | `AMENDS^PRCHDP6` | `amends_prchdp6` | Ported | 442.07 presence |
| PO-34 | `ASK2^PRCHDP2` | `ask2_prchdp2` | Ported | 442.11 presence |
| PO-35 | `EN^PRCHDP4` | `en_prchdp4` | Ported | none |
| PO-36 | `EN2^PRCHDP4` | `en2_prchdp4` | Ported | 442 .01; 442.8 .01 (`B`) |
| PO-37 | `EXDELV^PRCHDP4` | `exdelv_prchdp4` | Ported | none |
| PO-38 | `ASK1^PRCHDP2` | `ask1_prchdp2` | Ported | none |
| PO-39 | `Q^PRCHDP2` | `q_prchdp2` | Ported | none |
| PO-40 | `K^PRCHRPT` | `k_prchrpt` | Ported | none |
| PO-41 | `^%DT` | `pct_dt` | Stub | none |

**Built-ins** (`src/mumps.rs`, `pub(crate)`) map to M commands and functions, not tags: `READ` to `Session::read`; `WRITE !` to `Session::nl`; `WRITE ?n` to `Session::tab`; `$PIECE` `piece`; `$EXTRACT` `extract`; `$GET` `get`; `$DATA` `data`; `$ORDER` `order`; `$JUSTIFY` `justify`; unary `+` `num`; `\` `int_div`; `#` `modulo`; `]` `follows`; `[` `contains`.

**Rust-only infrastructure, with no MUMPS counterpart:** `Store`, `Sub`, `StoreError`, `load_fixtures`, `Store::format_ref`, `Locals`, `Halt`, `InquiryOutput`, `InquiryError`, `STUB_CONTEXT`; everything in `src/bridge.rs` and `src/bin/bridge.rs`; every test file.

**Current-to-Target, system level.** Each part of the original system and the crate component that takes its place:

| Current (MUMPS on YottaDB) | Target (Rust crate) |
|---|---|
| OPTION 1901 action `EN3^PRCHRPT`; routines `PRCHRPT`, `PRCHPAT`, `PRCHDP1`, `PRCHDP2`, `PRCHDP4`, `PRCHDP6`, `PRCHUTL`, `PRCHFPNT`, `PRCUTL` | `inquiry::inquire` (PO-01) plus one private function per contract in `src/inquiry.rs`, as in the table above |
| FileMan globals `^PRC(442)`, `^PRCD(442.3)`, `^PRC(440)` and others, as ZWR | `store::Store` (`src/store.rs`), a `BTreeMap<Vec<Sub>, String>` keyed by subscript sequence, the global name first, with numeric-before-string collation: `Sub::Num` before `Sub::Str` (section "Collation rule"). `store::load_fixtures` loads every `*.zwr` file of `tests/fixtures/ifcap/`, parsing each with `Store::from_zwr`: the eleven repository exports, then the capture owner's `cases.zwr` last when it is present. The inquiry reads it only through `&Store` |
| Device I/O (`R X:DTIME`, `W !`, `W ?n`, `$C(7)`) | `mumps::Session` (`src/mumps.rs`): input cursor, transcript, `$X`/`$Y` tracking, through `Session::read`, `Session::nl`, `Session::tab` and `Session::bell` |
| `^PRCFSITE`, `^DIC`, `IX^DIC`, `^%ZIS`, `^DIWP`, `%XY^%RCR` | Stub or reproduction functions in `src/inquiry.rs`: Stub `prcfsite`, `dic`, `ix_dic`, `pct_zis`; Repro `diwp`, `pct_xy_pct_rcr` (sections "Stubs" and "Flagged hard-MUMPS features") |
| Terminal session | `bridge` HTTP endpoints on `127.0.0.1:8080` and the front-end screen. `main` in `src/bin/bridge.rs` binds `127.0.0.1:8080` and calls `serve` in `src/bridge.rs`, which loads the fixture `Store` once and runs `serve_until`. `POST /api/{backend}/session`, with `{backend}` one of `mumps`, `rust` or `compare` and the body `{"inputs": [string, …]}`, runs one input sequence; `GET` on exactly `/` or `/index.html`, `/style.css` and `/app.js` serves the one-screen front-end, `frontend/index.html`, `frontend/style.css` and `frontend/app.js` respectively |

These rows map capabilities, not contracts: `Store` and the bridge stay the Rust-only infrastructure listed above, and neither is given an invented `TAG^ROUTINE` counterpart.

## Collation rule

A `Store` key is `Vec<Sub>` with the global name first. `Sub::Num` orders before `Sub::Str`; numbers compare by exact decimal value, strings by bytes. A subscript that is a canonical number (no leading zero, no trailing fraction zero, no leading `+`) is a number, whether or not it was quoted. Source relies on this order: the 411 export lists its numeric IENs (lines 4-288) before the string subscripts `"A IFCAP-Wide Parameters"` (289), `"AC"` (290), `"AE"` (291) and `"B"` (292-297) [411+ADMIN. ACTIVITY SITE PARAMETER.zwr:4-297]; the item loop `$O(^PRC(442,D0,2,PRCHDI)) Q:PRCHDI'>0` stops at the first string subscript because numeric items precede it [PRCHDP2.m:11]; the lookup's `$O(^PRC(442,"A["))` test [DICM.m:71] and the later-index walk from `"C"` [DIC3.m:63] depend on byte order of strings.

## Port rules

- **Coercions, only those exercised** (`src/mumps.rs`): unary `+` takes the leading numeric prefix [PRCHDP1.m:15] [PRCHDP2.m:49] [PRCHPAT.m:83], with no exponent form (FileMan guards it with `$P(X,"E")` [DIC.m:48]); numeric comparison of strings, `X<10` [PRCHRPT.m:73] and the screen [PRCHRPT.m:9] [PRCHPAT.m:81], where `""` is 0; string equality `S:X=0` [PRCHPAT.m:83]; `\` and `#` on FileMan dates, strictly left to right [PRCHDP1.m:69]; `$L` of the fraction digits [PRCHDP2.m:52]; `]` follows [PRCHDP1.m:8]; `[` contains [PRCHDP2.m:10].
- **Currency:** `justify` works on decimal strings with integer digit arithmetic (section "Amounts").
- **READ and WRITE:** `W !` appends `\n`, sets `$X=0` and adds 1 to `$Y`; `W ?n` pads with spaces to column `n` when `$X<n` and does nothing otherwise; `$C(7)` appends byte 0x07 and counts as one character of `$X` (no exercised `W ?n` follows a BEL on the same line, so a device's own counting is never observed); a READ takes the next input line without echoing it and without moving `$X` or `$Y`; with no line left `Session::read` returns `None`, the calling contract returns `Halt::InputExhausted`, and `inquire` turns that into `Ok` with the transcript so far. Lines left over after `EN3^PRCHRPT` returns are ignored. No exercised `W ?n` or `$X` test follows a READ before the next `W !`, so how a device moves `$X` on a READ never reaches the transcript.
- **Hard features:** a feature whose operand is literal source or DD code is reproduced as fixed logic; a feature only on an n/e branch is not converted; a feature whose operand depends on runtime state blocks its branch (section "Flagged hard-MUMPS features"). No interpreter or parser is built.
- **Transcript versus `Err`:** validation and no-match responses are transcript under `Ok`; `CorruptStore` only for an unguarded read of a missing node on an exercised path (section "Contracts"); `UnsupportedBranch` only for an n/e or BLOCKED branch. This rule governs any conflicting statement.
- **State:** globals are only read, through `&Store`; MUMPS locals shared between tags, including the word-wrap scratch `^UTILITY($J,"W")` and `^TMP($J,"W")`, are fields of the private `Locals`, and `KILL` resets them.
- **Stub constants:** `STUB_CONTEXT` and the `prcfsite`/`dic` output live in `src/inquiry.rs` with the values of section "Stubs"; no capture value overrides them.
- **Safety:** `#![forbid(unsafe_code)]` in `src/lib.rs`; no FFI; no `unwrap`, `expect`, `panic!` or slice indexing anywhere in `src/`, unit tests included; no test sets an environment variable.

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

## Unexercised and BLOCKED branches

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
| PO-09.b15 | [DIC0.m:22] [DIC1.m:41-49] | `^PRC(442,0)` absent, or its piece 2 not `442I`: no prompt, or other identifiers | not exercised, not implemented |
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

## Scope statement

- **Covered:** one read-only VistA workflow, OPTION `PRCH DISPLAY PO` (IEN 1901, action `EN3^PRCHRPT`), as it runs on `worldvista/vehu:202609` from the repository at commit `dd3fd0f3124c8178657d9fff4173c324da30f7ae`, with the fixed context `DUZ` `.5` and station 500.
- **Not claimed:** no VA-wide equivalence; equivalence with any other image, snapshot, site, user, device or option; or any behaviour the five cases do not exercise. The five cases (GC-01 to GC-05) show only their exercised paths.
- **Read-only:** the port has no write path. `PRCHQ1` (OPTION 10295, New RFQ, `EN2^PRCHQ2`) was named in the request but creates RFQs, so `PRCH DISPLAY PO` was substituted. The original's own non-business writes (`^DISV`, word-wrap and delivery scratch under `^UTILITY` and `^TMP`) are documented and not reproduced.
- **Out of scope:** every write path (creating, editing, receiving, amending or cancelling orders); every other IFCAP function and option; VistA GUI clients, terminal devices and terminal emulators; a Java port; every other VistA package; the earlier GMV vitals slice; and the request's "must not be built" list: a write-guard framework, a CLI or exit-code scheme, git or provenance modules, deterministic-report machinery, Windows or macOS branches, an async runtime, any network service other than the loopback bridge, FFI or C bindings, procedural macros, a second crate or workspace, more than two integration test files, a front-end framework or build tooling, authentication or TLS.
- **Unexercised branches:** every branch marked "not exercised, not implemented", and every BLOCKED branch, returns `UnsupportedBranch` with its branch ID and writes no further output.

## Exit input

The exit input is `^`. At the order prompt it ends the lookup with no output [DIC11.m:23] [DIC.m:45]; `EN3^PRCHPAT` then quits on `+Y<0` [PRCHPAT.m:83] and `EN30` quits because `PRCHPO` is undefined [PRCHRPT.m:72], so `EN3^PRCHRPT` returns. Any other value beginning with `^` does the same [DIC11.m:33]. `inquire` has no special case for it: `ix_dic` follows PO-09.b3. The front-end sends it as the constant `EXIT_INPUT = "^"` in `frontend/app.js`, an ordinary input; `tests/golden/ifcap/README.md` states it.

## Source corrections and interface departures

Each is reflected identically in this document, the implementation and the tests, and listed in `REPORT.md`.

- **C9, input sequences.** The request's two-line `input.txt` (identifier, then exit) does not end a found-order session cleanly: every session first answers the station prompt [PRCFSITE.m:13-18]; a found order pauses at `ENTER '^' TO HALT: ` before its items [PRCHDP2.m:10] and at `END OF DISPLAY…` after them [PRCHDP2.m:41]; the option then re-prompts [PRCHRPT.m:75]; `^` exits [DIC11.m:23]. Source governs, one input per line:
  - GC-01, GC-02, GC-05: empty line (station default), the order's `C` value, empty line (`ENTER '^' TO HALT`), empty line (`END OF DISPLAY`), `^`.
  - GC-03: empty line, a syntactically valid `C` value absent from the snapshot that contains at least one letter (the `C` part must match `3UN3N` [DD.zwr:514312]), `^`.
  - GC-04: empty line, `INVALID!`, `^`.
  - If the original pages (O-01), one more empty line per extra `ENTER '^' TO HALT: ` prompt. The exit stays `^`.
- **Order identifier.** The text typed is the common-numbering part after `500-`, the `C` value [DD.zwr:514318-514320] [PRCHPAT.m:80]. `meta.txt` `record_ids` records both the IEN and the full PO number.
- **Context keys.** `{"DUZ":".5","IOF":…,"IOSL":…,"STATION":"500"}`; `IOF` and `IOSL` are added because the display reads them (section "Stubs").
- **Station echo.** `      CAMP MASTER`, not the plan's predicted `  500  CAMP MASTER` (R1).
- **No-match re-ask.** `<BEL> ??\n\nP.O./REQ.NO.: `: two newlines, not one (R3).
- **No IEN attempt at the order prompt.** PO-09.b7 is unreachable (R5 step 4); a numeric value is searched like any other.
- **PO-09.b12 unreachable; PO-09.b15 and PO-09.b16 appended** (R4, R5).
- **`^`-prefixed values** exit like `^` and are reported as PO-09.b3 (R5 step 3).
- **`^XUTL("XQ",$J)`** is read, not written, on this path [ZIS.m:28, 82-86].
- **Pointer-target supplement.** The plan's 29 files and subfiles omit 445 `^PRCP(445,` and 440.5 `^PRC(440.5,`, the targets of 442 indexes `G` and `M`, whose `B` index, zero node and `-9` node only the pointer-candidate rule reads (R6). Section "FileMan definitions read" adds them as a source-correction supplement [1+FILE.zwr:31965, 32562] [DD.zwr:511963-511967, 525062-525066].
- **`PRCHQ1` substitution** by `PRCH DISPLAY PO` (section "Workflow selection").
- **C12, public helpers.** "No other public operation exists" governs inquiry operations only. Capture-format and loader helpers that two components must apply identically are public: `split_input_lines`, `parse_normalisation`, `normalise`, `parse_meta`, `Meta`, `validate_context` in `src/bridge.rs`; `load_fixtures` and `Store::format_ref` in `src/store.rs`.
- **C17, `MumpsMode::Live { container: String, golden_dir: PathBuf }`.** Live mode must read `meta.txt` from the capture directories, `tests/bridge.rs` must cover that with synthetic directories, and no test may set an environment variable, so the directory reaches the bridge only as an argument.
- **Return value.** Every case returns `None`, so every `returned.txt` is zero bytes.
- **Predicted-path fixture profile** (stated in `tests/golden/ifcap/README.md`; each item keeps a case off the n/e branch named):
  - `^PRC(442,0)` present with piece 2 `442I`, and at least one `^PRC(442)` subscript after `"A["` (PO-09.b15, b16).
  - The order passes the screen: station 500, and MOP below 10 or equal to 25. No other order shares its `C` value or has a `C` value beginning with it, and no later index holds a subscript beginning with it (PO-09.b9, b10).
  - Nodes 0 and 1 present (PO-10.b1 to b3); no node 11 (PO-10.b5, PO-34.b2); the vendor is not EDI, or field 116 is `n` (PO-10.b6); field 65 not `RMPR` (PO-10.b7); node 23 piece 7 empty (PO-11.b2); field .08 not `Y` (PO-23.b2).
  - No node 5 (PO-18.b2), 3 (PO-25.b5), 15 (PO-25.b7), 4 (PO-31.b2), 13 (PO-32.b2) or 6 (PO-32.b3, PO-33.b2, PO-38.b1).
  - At least one line item (PO-25.b3); every item has a zero node and a node 2 whose EDI status codes are empty (PO-29.b1, b3); description text that wraps to one line at width 33 under the PO-15 rules, so at most 32 characters in all, with no `|` (PO-15.b2) and no `| IN ` (PO-28.b2) (PO-27.b2, b3; PO-28.b3); no contract number (PO-20.b1); no partial receipts (PO-27.b5).
  - No delivery schedule under the order number in `^PRC(442.8,"B")` (PO-36.b3).
  - Values the profile leaves free are implemented: the MOP-4 and direct-delivery ship-to, the order-type labels, the `SIMPLIFIED` vendor, a verbal or confirmation copy, the status-order message, the estimated-shipping line, and purchase-card item columns.

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

## Conflict register

| ID | Conflict | Decision and reason |
|---|---|---|
| C1 | Toolchain install versus no download | No download: every `rustc`/`cargo` command runs with `RUSTUP_AUTO_INSTALL=0`; a missing or wrong toolchain is BLOCKED, never substituted |
| C2 | Capture directories versus placeholder files | No placeholder is written under `tests/golden/ifcap/GC-0N/`; a missing directory reads as every prerequisite missing (NOT COMPARED) |
| C3 | Where the GC-02 end-to-end test lives | `tests/bridge.rs` (`gc_02_end_to_end_compare`), because the request allows two integration test files and it exercises the bridge |
| C4 | Request's stub starting values versus source | Source-verified values replace them (section "Stubs") |
| C5 | No design system named | No design-system section; the retro look is plain CSS |
| C6 | No user rules | Enterprise-standard practice applies |
| C7 | `UnsupportedBranch` rule versus other statements | The binding rule governs: `UnsupportedBranch` only for n/e or BLOCKED branches |
| C8 | Hard features resolved versus blocked | Literal operands are reproduced; runtime-dependent operands block their branch |
| C9 | Request's input sequences versus source | Source governs (section "Source corrections and interface departures") |
| C10 | Tech-spec citation for `DUZ` | The test code is in `ZOSVGUT1.m` [ZOSVGUT1.m:13]; section 9.1 records only station 500 |
| C11 | Control point | None is needed: no control-point read or prompt on the path |
| C12 | "No other public operation" versus shared helpers | Governs inquiry operations only; the capture-format and loader helpers are public |
| C13 | FileMan contracts | FileMan routines get contracts only at the boundary the slice calls (PO-05, PO-09, PO-15, PO-16, PO-41) |
| C14 | Focus glow versus no shadows | The glow is an `outline` colour transition |
| C15 | Working state versus unchanged label | The working state is visual only; the `SEND` label is unchanged |
| C16 | COMPARE reactions | COMPARE takes MUMPS behaviour: no reactions |
| C17 | `MumpsMode::Live` signature | Gains `golden_dir`, the only way live mode can read `meta.txt` without environment mutation in tests |

## Source-unchanged check

Criterion A7: every file outside `rust/ifcap-po-inquiry/` stays as it is, and every MUMPS routine and ZWR export stays byte for byte. The evidence working directory is the crate folder, and git pathspecs resolve against it, so both commands are anchored at the repository root with `-C ../..`:

- `git -C ../.. diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION` must exit 0.
- `git -C ../.. status --porcelain -- . ':(exclude)rust/ifcap-po-inquiry'` must print nothing.

Run for this document from `rust/ifcap-po-inquiry/` by shell redirection into `test-results/20261108T035854Z/source-unchanged.txt`, block `# A7 (SPEC.md)`, at 2026-11-08T05:07:58Z with `HEAD` `8719e11c7415616878d50994ff1d8e898a36876b`:

| Command | Output | Exit status |
|---|---|---|
| `git -C ../.. diff --quiet HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION` | none | 0 |
| `git -C ../.. status --porcelain -- . ':(exclude)rust/ifcap-po-inquiry'` | none | 0 |
| `git -C ../.. diff --quiet dd3fd0f3124c8178657d9fff4173c324da30f7ae HEAD -- Packages Packages.csv README.rst LICENSE NOTICE ATTESTATION` (supplementary: unchanged since the baseline commit) | none | 0 |

Result: A7 holds at the time of this document. The planning-time run gave the same result (first command exit 0, second printed nothing); that was planning, not run evidence. `REPORT.md`'s evidence run appends an end-of-run A7 block to the same file, and that block decides A7 for the run.

## Front-end checklist

Checked by the team (B3, B4) in current Chrome and Firefox; this run reports each item NOT RUN. Each check records the browser versions, OS, timestamp, case and mode, expected and observed behaviour, screenshot locations, and PASS, FAIL or NOT RUN.

1. **Idle-screen identity:** at rest, MUMPS and RUST screenshots are pixel-identical with the backend selector box masked, per browser; no RUST style applies without hover, focus or a request in flight.
2. **RUST reactions, in RUST mode only**, each behind `[data-mode="rust"]` and `prefers-reduced-motion: no-preference`:
   - a 150 ms focus glow on the input, select and buttons, done as an `outline` colour transition;
   - a hover highlight on `.ln` (inverted green and black);
   - a working state on `SEND` while a request is in flight, visual only, label unchanged;
   - a 300 ms slide-in and highlight on lines whose index was not present in the previous render from the same backend;
   - a 300 ms shake and red outline on a rejection, meaning only a non-2xx response or a failed request, never an HTTP 200.
3. **Reduced motion:** none of the five reactions under `prefers-reduced-motion: reduce`.
4. **MUMPS and COMPARE** redraw instantly, with no transition.
5. **Cases:** GC-01 to GC-05 run in MUMPS, RUST and COMPARE modes; the displayed transcript equals the capture; COMPARE shows `MATCH`, `MISMATCH` or `NOT COMPARED` taken only from `match` (`true`, `false`, `null`).
6. **Labels:** the literal upper-case labels (`MUMPS`, `RUST`, `COMPARE`, `SEND`, `EXIT`) are byte-identical in every mode.
7. **No validation:** no `autofocus`, `pattern`, `required` or `maxlength` attribute; `SEND` appends `input.value` unchanged (the empty string included); `EXIT` appends `EXIT_INPUT = "^"`; and the static search of `frontend/` recorded in `test-results/<run-id>/search-frontend.txt` has zero hits.
8. **Transcript grid:** one `<div class="ln">` per `\n`-separated line, set through `textContent`, in an `80ch` by 24-line box with `white-space: pre`; typed inputs are not echoed; error bodies appear verbatim in a separate status line and the transcript stays as last rendered.
9. **Screen controls:** one text input, `<input type="text">`, with a `SEND` button and an `EXIT` button; in COMPARE, the two transcript areas (MUMPS and RUST) are side by side.
10. **Session behaviour (`app.js`):**
    - `inputs` is an in-memory array, so a reload starts a new session with an empty sequence;
    - `SEND` posts `{"inputs": inputs}`, the whole accumulated sequence, to `/api/{backend}/session`;
    - changing the selector re-sends the current sequence to the newly selected backend;
    - a request counter discards stale responses: a response to an earlier request is never drawn once a later request has been sent;
    - no input is validated, trimmed or transformed, and backend text is never read to decide anything.
11. **Transcript box styling:** `overflow-y: auto`, with no wrapping; each line has a fixed `line-height` with a `min-height` (or its logical equivalent `min-block-size`), so empty lines keep their row.
12. **Retro look, shared by every mode:**
    - the system monospace font (`ui-monospace, monospace`), with no web font, CDN or image dependency;
    - `#33FF33` text on `#000000`;
    - square corners;
    - no `box-shadow`, gradient or image styling;
    - a static block cursor, `<span class="cursor">`, after the input, with no blink animation.
13. **Mode attribute and motion check:** the root element carries `data-mode="mumps|rust|compare"`, following the selected backend; `app.js` checks `matchMedia("(prefers-reduced-motion: reduce)")` before adding animation classes, so none is added under `reduce`.
