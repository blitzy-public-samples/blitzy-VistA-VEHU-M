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
// Whether the latest request is still unanswered.
let pending = false;
// Rows in the previous render, keyed by the pane's backend, so the RUST
// slide-in marks only rows whose index that backend has not shown before.
const previousLineCount = new Map();
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

const root = document.documentElement;
const bridgeStatus = document.getElementById("bridge-status");
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

const BRIDGE_UNREACHABLE = "BRIDGE UNREACHABLE";

// The reaction classes (busy, new, reject) are added only in RUST mode and
// only without reduced motion; style.css gates them the same way again.
function reactionsOn() {
  return root.dataset.mode === "rust" && !reduceMotion.matches;
}

// Every reaction class comes off at once: the working state, the shake and
// every slide-in still running.
function clearReactions() {
  sendButton.classList.remove("busy");
  transcriptEl.classList.remove("reject");
  for (const row of transcriptEl.querySelectorAll(".ln.new")) {
    row.classList.remove("new");
  }
}

// Rendering, step one: one row per "\n"-separated line, text set verbatim
// (BEL and other control bytes included), built into a detached fragment.
// No pane is touched, so every pane of an answer is built before any is shown.
// firstNew is the index of the first row this backend has not shown before.
function prepareRows(container, transcript, key, animate) {
  const lines = transcript.split("\n");
  const fragment = document.createDocumentFragment();
  lines.forEach((text) => {
    const row = document.createElement("div");
    row.className = "ln";
    row.textContent = text;
    fragment.append(row);
  });
  return {
    container: container,
    key: key,
    fragment: fragment,
    count: lines.length,
    animate: animate,
    firstNew: previousLineCount.get(key) ?? 0,
  };
}

// The slide-in. The positional rule is unchanged for every line the box
// displays once it is scrolled to the end: a row is marked when its index is
// firstNew or later. A new line above the box's view cannot be seen during
// the 300 ms, so marking it would only cost one animation and one layer per
// row, thousands of them for a long transcript. Walking back from the last
// row stops at the first row that is old or above the view. Every row is
// measured before any is marked, so layout is computed once.
function markShownNewRows(container, firstNew, count) {
  const top = container.getBoundingClientRect().top + container.clientTop;
  const shown = [];
  let row = container.lastElementChild;
  let index = count - 1;
  while (row !== null && index >= firstNew && row.getBoundingClientRect().bottom > top) {
    shown.push(row);
    row = row.previousElementSibling;
    index -= 1;
  }
  for (const newRow of shown) {
    newRow.classList.add("new");
  }
}

