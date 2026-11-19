#!/usr/bin/env bash
# check_corpus.sh: offline file checks of a parity-corpus recording, the shell counterpart of the
# integrity rules of golden::load_corpus_run in src/golden.rs. It checks what record_corpus.sh
# wrote under tests/golden/ifcap/corpus/: every case in manifest.tsv has its input.txt and a
# non-empty transcript.txt; no file anywhere under the corpus folder holds a carriage return, and
# the folder holds only folders, each listed in full, and regular files; the run files meta.txt,
# run-summary.txt and SHA256SUMS exist; every non-blank SHA256SUMS line is a SHA-256 digest and a
# path the run may list (BASE.txt, meta.txt, run-summary.txt, or the input.txt or transcript.txt
# of a manifest case), each path listed once, and matches its file's bytes; and every
# transcript.txt anywhere under the corpus folder is listed in SHA256SUMS.
# It is offline: it uses no container and runs no M command.
#
# Usage (from anywhere; it changes to the crate folder rust/ifcap-po-inquiry/ itself and takes no
# arguments):
#   bash tests/golden/ifcap/corpus/scripts/check_corpus.sh
#
# Prerequisites:
#   - a recording by tests/golden/ifcap/corpus/scripts/record_corpus.sh;
#   - for the whole sequence, the snapshot image vehu-snap:gc-base, which load_corpus.sh builds on;
#   - bash (macOS /bin/bash 3.2 or later), dirname, wc, ls, grep and shasum.
#
# The team's order:
#   1. tests/golden/ifcap/corpus/scripts/load_corpus.sh
#   2. tests/golden/ifcap/corpus/scripts/record_corpus.sh
#   3. tests/golden/ifcap/corpus/scripts/check_corpus.sh     (this script)
#   4. cargo test --offline --locked
#
# It reads files only; writes nothing. It opens regular files only, never through a symbolic link
# inside the corpus folder, so no listing can make it read outside that folder. Every check runs
# and each problem is collected, never fatal: after the last check it prints one PROBLEM: line per
# problem, then the result line.
#
# Exit status: 0 with CORPUS CHECK PASS <n> CASES when no problem was found; 1 with
# CORPUS CHECK FAIL <k> PROBLEMS when any was, or with a FAIL: line when the crate folder cannot be
# entered or the script does not sit in the crate's corpus scripts folder; 2 with a usage line on
# standard error when it is given any argument.

set -u
# Pathname expansion is off except while walk_folder lists a folder, so no value read from a file
# is ever expanded as a file name pattern.
set -f

if [ "$#" -ne 0 ]; then
  echo "usage: bash tests/golden/ifcap/corpus/scripts/check_corpus.sh (it takes no arguments and checks the corpus folder of the crate it sits in)" >&2
  exit 2
fi

# Inherited settings that would redirect a relative cd, hide files from the walk or ignore case.
unset CDPATH GLOBIGNORE
shopt -u nocasematch
# Bytes, not characters: the line and field checks then mean the same in every locale, and the
# length of each line read_lines reads is its byte count, which it adds up to compare with wc.
LC_ALL=C

