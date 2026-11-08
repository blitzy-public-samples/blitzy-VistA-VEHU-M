"use strict";

// The one exit input: the VistA up-arrow, which ends the FileMan lookup at
// the order prompt [Packages/VA FileMan/Routines/DIC11.m:23]. EXIT sends it
// as an ordinary input.
const EXIT_INPUT = "^";

// The session is this array and nothing else: a reload starts a new one.
// Every request carries the whole sequence, because the bridge keeps no
// session state.
const inputs = [];
// Every request takes the next number; a response whose number is no longer
// the latest is stale and is dropped without touching the screen.
let requestSeq = 0;
// Rows in the previous render, keyed by the pane's backend, so the RUST
// slide-in marks only rows whose index that backend has not shown before.
const previousLineCount = new Map();
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

const root = document.documentElement;
const backendSelect = document.getElementById("backend");
const transcriptEl = document.getElementById("transcript");
const mumpsPane = document.getElementById("transcript-mumps");
const rustPane = document.getElementById("transcript-rust");
const matchStatus = document.getElementById("match-status");
const entryForm = document.getElementById("entry");
const lineInput = document.getElementById("line");
const sendButton = document.getElementById("send");
const exitButton = document.getElementById("exit");
const errorStatus = document.getElementById("error-status");

const NOT_COMPARED = "NOT COMPARED";

// The reaction classes (busy, new, reject) are added only in RUST mode and
// only without reduced motion; style.css gates them the same way again.
function reactionsOn() {
  return root.dataset.mode === "rust" && !reduceMotion.matches;
}

// Rendering, step one: one row per "\n"-separated line, text set verbatim
// (BEL and other control bytes included), built into a detached fragment.
// No pane is touched, so every pane of an answer is built before any is shown.
function prepareRows(container, transcript, key, animate) {
  const lines = transcript.split("\n");
  const prev = previousLineCount.get(key) ?? 0;
  const fragment = document.createDocumentFragment();
  lines.forEach((text, i) => {
    const row = document.createElement("div");
    row.className = "ln";
    row.textContent = text;
    if (animate && i >= prev) {
      row.classList.add("new");
      row.addEventListener("animationend", () => row.classList.remove("new"), { once: true });
    }
    fragment.append(row);
  });
  return { container: container, key: key, fragment: fragment, count: lines.length };
}

// Rendering, step two: the pane's rows are swapped for the prepared fragment
// in one call, however many lines it holds, so the screen shows exactly what
// the backend returned.
function commitRows(prepared) {
  prepared.container.replaceChildren(prepared.fragment);
  previousLineCount.set(prepared.key, prepared.count);
  prepared.container.scrollTop = prepared.container.scrollHeight;
}

// The transcript of one 200 body side. Anything else throws before any pane
// is redrawn, so a malformed body leaves every transcript as last rendered.
function transcriptOf(side) {
  const text = side.transcript;
  if (typeof text !== "string") {
    throw new TypeError("transcript is not a string");
  }
  return text;
}

// The shake and red outline. The class is removed and a reflow forced first,
// so a second rejection in a row replays the animation. One shared listener
// ends every shake, so a shake cancelled before it finishes leaves nothing
// behind.
function reject() {
  if (!reactionsOn()) {
    return;
  }
  transcriptEl.classList.remove("reject");
  void transcriptEl.offsetWidth;
  transcriptEl.classList.add("reject");
}

// Registered once for the life of the page. Row slide-ins bubble their
// animationend up to the transcript, so only the transcript's own shake
// ending removes the class.
transcriptEl.addEventListener("animationend", (event) => {
  if (event.target === transcriptEl) {
    transcriptEl.classList.remove("reject");
  }
});

// A request that never reached a 2xx answer: the error is shown, the
// transcripts stay as last rendered, and COMPARE has no result to show.
function showFailure(backend, message) {
  errorStatus.textContent = message;
  if (backend === "compare") {
    matchStatus.textContent = NOT_COMPARED;
  }
  reject();
}