// Rendering, step two: the pane's rows are swapped for the prepared fragment
// in one call, however many lines it holds, so the screen shows exactly what
// the backend returned. A RUST render then marks its shown new rows.
function commitRows(prepared) {
  prepared.container.replaceChildren(prepared.fragment);
  previousLineCount.set(prepared.key, prepared.count);
  prepared.container.scrollTop = prepared.container.scrollHeight;
  if (prepared.animate) {
    markShownNewRows(prepared.container, prepared.firstNew, prepared.count);
  }
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
// so a second rejection in a row replays the animation. The class comes off
// when the shake ends or is stopped early: the transcript's animation
// listener takes it off at the end or the cancel, and the motion-preference
// listener takes it off when reduced motion stops a shake with no event.
function reject() {
  if (!reactionsOn()) {
    return;
  }
  transcriptEl.classList.remove("reject");
  void transcriptEl.offsetWidth;
  transcriptEl.classList.add("reject");
}

// Registered once for the life of the page, for both the end and the cancel
// of every reaction animation in the transcript. A row's slide-in bubbles up
// from the row, which loses its new class; the transcript's own shake loses
// the reject class. The one exception is a cancel while a shake is still
// current: reject() restarts the shake by taking the class off and putting it
// back, which cancels the old shake, and that cancel arrives a frame later,
// after the new shake has started, so it must not end the new one.
function endReaction(event) {
  if (event.target !== transcriptEl) {
    event.target.classList.remove("new");
    return;
  }
  if (
    event.type === "animationcancel" &&
    transcriptEl.getAnimations().some((animation) => animation.animationName === "reject-shake")
  ) {
    return;
  }
  transcriptEl.classList.remove("reject");
}
transcriptEl.addEventListener("animationend", endReaction);
transcriptEl.addEventListener("animationcancel", endReaction);

// Registered once for the life of the page. Under reduced motion the
// stylesheet stops a running reaction without an animationend, or with no
// event at all when the change beats the reaction's first frame, so every
// reaction class is taken off here and nothing replays when motion is
// allowed again. Allowing motion again brings back only the working state of
// a request still in flight.
reduceMotion.addEventListener("change", () => {
  if (reduceMotion.matches) {
    clearReactions();
  } else {
    sendButton.classList.toggle("busy", pending && reactionsOn());
  }
});

// A request that never reached a 2xx answer: the error is shown, the
// transcripts stay as last rendered, and the COMPARE verdict line is emptied,
// since there is no verdict to show.
function showFailure(backend, message) {
  errorStatus.textContent = message;
  if (backend === "compare") {
    matchStatus.textContent = "";
  }
  reject();
}

// The verdict of one COMPARE 200 body, which the bridge writes as
// {"mumps": {...}, "rust": {...}, "match": bool} (compare_session in
// src/bridge.rs). A match that is not a boolean throws, the same as a
// transcript that is not a string, so it is never shown as a MISMATCH.
function verdictOf(body) {
  if (typeof body.match !== "boolean") {
    throw new TypeError("match is not a boolean");
  }
  return body.match ? "MATCH" : "MISMATCH";
}

// A 2xx answer: every pane it names is prepared, and a COMPARE verdict read,
// before any pane is committed, so either every pane and the verdict change
// together or, for a body that cannot be shown, only the error line is set
// and the COMPARE verdict line is emptied.
function showSuccess(backend, text) {
  errorStatus.textContent = "";
  try {
    const body = JSON.parse(text);
    if (backend === "compare") {
      // Only the bridge decides the comparison verdict: the line shows its
      // boolean match field, and transcripts are never compared here.
      const verdict = verdictOf(body);
      const mumpsText = transcriptOf(body.mumps);
      const rustText = transcriptOf(body.rust);
      const mumpsRows = prepareRows(mumpsPane, mumpsText, "compare:mumps", false);
      const rustRows = prepareRows(rustPane, rustText, "compare:rust", false);
      commitRows(mumpsRows);
      commitRows(rustRows);
      matchStatus.textContent = verdict;
    } else {
      commitRows(prepareRows(transcriptEl, transcriptOf(body), backend, reactionsOn()));
    }
  } catch (err) {
    // A 200 is never a rejection, so no shake here, and no verdict stands
    // beside the error line.
    errorStatus.textContent = "MALFORMED RESPONSE: " + text;
    if (backend === "compare") {
      matchStatus.textContent = "";
    }
  }
}

// The error line for a failed request. The rejection value is whatever fetch
// or the body stream rejected with; one that cannot be turned into text is
// reported without its text, so the failure is still shown and nothing
// escapes send().
function failureText(err) {
  try {
    return "REQUEST FAILED: " + String(err);
  } catch {
    return "REQUEST FAILED: UNKNOWN ERROR";
  }
}

// Posts the whole sequence to the backend selected now and shows the answer,
// unless a later request or a backend change has superseded it. It never
// rejects, since every failure is shown on the screen, so its callers fire it
// without waiting or catching.
async function send() {
  const backend = backendSelect.value;
  const seq = ++requestSeq;
  pending = true;
  // The verdict line only ever describes the sequence last answered, so it
  // is empty while a compare is in flight: a 200 sets it again, and an error
  // leaves it empty.
  if (backend === "compare") {
    matchStatus.textContent = "";
  }
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
      showFailure(backend, failureText(err));
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
      showFailure(backend, failureText(err));
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
      pending = false;
      sendButton.classList.remove("busy");
    }
  }
}