step() { echo; echo "== $*"; }
PROBLEMS=""
K=0
# add_problem TEXT: collects one problem. A newline or carriage return in TEXT, such as one in a
# file name, is shown as \n or \r, so that each problem stays on one line.
add_problem() {
  local text="$*"
  # Unquoted, so that bash 3.2 too removes the quoting of the replacement.
  text=${text//"$NL"/\\n}
  text=${text//"$CR"/\\r}
  PROBLEMS="${PROBLEMS}PROBLEM: $text$NL"
  K=$((K + 1))
}
# step_result: how many problems the current step added, counted from STEP_K.
STEP_K=0
step_result() { echo "problems in this step: $((K - STEP_K))"; }

NL=$'\n'
CR=$'\r'
TAB=$'\t'
# The characters a blank SHA256SUMS line may hold, which golden.rs skips as well.
WS=$' \t\v\f\r'

# read_lines FILE: puts the lines of FILE, each without its newline, into FILE_LINES, indexed from
# 0, and their number into FILE_LINE_COUNT, and returns 0 when those lines hold every byte of
# FILE. A last line with no newline is still a line, and a final newline adds none. Otherwise it
# returns 1, FILE_LINES is not to be used, and READ_FAULT gives the cause: FILE cannot be opened,
# wc fails or gives no count, or the bytes read, counted under LC_ALL=C, differ in number from the
# byte count of wc. The builtin read drops each NUL byte, and ends at a read error as at the end
# of the file, so without the count a file that holds a NUL, that could not be read in full, or
# that changed between the two reads, would be checked as text other than its bytes. It reads in
# this shell with the builtin read alone, with no subshell, pipe or scratch file, in time linear
# in the length of FILE. bash's own message on a failed open or read is still shown, as is wc's.
read_lines() {
  local line="" term=1 bytes=0 st size
  READ_FAULT=""
  FILE_LINES=()
  FILE_LINE_COUNT=0
  while :; do
    if IFS= read -r line; then
      term=1
    elif [ -n "$line" ]; then
      term=0
    else
      break
    fi
    FILE_LINES[FILE_LINE_COUNT]="$line"
    FILE_LINE_COUNT=$((FILE_LINE_COUNT + 1))
    bytes=$((bytes + ${#line} + term))
    [ "$term" -eq 1 ] || break
  done < "$1"
  st=$?
  if [ "$st" -ne 0 ]; then
    READ_FAULT="cannot be read (it cannot be opened)"
    return 1
  fi
  size="$(wc -c < "$1")"
  st=$?
  if [ "$st" -ne 0 ]; then
    READ_FAULT="cannot be read (wc exited $st)"
    return 1
  fi
  # The count alone, without the spaces macOS wc pads it with.
  size="${size//[!0123456789]/}"
  if [ -z "$size" ]; then
    READ_FAULT="cannot be read (wc gave no byte count)"
    return 1
  fi
  if [ "$bytes" != "$size" ]; then
    READ_FAULT="holds a NUL byte, could not be read in full, or changed while it was read (not read)"
    return 1
  fi
  return 0
}

# is_case_id WORD: whether WORD, as a whole, is an id of the three corpus series. Ids become paths,
# so only these pass; the digits are listed so that no locale widens the range.
is_case_id() {
  case "$1" in
    CP-[0123456789][0123456789][0123456789]) return 0 ;;
    CE-[0123456789][0123456789] | CR-[0123456789][0123456789]) return 0 ;;
  esac
  return 1
}

# allowed_path PATH: whether PATH is one a SHA256SUMS line may list, by the rule of golden.rs
# run_member: BASE.txt, meta.txt, run-summary.txt, or <id>/input.txt or <id>/transcript.txt for
# an id in VALID. Any other path, such as an absolute one, one through .. or one starting ./, is
# not.
allowed_path() {
  local id
  case "$1" in
    BASE.txt | meta.txt | run-summary.txt) return 0 ;;
    */input.txt | */transcript.txt) id="${1%/*}" ;;
    *) return 1 ;;
  esac
  is_case_id "$id" || return 1
  case "$VALID " in
    *" $id "*) return 0 ;;
  esac
  return 1
}

# walk VISITOR: runs VISITOR KIND PATH for every entry under the corpus folder C, depth first,
# where KIND is file (a regular file), folder (a folder, walked next when it can be listed) or
# other (a symbolic link, never followed, or a FIFO, device or socket, never opened). For each
# folder, C included, whose entries cannot all be listed, it also runs VISITOR unlisted FOLDER
# CAUSE, where CAUSE names the folder and why. It lists with bash globs in this shell, so a
# visitor's add_problem counts, and checks each listing against ls -A, because a glob gives no
# status when a folder cannot be read. It returns 1, visiting nothing, when C is not a folder it
# can list.
walk() {
  if [ -L "$C" ] || [ ! -d "$C" ] || [ ! -r "$C" ] || [ ! -x "$C" ]; then
    return 1
  fi
  shopt -s nullglob dotglob
  shopt -u failglob
  walk_folder "$1" "$C"
  shopt -u nullglob dotglob
  return 0
}