// A 2xx answer: every pane it names is prepared before any is committed, so
// either every pane and the verdict change together or, for a body that
// cannot be shown, only the error line does.
function showSuccess(backend, text) {
  errorStatus.textContent = "";
  try {
    const body = JSON.parse(text);
    if (backend === "compare") {
      const mumpsText = transcriptOf(body.mumps);
      const rustText = transcriptOf(body.rust);
      // Only the bridge decides the comparison verdict: the status line shows
      // its match field, and transcripts are never compared here.
      const verdict = body["match"];
      const mumpsRows = prepareRows(mumpsPane, mumpsText, "compare:mumps", false);
      const rustRows = prepareRows(rustPane, rustText, "compare:rust", false);
      commitRows(mumpsRows);
      commitRows(rustRows);
      matchStatus.textContent =
        verdict === true ? "MATCH" : verdict === false ? "MISMATCH" : NOT_COMPARED;
    } else {
      commitRows(prepareRows(transcriptEl, transcriptOf(body), backend, reactionsOn()));
    }
  } catch (err) {
    // A 200 is never a rejection, so no shake here.
    errorStatus.textContent = "MALFORMED RESPONSE: " + text;
  }
}

// Posts the whole sequence to the backend selected now and shows the answer,
// unless a later request or a backend change has superseded it.
async function send() {
  const backend = backendSelect.value;
  const seq = ++requestSeq;
  if (reactionsOn()) {
    sendButton.classList.add("busy");
  }
  try {
    let res;
    try {
      res = await fetch("/api/" + backend + "/session", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ inputs: inputs }),
      });
    } catch (err) {
      if (seq !== requestSeq) {
        return;
      }
      showFailure(backend, "REQUEST FAILED: " + String(err));
      return;
    }
    if (seq !== requestSeq) {
      return;
    }
    // A body that cannot be read is a failed request, the same as a fetch
    // that throws: no answer reached the screen.
    let text;
    try {
      text = await res.text();
    } catch (err) {
      if (seq !== requestSeq) {
        return;
      }
      showFailure(backend, "REQUEST FAILED: " + String(err));
      return;
    }
    if (seq !== requestSeq) {
      return;
    }
    if (!res.ok) {
      // The bridge's {"error": ...} body, shown verbatim and never read.
      showFailure(backend, text);
      return;
    }
    showSuccess(backend, text);
  } finally {
    if (seq === requestSeq) {
      sendButton.classList.remove("busy");
    }
  }
}

// The typed line goes in unchanged, the empty string included: RETURN
// answers the station prompt and both display pauses
// [Packages/IFCAP/Routines/PRCFSITE.m:18] [Packages/IFCAP/Routines/PRCHDP2.m:10, 41].
entryForm.addEventListener("submit", (event) => {
  event.preventDefault();
  inputs.push(lineInput.value);
  lineInput.value = "";
  send();
});

exitButton.addEventListener("click", () => {
  inputs.push(EXIT_INPUT);
  send();
});

// A new backend never shows another backend's transcript or verdict: any
// request in flight is superseded, every pane is emptied, and the current
// sequence is sent again to the new backend.
backendSelect.addEventListener("change", () => {
  root.dataset.mode = backendSelect.value;
  requestSeq += 1;
  sendButton.classList.remove("busy");
  transcriptEl.classList.remove("reject");
  for (const row of document.querySelectorAll(".ln")) {
    row.classList.remove("new");
  }
  errorStatus.textContent = "";
  transcriptEl.replaceChildren();
  mumpsPane.replaceChildren();
  rustPane.replaceChildren();
  matchStatus.textContent = NOT_COMPARED;
  if (inputs.length) {
    send();
  }
});

// Nothing is sent on load: an empty sequence matches no capture, so MUMPS
// replay would answer 502 while RUST showed the station prompt, and the idle
// MUMPS and RUST screens must differ only in the selector.
root.dataset.mode = backendSelect.value;