// The status line of one GET /api/status body, which the bridge writes as
// {"mumps_mode": "replay" | "live", "recorded": [...], "ready": [...],
// "incomplete": {...}} (status_body in src/bridge.rs): the mode, then how
// many of the five cases are recorded and how many are READY. A body without
// that shape throws, the same as JSON that does not parse.
function statusLine(body) {
  if (
    body === null ||
    typeof body !== "object" ||
    typeof body.mumps_mode !== "string" ||
    !Array.isArray(body.recorded) ||
    !Array.isArray(body.ready)
  ) {
    throw new TypeError("not a status body");
  }
  return (
    "MUMPS BACKEND: " +
    body.mumps_mode.toUpperCase() +
    " RECORDED " +
    body.recorded.length +
    " OF 5 READY " +
    body.ready.length +
    " OF 5"
  );
}

// Fills the status line from GET /api/status (route_api in src/bridge.rs),
// asked once on load. A failed fetch or body read, a non-2xx answer, or a
// body statusLine cannot read shows BRIDGE UNREACHABLE. It is not a
// rejection: no shake, and no other line or request state is touched. It
// never rejects, so it is fired without waiting or catching.
async function showBridgeStatus() {
  try {
    const res = await fetch("/api/status");
    if (!res.ok) {
      bridgeStatus.textContent = BRIDGE_UNREACHABLE;
      return;
    }
    bridgeStatus.textContent = statusLine(JSON.parse(await res.text()));
  } catch {
    bridgeStatus.textContent = BRIDGE_UNREACHABLE;
  }
}

// The typed line goes in unchanged, the empty string included: RETURN
// answers the station prompt and both display pauses
// [Packages/IFCAP/Routines/PRCFSITE.m:18] [Packages/IFCAP/Routines/PRCHDP2.m:10, 41].
// INPUT is then emptied and given focus, so the next answer can be typed at
// once: a mouse click on SEND moves focus to the button, and keystrokes
// there would be lost.
entryForm.addEventListener("submit", (event) => {
  event.preventDefault();
  inputs.push(lineInput.value);
  lineInput.value = "";
  lineInput.focus();
  send();
});

// A mouse double-click on SEND is one answer, not two. The second and later
// clicks of one multi-click (event.detail above 1) are stopped before they
// submit the form, which would add the emptied INPUT as an extra RETURN, and
// focus goes back to INPUT, which that click's press moved to the button. A
// key press, on INPUT or on SEND, clicks with detail 0, and clicks far enough
// apart each count from 1 again, so each of those still sends one answer.
sendButton.addEventListener("click", (event) => {
  if (event.detail > 1) {
    event.preventDefault();
    lineInput.focus();
  }
});

// EXIT adds only the exit input. Any text typed in INPUT is discarded, not
// sent, so it cannot follow the exit input on a later RETURN, and INPUT gets
// focus back from the button. As on SEND, the second and later clicks of one
// mouse multi-click add nothing and only give INPUT focus back: replay
// matches the whole input sequence, so an extra "^" would leave a recorded
// case NOT RECORDED.
exitButton.addEventListener("click", (event) => {
  if (event.detail > 1) {
    lineInput.focus();
    return;
  }
  inputs.push(EXIT_INPUT);
  lineInput.value = "";
  lineInput.focus();
  send();
});

// A new backend never shows another backend's transcript or verdict: any
// request in flight is superseded, every pane and the verdict line are
// emptied, and the current sequence, an empty one included, is always sent
// again to the new backend. The status line is left as it is.
backendSelect.addEventListener("change", () => {
  root.dataset.mode = backendSelect.value;
  requestSeq += 1;
  pending = false;
  clearReactions();
  errorStatus.textContent = "";
  transcriptEl.replaceChildren();
  mumpsPane.replaceChildren();
  rustPane.replaceChildren();
  matchStatus.textContent = "";
  send();
});

// On load the only request is GET /api/status, for the status line. No
// session request is sent, since a reload starts a new session. A later
// selector change does resend the sequence, so at rest with no captures RUST
// shows the station prompt where MUMPS shows nothing: the idle-screen check
// masks the transcript box, selector and status line (SPEC.md conflict K13).
root.dataset.mode = backendSelect.value;
showBridgeStatus();