# walk_folder VISITOR DIR: the walk of one folder that can be listed. The glob gives the entries,
# by their exact names, and is expanded once; pathname expansion is on only while the list of the
# for loop is expanded. ls -A, run just before, prints each name followed by a newline, and the x
# printed after it keeps every newline, so the newlines count the names exactly. A failed ls, or a
# count other than the number of entries the glob gave, means the glob may have missed entries,
# and the folder is reported as unlisted; its entries the glob gave are still visited. A name that
# holds a newline counts more than once, so its folder is reported too.
walk_folder() {
  local visitor="$1" dir="$2" entry listing st names globbed=0
  listing="$(LC_ALL=C ls -A "$dir" 2>/dev/null && printf x)"
  st=$?
  set +f
  for entry in "$dir"/*; do
    set -f
    case "$entry" in
      */. | */..) continue ;;
    esac
    globbed=$((globbed + 1))
    if [ -L "$entry" ]; then
      "$visitor" other "$entry"
    elif [ -d "$entry" ]; then
      "$visitor" folder "$entry"
      if [ -r "$entry" ] && [ -x "$entry" ]; then
        walk_folder "$visitor" "$entry"
      else
        "$visitor" unlisted "$entry" "$entry: folder cannot be listed"
      fi
    elif [ -f "$entry" ]; then
      "$visitor" file "$entry"
    else
      "$visitor" other "$entry"
    fi
  done
  set -f
  if [ "$st" -ne 0 ]; then
    "$visitor" unlisted "$dir" "$dir: folder cannot be listed (ls exited $st)"
    return 0
  fi
  listing="${listing%x}"
  names="${listing//[!$NL]/}"
  if [ "${#names}" -ne "$globbed" ]; then
    "$visitor" unlisted "$dir" "$dir: folder listing incomplete ($globbed entries by glob, ${#names} by ls)"
  fi
}

cd "$(dirname "$0")/../../../../.." || { echo "FAIL: cannot change to the crate folder five levels above $(dirname "$0")"; exit 1; }
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || { echo "FAIL: run from the crate's corpus scripts folder"; exit 1; }

C=tests/golden/ifcap/corpus
MANIFEST="$C/manifest.tsv"

step "1. Manifest"
STEP_K=$K
# VALID holds the valid case ids, space separated, in manifest order; later steps walk it.
VALID=""
N=0
if [ -L "$MANIFEST" ] || { [ -e "$MANIFEST" ] && [ ! -f "$MANIFEST" ]; }; then
  add_problem "$MANIFEST: not a regular file (not read)"
elif [ ! -e "$MANIFEST" ]; then
  add_problem "$MANIFEST: missing"
else
  read_lines "$MANIFEST"
  st=$?
  if [ "$st" -ne 0 ]; then
    # No row of a failed read, or of lines that are not the file's bytes, is checked, so a partial
    # or altered list can never pass.
    add_problem "$MANIFEST: $READ_FAULT"
  else
    # Lines are split on newlines and fields on tabs alone, so each id is checked whole, as
    # written. A last row with no newline is still a row, and a final newline adds none. n is
    # the next line's index and, once raised, its line number.
    n=0
    while [ "$n" -lt "$FILE_LINE_COUNT" ]; do
      line="${FILE_LINES[$n]}"
      n=$((n + 1))
      id="${line%%"$TAB"*}"
      if [ "$n" -eq 1 ]; then
        class=""
        case "$line" in
          *"$TAB"*) class="${line#*"$TAB"}"; class="${class%%"$TAB"*}" ;;
        esac
        if [ "$id" != "case" ] || [ "$class" != "class" ]; then
          add_problem "$MANIFEST: line 1 is not the header case<TAB>class<TAB>..."
        fi
        continue
      fi
      if [ -z "$id" ]; then
        add_problem "$MANIFEST: line $n: no case id"
        continue
      fi
      if ! is_case_id "$id"; then
        add_problem "$MANIFEST: line $n: '$id': not a corpus case id"
        continue
      fi
      # A repeated id is counted and checked once, so that N counts cases rather than rows.
      case "$VALID " in
        *" $id "*) add_problem "$MANIFEST: line $n: $id: listed more than once"; continue ;;
      esac
      VALID="$VALID $id"
      N=$((N + 1))
    done
    # An empty file has no header line either.
    [ "$n" -gt 0 ] || add_problem "$MANIFEST: line 1 is not the header case<TAB>class<TAB>..."
    # A manifest with no case would otherwise pass with nothing checked.
    [ "$N" -gt 0 ] || add_problem "$MANIFEST: lists no case"
  fi
