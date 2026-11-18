#!/usr/bin/env python3
"""Three-way comparison: recorded capture, live original (docker exec), Rust API.

Verdicts per case
  PASS         all three equal after the case's normalisation rules
  RUST_DIFF    live original equals the capture, Rust differs: a real parity failure
  ENV_DRIFT    live original differs from the capture: the container is not the recorded
               world (wrong snapshot, other day, changed data). Fix the environment first.
  LIVE_PASS    no transcript.txt in the folder (an ad hoc input): live original equals Rust
  LIVE_DIFF    same, but live original and Rust differ
  ERROR        a side could not be run
Works on Python 3.6+.
"""
import hashlib, json, os, subprocess, sys, urllib.request, urllib.error

CONTAINER = os.environ["VEHU_CONTAINER"]
CMD = os.environ["INQUIRY_CMD"]
RUST = os.environ["RUST_URL"]
CASES = os.environ["CASES_DIR"]
OUT = os.path.join(os.environ["RUN_DIR"], "compare")
os.makedirs(OUT, exist_ok=True)


def sha(b):
    return hashlib.sha256(b).hexdigest()[:8]


def read(path):
    with open(path, "rb") as f:
        return f.read()


def inputs_of(raw):
    # Same convention as golden.rs split_input_lines: drop one trailing LF, split on LF.
    text = raw.decode("utf-8")
    if text == "":
        return []
    if text.endswith("\n"):
        text = text[:-1]
    return text.split("\n")


def rules_of(path):
    rules = []
    if os.path.exists(path):
        for line in read(path).decode("utf-8").split("\n"):
            if line:
                a, b = line.split("\t")
                rules.append((a, b))
    return rules


def normalise(data, rules):
    text = data.decode("utf-8", "surrogateescape")
    for op, cl in rules:
        out, pos = [], 0
        while True:
            i = text.find(op, pos)
            if i < 0:
                break
            j = text.find(cl, i + len(op))
            if j < 0:
                break
            out.append(text[pos:i]); out.append("<NORM>")
            pos = j + len(cl)
        out.append(text[pos:]); text = "".join(out)
    return text.encode("utf-8", "surrogateescape")


def run_original(raw_input):
    p = subprocess.run(["docker", "exec", "-i", CONTAINER, "su", "-", "vehu", "-c", CMD],
                       input=raw_input, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180)
    return p.stdout


def run_rust(inputs):
    body = json.dumps({"inputs": inputs}).encode("utf-8")
    req = urllib.request.Request(RUST + "/api/rust/session", data=body,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            doc = json.loads(r.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        raise RuntimeError("bridge HTTP %s: %s" % (e.code, e.read()[:200]))
    if "transcript" not in doc:
        raise RuntimeError("bridge answered without a transcript: %r" % (doc,))
    return doc["transcript"].encode("utf-8")


def first_diff(a, b):
    n = min(len(a), len(b))
    for i in range(n):
        if a[i] != b[i]:
            line = a[:i].count(b"\n") + 1
            return "first difference at byte %d (line %d); lengths %d vs %d" % (i, line, len(a), len(b))
    return "one is a prefix of the other; lengths %d vs %d" % (len(a), len(b))


def main():
    if len(sys.argv) > 1:
        dirs = [os.path.abspath(a) for a in sys.argv[1:]]
    else:
        dirs = sorted(os.path.join(CASES, d) for d in os.listdir(CASES)
                      if os.path.isfile(os.path.join(CASES, d, "input.txt")))
    if not dirs:
        print("no case folders found under", CASES); return 2
    rows, failed = [], 0
    for d in dirs:
        cid = os.path.basename(d.rstrip("/"))
        try:
            raw = read(os.path.join(d, "input.txt"))
            rules = rules_of(os.path.join(d, "normalisation.txt"))
            cpath = os.path.join(d, "transcript.txt")
            cap = normalise(read(cpath), rules) if os.path.exists(cpath) else None
            org = normalise(run_original(raw), rules)
            rus = normalise(run_rust(inputs_of(raw)), rules)
        except Exception as e:  # noqa
            rows.append((cid, "-", "-", "-", "ERROR", str(e))); failed += 1; continue
        cdir = os.path.join(OUT, cid); os.makedirs(cdir, exist_ok=True)
        for name, data in (("capture.txt", cap), ("original_live.txt", org), ("rust.txt", rus)):
            if data is None:
                continue
            with open(os.path.join(cdir, name), "wb") as f:
                f.write(data)
        if cap is None:
            if org == rus:
                verdict, note = "LIVE_PASS", "no capture on file; original and Rust agree"
            else:
                verdict, note = "LIVE_DIFF", "original vs rust: " + first_diff(org, rus)
            cap = b""
        elif org == cap and rus == cap:
            verdict, note = "PASS", ""
        elif org != cap:
            verdict, note = "ENV_DRIFT", "live original vs capture: " + first_diff(cap, org)
        else:
            verdict, note = "RUST_DIFF", "rust vs capture: " + first_diff(cap, rus)
        if verdict not in ("PASS", "LIVE_PASS"):
            failed += 1
        rows.append((cid, sha(cap) if cap else "-", sha(org), sha(rus), verdict, note))
    lines = ["%-10s %-9s %-9s %-9s %s" % ("CASE", "CAPTURE", "ORIGINAL", "RUST", "VERDICT")]
    for r in rows:
        lines.append("%-10s %-9s %-9s %-9s %s" % r[:5])
        if r[5]:
            lines.append("           " + r[5])
    lines.append("")
    lines.append("%d of %d PASS" % (len(rows) - failed, len(rows)))
    report = "\n".join(lines)
    print(report)
    with open(os.path.join(OUT, "report.txt"), "w") as f:
        f.write(report + "\n")
    with open(os.path.join(OUT, "summary.json"), "w") as f:
        json.dump([dict(zip(("case", "capture", "original", "rust", "verdict", "note"), r)) for r in rows], f, indent=1)
    print("\nFiles per case: %s/<case>/{capture,original_live,rust}.txt  (diff -u them to see a failure)" % OUT)
    return 0 if failed == 0 else 1


sys.exit(main())