fi
echo "cases: $N (corpus_gen writes 231)"
step_result

step "2. Case files"
STEP_K=$K
for id in $VALID; do
  [ -f "$C/$id/input.txt" ] || add_problem "$C/$id/input.txt: missing"
  test -s "$C/$id/transcript.txt" || add_problem "$C/$id/transcript.txt: missing or empty"
done
step_result

step "3. Carriage returns"
STEP_K=$K
# Every entry under the corpus folder is visited, whatever the manifest lists: each regular file
# is searched, and anything but a folder or a regular file is reported, never followed or opened,
# as is a folder whose entries cannot all be listed.
FILES=0
cr_visit() {
  case "$1" in
    file)
      FILES=$((FILES + 1))
      LC_ALL=C grep -l $'\r' "$2" >/dev/null 2>&1
      case "$?" in
        0) add_problem "$2: carriage return (a pseudo-terminal was used?)" ;;
        1) ;;
        *) add_problem "$2: cannot be read" ;;
      esac
      ;;
    unlisted) add_problem "$3" ;;
    other) add_problem "$2: not a regular file or folder (never followed or read)" ;;
  esac
}
if walk cr_visit; then
  echo "files searched: $FILES"
elif [ -L "$C" ]; then
  add_problem "$C: not a regular file or folder (never followed or read)"
elif [ -d "$C" ]; then
  add_problem "$C: folder cannot be listed"
else
  echo "skipped: no corpus folder (counted in steps 1 and 4)"
fi
step_result

step "4. Run files"
STEP_K=$K
for name in meta.txt run-summary.txt SHA256SUMS; do
  [ -f "$C/$name" ] || add_problem "$C/$name: missing"
done
step_result

step "5. SHA256SUMS verification"
STEP_K=$K
SUMS="$C/SHA256SUMS"
# SUMS_READ becomes 1 once SHA256SUMS is read in full. LISTED then holds, for step 6, the path of
# every line that passed the line rules, each path between newlines.
SUMS_READ=0
LISTED="$NL"
if [ -L "$SUMS" ]; then
  add_problem "$SUMS: not a regular file (not read)"
elif [ ! -f "$SUMS" ]; then
  echo "skipped: no SHA256SUMS (counted in step 4)"
else
  read_lines "$SUMS"
  st=$?
  if [ "$st" -ne 0 ]; then
    add_problem "$SUMS: $READ_FAULT"
  else
    SUMS_READ=1
    # The line rules of golden.rs walk_sha256sums, each line checked here and none left to
    # shasum: a blank line is skipped, and any other is 64 lower-case hex digits, two spaces or
    # a space and *, and a path allowed_path accepts, listed once. Only a line that passes them
    # and names a regular file reached through no symbolic link is handed to shasum, on its
    # standard input, so no listing can make it open a path outside the corpus folder, a FIFO or
    # a device.
    n=0
    lines=0
    passed=0
    OK_LINES=""
    while [ "$n" -lt "$FILE_LINE_COUNT" ]; do
      line="${FILE_LINES[$n]}"
      n=$((n + 1))
      case "$line" in
        *[!"$WS"]*) ;;
        *) continue ;;
      esac
      lines=$((lines + 1))
      digest="${line:0:64}"
      sep="${line:64:2}"
      path="${line:66}"
      shape=1
      [ "${#digest}" -eq 64 ] || shape=0
      case "$digest" in
        *[!0123456789abcdef]*) shape=0 ;;
      esac
      case "$sep" in
        "  " | " *") ;;
        *) shape=0 ;;
      esac
      if [ "$shape" -ne 1 ]; then
        add_problem "$SUMS: line $n: not a SHA-256 digest and path"
        continue
      fi
      if ! allowed_path "$path"; then
        add_problem "$SUMS: line $n: path not allowed"
        continue
      fi
      case "$LISTED" in
        *"$NL$path$NL"*) add_problem "$SUMS: line $n: path listed twice"; continue ;;
      esac
      LISTED="$LISTED$path$NL"
      # The file, and the case folder that holds it, must not be symbolic links, and the file
      # must be a regular one, so that shasum opens nothing else.
      case "$path" in
        */*) holder="$C/${path%/*}" ;;
        *) holder="$C" ;;
      esac
      if [ -L "$holder" ] || [ -L "$C/$path" ] || { [ -e "$C/$path" ] && [ ! -f "$C/$path" ]; }; then
        add_problem "$C/$path: listed in SHA256SUMS but not a regular file (not read)"
      elif [ ! -e "$C/$path" ]; then
        add_problem "$C/$path: listed in SHA256SUMS but missing"
      else
        OK_LINES="$OK_LINES$line$NL"
        passed=$((passed + 1))
      fi
    done
    # A listing with no line would otherwise pass with nothing verified.
    [ "$lines" -gt 0 ] || add_problem "$SUMS: lists no file"
    ok=0
    if [ "$passed" -gt 0 ]; then
      out="$(printf '%s' "$OK_LINES" | (cd -- "$C" && shasum -a 256 -c -) 2>/dev/null)"
      rc=$?
      added=0
      # One output line per line handed over: <path>: OK, or a failure such as <path>: FAILED
      # or <path>: FAILED open or read. Split on newlines only, with no glob expansion (set -f).
      OLD_IFS="$IFS"
      IFS="$NL"
      for line in $out; do
        case "$line" in
          *": OK") ok=$((ok + 1)) ;;
          *) add_problem "SHA256SUMS: $line"; added=$((added + 1)) ;;
        esac
      done
      IFS="$OLD_IFS"
      # Without a failure line, both the exit status and the OK count must confirm every line
      # handed over, so that a line shasum skipped or a shasum that did not run cannot pass.
      if [ "$added" -eq 0 ] && { [ "$rc" -ne 0 ] || [ "$ok" -ne "$passed" ]; }; then
        add_problem "SHA256SUMS: shasum -c exited $rc and confirmed $ok of $passed listed files"
      fi
    fi
    echo "files that match their SHA256SUMS line: $ok"
  fi
fi
step_result

step "6. Every transcript listed"
STEP_K=$K
# Every entry named transcript.txt under the corpus folder, of any kind and in any folder, a
# manifest case's or not, must have its path relative to the corpus folder among the paths of
# the SHA256SUMS lines that passed the line rules of step 5. A folder whose entries cannot all be
# listed may hide one, so it is reported here as well, even when step 3 could list it.
TRANSCRIPTS=0
listed_visit() {
  local rel
  if [ "$1" = unlisted ]; then
    add_problem "$3, so not every transcript in it could be checked"
    return 0
  fi
  case "$2" in
    */transcript.txt) ;;
    *) return 0 ;;
  esac
  TRANSCRIPTS=$((TRANSCRIPTS + 1))
  rel="${2#"$C/"}"
  # A path that holds a newline is never listed, whatever lines its parts match.
  case "$rel" in
    *"$NL"*) ;;
    *)
      case "$LISTED" in
        *"$NL$rel$NL"*) return 0 ;;
      esac
      ;;
  esac
  add_problem "$2: not listed in SHA256SUMS"
}
if [ "$SUMS_READ" -ne 1 ]; then
  if [ -L "$SUMS" ] || [ -f "$SUMS" ]; then
    echo "skipped: SHA256SUMS was not read (counted in step 5)"
  else
    echo "skipped: no SHA256SUMS (counted in step 4)"
  fi
elif walk listed_visit; then
  echo "transcripts found: $TRANSCRIPTS"
else
  echo "skipped: the corpus folder cannot be listed (counted in step 3)"
fi
step_result

step "Result"
printf '%s' "$PROBLEMS"
if [ "$K" -eq 0 ]; then
  echo "CORPUS CHECK PASS $N CASES"
  exit 0
fi
echo "CORPUS CHECK FAIL $K PROBLEMS"
exit 1
