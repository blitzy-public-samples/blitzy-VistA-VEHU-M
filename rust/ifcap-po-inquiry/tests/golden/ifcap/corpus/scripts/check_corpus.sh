#!/usr/bin/env bash
# check_corpus.sh: offline file checks of a parity-corpus recording, the shell counterpart of the
# integrity rules of golden::load_corpus_run in src/golden.rs. It checks what record_corpus.sh
# wrote under tests/golden/ifcap/corpus/, in nine steps:
#   1. tests, tests/golden, tests/golden/ifcap and the corpus folder are each a folder and not a
#      symbolic link, and the corpus folder can be listed; otherwise no later step reads anything;
#   2. manifest.tsv, within 1048576 bytes, has its header, each case id is one of the 231 corpus
#      ids (CP-001 to CP-200, CE-01 to CE-26, CR-01 to CR-05), listed once, and every corpus id is
#      listed;
#   3. every case in manifest.tsv has its input.txt and a non-empty transcript.txt, and each of the
#      two that is a regular file holds at most 1048576 bytes and is UTF-8;
#   4. one walk of the corpus folder, within 4096 entries and 8 folder levels: it holds only
#      folders, each listed in full, and regular files, and no file holds a carriage return; a
#      file is searched only once measured within its budget, 65536 bytes for a run file or
#      BASE.txt and 1048576 for any other, and one over it, or one that cannot be measured, is
#      reported as not searched;
#   5. the run files meta.txt, run-summary.txt and SHA256SUMS exist as regular files;
#   6. meta.txt, within 65536 bytes and UTF-8, holds each of its five keys once and no other key;
#   7. run-summary.txt, within 65536 bytes and UTF-8, holds exports=IDENTICAL, runs=IDENTICAL and
#      an export_sha256= of 64 lower-case hex digits, once each, and one RECORDED or QUARANTINED
#      line for each manifest case and for no other id; a DIFF line names the export or a
#      manifest case, and is a problem beside exports=IDENTICAL and runs=IDENTICAL, since
#      record_corpus.sh writes one only beside a DIFFERENT identity line; any other line is a
#      problem;
#   8. SHA256SUMS, within 65536 bytes and UTF-8: every non-blank line is a SHA-256 digest and a
#      path the run may list (BASE.txt, meta.txt, run-summary.txt, or the input.txt or
#      transcript.txt of a manifest case), each path listed once, and matches its file's bytes; a
#      listed BASE.txt holds at most 65536 bytes and is UTF-8; a listed file is hashed only once
#      a check has measured it within its own budget, and any other is reported as not hashed;
#   9. every transcript.txt the walk of step 4 found is listed in SHA256SUMS.
# A PASS does not cover the corpus reader's input rules (input.txt is not empty, and its last line
# begins with ^) or the equivalence harness's comparison of each committed input.txt with the
# generated model; cargo test --offline --locked applies both.
# It is offline: it uses no container and runs no M command.
#
# Usage (from anywhere; it changes to the crate folder rust/ifcap-po-inquiry/ itself and takes no
# arguments):
#   bash tests/golden/ifcap/corpus/scripts/check_corpus.sh
#
# Prerequisites:
#   - a recording by tests/golden/ifcap/corpus/scripts/record_corpus.sh;
#   - for the whole sequence, the snapshot image vehu-snap:gc-base, which load_corpus.sh builds on;
#   - bash (macOS /bin/bash 3.2 or later), dirname, wc, ls, grep, tr and shasum.
#
# The team's order:
#   1. tests/golden/ifcap/corpus/scripts/load_corpus.sh
#   2. tests/golden/ifcap/corpus/scripts/record_corpus.sh
#   3. tests/golden/ifcap/corpus/scripts/check_corpus.sh     (this script)
#   4. cargo test --offline --locked
#
# It reads files only; writes nothing. It rejects every symbolic link it finds at the paths it
# inspects, at or inside the corpus folder or in the three folders above it, and opens a file only
# after finding it a regular file, not a link, so no listing can make it read outside that folder
# while the tree stays as inspected. These checks, like the measurements below, are not atomic
# with the opens they guard, and no check after a read confirms that the path still holds the file
# first found there, as the corpus reader's does: they assume a stable tree. What it holds in the
# shell is bounded: manifest.tsv by 1048576 bytes,
# meta.txt, run-summary.txt and SHA256SUMS by 65536 each, and the walk by its entry and depth
# budgets; case files are only measured and searched, by wc and grep. What it reads is bounded
# too: wc -c is given each file it measures as an argument, which the wc of macOS, GNU coreutils
# and uutils coreutils each answer from the file's size, not by reading it through; and no file is
# searched or hashed, by grep or shasum, until a measurement has found it within its budget, so a
# file over its budget, or one that cannot be measured, is reported and neither searched nor
# hashed.
# Every check runs, except those step 1 or a missing input leaves nothing to check, and each
# problem is collected, never fatal: after the last check it prints one PROBLEM: line per
# problem, then the result line. Each problem line shows a newline or carriage return as \n or
# \r, and every other byte outside printable ASCII as \xNN.
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
# PROBLEM_LINES holds one PROBLEM: line per problem, indexed from 0, and K counts them. An array
# gains an element in constant time, where a string that grows copies all it holds each time.
PROBLEM_LINES=()
K=0
# ESCAPE_PART: the most bytes of a problem text add_problem escapes at a time. A replacement of
# every match takes time in proportion to the number of matches times the length of the text it
# runs on, so parts of bounded length keep the time per byte bounded, whatever the text holds.
ESCAPE_PART=16384
# add_problem TEXT: collects one problem. A newline or carriage return in TEXT, such as one in a
# file name, is shown as \n or \r, so that each problem stays on one line, and every other byte
# outside printable ASCII (0x20 to 0x7E) as \x and its two lower-case hex digits, so that no
# control byte of a file name or of a line read from a file reaches the terminal. The bytes 0x80
# to 0xFF are all escaped, not only the C1 controls 0x80 to 0x9F: that covers a raw C1 control
# and the UTF-8 form of one (C2 80 to C2 9F, which a UTF-8 terminal obeys) with no UTF-8
# decoding, and corpus ids and file names are ASCII by construction. A text of printable ASCII
# alone is kept as it is. Any other is escaped in parts of at most ESCAPE_PART bytes, one pass
# per distinct byte to escape in each part, and the parts, held in ESCAPED_PARTS, are joined
# once, so the time taken grows in proportion to the length of TEXT.
add_problem() {
  local text="$*"
  case "$text" in
    *[![:print:]]*)
      # Made only here, as most problem texts are printable ASCII and need no escape.
      local part rest before c code len i=0 j=0
      len="${#text}"
      ESCAPED_PARTS=()
      while [ "$i" -lt "$len" ]; do
        part="${text:$i:$ESCAPE_PART}"
        i=$((i + ESCAPE_PART))
        case "$part" in
          *[![:print:]]*)
            # Unquoted, so that bash 3.2 too removes the quoting of the replacement.
            part=${part//"$NL"/\\n}
            part=${part//"$CR"/\\r}
            # rest keeps only the bytes still to escape; each pass escapes every copy of its
            # first byte. The loop stops when a pass removes nothing from rest, so it ends even in
            # a shell that cannot match some byte, and any byte a pass could not match is shown
            # as ?.
            rest="${part//[[:print:]]/}"
            while [ -n "$rest" ]; do
              c="${rest:0:1}"
              printf -v code '%02x' "'$c"
              part=${part//"$c"/\\x$code}
              before="${#rest}"
              rest="${rest//"$c"/}"
              [ "${#rest}" -lt "$before" ] || break
            done
            part=${part//[![:print:]]/?}
            ;;
        esac
        ESCAPED_PARTS[j]="$part"
        j=$((j + 1))
      done
      # TEXT is not empty here, so the array holds a part: bash 3.2 under set -u rejects the
      # expansion of an empty array.
      printf -v text '%s' "${ESCAPED_PARTS[@]}"
      ;;
  esac
  PROBLEM_LINES[K]="PROBLEM: $text"
  K=$((K + 1))
}
# step_result: how many problems the current step added, counted from STEP_K.
STEP_K=0
step_result() { echo "problems in this step: $((K - STEP_K))"; }

NL=$'\n'
CR=$'\r'
TAB=$'\t'
# The characters a blank line of meta.txt, run-summary.txt or SHA256SUMS may hold, which golden.rs
# skips: str::trim removes each character char::is_whitespace accepts, the 25 of Unicode
# White_Space. WS holds the ASCII ones a line can hold, and UNICODE_WS the UTF-8 form of each of
# the other 19: U+0085, U+00A0, U+1680, U+2000 to U+200A, U+2028, U+2029, U+202F, U+205F and
# U+3000. WS_LEADS holds their lead bytes. The bytes are made by $'\xNN' in plain assignments,
# since $'...' is not expanded inside double quotes.
WS=$' \t\v\f\r'
UNICODE_WS=($'\xc2\x85' $'\xc2\xa0' $'\xe1\x9a\x80' $'\xe2\x80\x80' $'\xe2\x80\x81' $'\xe2\x80\x82'
  $'\xe2\x80\x83' $'\xe2\x80\x84' $'\xe2\x80\x85' $'\xe2\x80\x86' $'\xe2\x80\x87' $'\xe2\x80\x88'
  $'\xe2\x80\x89' $'\xe2\x80\x8a' $'\xe2\x80\xa8' $'\xe2\x80\xa9' $'\xe2\x80\xaf' $'\xe2\x81\x9f'
  $'\xe3\x80\x80')
WS_LEADS=$'\xc2\xe1\xe2\xe3'

# Budgets. read_lines reads manifest.tsv within MANIFEST_LIMIT bytes (corpus_gen writes about
# 105000), and meta.txt, run-summary.txt and SHA256SUMS within TEXT_LIMIT, the budget golden.rs
# size_limit gives every run file and BASE.txt. CASE_FILE_LIMIT is the budget it gives each
# input.txt and transcript.txt.
MANIFEST_LIMIT=1048576
TEXT_LIMIT=65536
CASE_FILE_LIMIT=1048576
# SCAN_LIMIT is step 4's budget for the carriage-return search of a file no earlier step measured,
# other than a run file or BASE.txt (TEXT_LIMIT): the largest budget of any corpus file
# (manifest.tsv, input.txt and transcript.txt), so that even a file no reader reads, such as an
# extra one, is searched only within it.
SCAN_LIMIT=1048576
# IN_BUDGET holds the path of every file a check has measured within its own budget, each between
# newlines: manifest.tsv and each run file read_lines read in full, and each case file and
# BASE.txt that check_case_budgets or check_text_budget found within its budget. Step 4 searches
# such a file without measuring it again, and step 8 hashes no other. It holds at most the two
# case files of each manifest case and five more paths.
IN_BUDGET="$NL"
# The walk lists at most ENTRY_BUDGET entries in all (a recording of the 231 cases holds about
# 700) and no folder more than DEPTH_BUDGET levels below the corpus folder (the layout uses one).
ENTRY_BUDGET=4096
DEPTH_BUDGET=8

# utf8_line_pattern: sets UTF8_LINE, an extended regular expression that a line, matched whole
# under LC_ALL=C as bytes, matches exactly when it is UTF-8 as the corpus reader's
# String::from_utf8 accepts it: ASCII bytes, NUL included, and two- to four-byte sequences in
# their shortest form, with no surrogate (ED A0 to ED BF) and nothing above U+10FFFF. An overlong
# form, a C0, C1 or F5 to FF lead byte, a lone continuation byte or a cut sequence does not
# match. The bytes are made by $'\xNN' in plain assignments, since $'...' is not expanded inside
# double quotes.
utf8_line_pattern() {
  local x80=$'\x80' x8f=$'\x8f' x90=$'\x90' x9f=$'\x9f' xa0=$'\xa0' xbf=$'\xbf' xc2=$'\xc2'
  local xdf=$'\xdf' xe0=$'\xe0' xe1=$'\xe1' xec=$'\xec' xed=$'\xed' xee=$'\xee' xef=$'\xef'
  local xf0=$'\xf0' xf1=$'\xf1' xf3=$'\xf3' xf4=$'\xf4' xff=$'\xff' t
  t="[$x80-$xbf]"
  UTF8_LINE="^([^$x80-$xff]|[$xc2-$xdf]$t|$xe0[$xa0-$xbf]$t|[$xe1-$xec$xee$xef]$t$t|$xed[$x80-$x9f]$t|$xf0[$x90-$xbf]$t$t|[$xf1-$xf3]$t$t$t|$xf4[$x80-$x8f]$t$t)*\$"
}
utf8_line_pattern

# read_lines FILE LIMIT: puts the lines of FILE, each without its newline, into FILE_LINES,
# indexed from 0, and their number into FILE_LINE_COUNT, and returns 0 when those lines hold every
# byte of FILE and FILE holds at most LIMIT bytes. A last line with no newline is still a line, and
# a final newline adds none. A shell variable cannot hold a NUL byte, so FILE is read through tr,
# which turns each NUL into the byte \002 and keeps every other byte. Like NUL, \002 is not
# White_Space, a tab, = or a newline, and fits no digest, id, key, status or path, so every line
# rule decides on it as the corpus reader decides on the NUL, which it reads as an ordinary
# character; \001 is not used, as bash quotes with it internally. A problem text shows a NUL as
# \x02, the byte \002 it was read as, escaped by add_problem. Otherwise it returns 1, FILE_LINES
# is not to be used, and READ_FAULT
# gives the cause: wc fails or gives no count; FILE holds more than LIMIT bytes by that count, and
# then nothing of it is read; the output of tr cannot be opened; it grew past LIMIT while it was
# read; or the bytes read, counted under LC_ALL=C, differ in number from the byte count of wc,
# taken again after the read. tr gives one byte for each byte, and its output just ends early, as
# at the end of the file, when FILE cannot be opened or read in full, so without the second count
# a file that could not be read in full, or that changed between the reads, would be checked as
# text other than its bytes. Each read takes at most one byte more than the budget has left, so no
# line, and no number of lines, can hold more than LIMIT bytes in this shell, whatever FILE holds
# or becomes. The loop runs in this shell, fed by the one tr process through a process
# substitution, with no scratch file, in time linear in the length of FILE, which is at most
# LIMIT. The messages of bash on a failed open, of tr and of wc are still shown. When it returns
# 0, FILE is added to IN_BUDGET, as a file measured within its budget.
read_lines() {
  local line="" term=1 bytes=0 st room
  READ_FAULT=""
  FILE_LINES=()
  FILE_LINE_COUNT=0
  file_size "$1"
  st=$?
  if [ "$st" -ne 0 ]; then
    READ_FAULT="$SIZE_FAULT"
    return 1
  fi
  if [ "$SIZE" -gt "$2" ]; then
    READ_FAULT="larger than $2 bytes (not read)"
    return 1
  fi
  while :; do
    room=$(($2 - bytes + 1))
    if IFS= read -r -n "$room" line; then
      term=1
    elif [ -n "$line" ]; then
      term=0
    else
      break
    fi
    # A read that filled its bound stopped before any newline, so the line, and the file, hold
    # more than the budget has left.
    if [ "${#line}" -ge "$room" ]; then
      bytes=$(($2 + 1))
      break
    fi
    FILE_LINES[FILE_LINE_COUNT]="$line"
    FILE_LINE_COUNT=$((FILE_LINE_COUNT + 1))
    bytes=$((bytes + ${#line} + term))
    [ "$term" -eq 1 ] && [ "$bytes" -le "$2" ] || break
  done < <(LC_ALL=C tr '\000' '\002' < "$1")
  st=$?
  if [ "$st" -ne 0 ]; then
    READ_FAULT="cannot be read (the output of tr cannot be opened)"
    return 1
  fi
  if [ "$bytes" -gt "$2" ]; then
    READ_FAULT="larger than $2 bytes (it grew while it was read)"
    return 1
  fi
  file_size "$1"
  st=$?
  if [ "$st" -ne 0 ]; then
    READ_FAULT="$SIZE_FAULT"
    return 1
  fi
  if [ "$bytes" != "$SIZE" ]; then
    READ_FAULT="could not be read in full, or changed while it was read (not read)"
    return 1
  fi
  IN_BUDGET="$IN_BUDGET$1$NL"
  return 0
}

# file_size FILE: puts the byte count wc -c gives for FILE into SIZE and returns 0, or returns 1
# with the cause in SIZE_FAULT when wc fails or gives no count. FILE is given to wc as an
# argument, never on its standard input: the wc of macOS, of GNU coreutils and of uutils
# coreutils then takes the count from the file's size, whereas uutils coreutils wc (Ubuntu
# 25.10's) reads a standard input to its end, which for a sparse file of a terabyte takes minutes.
# FILE always begins with the corpus folder, so wc never takes it for an option, and its message
# on a failed open is wc's.
file_size() {
  local st
  SIZE_FAULT=""
  SIZE="$(LC_ALL=C wc -c "$1")"
  st=$?
  if [ "$st" -ne 0 ]; then
    SIZE_FAULT="cannot be read (wc exited $st)"
    return 1
  fi
  # The count alone: wc prints <count> <FILE>, the count padded with leading spaces on macOS, so
  # the spaces are dropped and the digits before the next byte kept, never one of FILE's.
  SIZE="${SIZE#"${SIZE%%[! ]*}"}"
  SIZE="${SIZE%%[!0123456789]*}"
  if [ -z "$SIZE" ]; then
    SIZE_FAULT="cannot be read (wc gave no byte count)"
    return 1
  fi
  return 0
}

# is_case_id WORD: whether WORD, as a whole, is one of the 231 ids of the series of
# golden::corpus_case_ids(): CP-001 to CP-200, CE-01 to CE-26 and CR-01 to CR-05, the ids
# corpus_case_known in src/golden.rs accepts and no other. Ids become paths, so only these pass;
# the digits are listed so that no locale widens the range.
is_case_id() {
  case "$1" in
    CP-00[123456789] | CP-0[123456789][0123456789]) return 0 ;;
    CP-1[0123456789][0123456789] | CP-200) return 0 ;;
    CE-0[123456789] | CE-1[0123456789] | CE-2[0123456]) return 0 ;;
    CR-0[12345]) return 0 ;;
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

# is_run_file PATH: whether PATH is a regular file that is not a symbolic link, the only kind of
# run file the later steps read.
is_run_file() {
  [ ! -L "$1" ] && [ -f "$1" ]
}

# in_manifest WORD: whether WORD, as a whole, is one of the manifest's valid case ids (VALID).
in_manifest() {
  is_case_id "$1" || return 1
  case "$VALID " in
    *" $1 "*) return 0 ;;
  esac
  return 1
}

# is_blank_line LINE: whether LINE holds only White_Space characters, or nothing, so that golden.rs
# skips it as line.trim().is_empty(). A line of WS bytes alone is blank, and one holding any other
# byte but no byte of WS_LEADS is not, so an ASCII line is decided by two matches. Otherwise the
# line is blank when nothing but WS bytes is left once every UNICODE_WS sequence is removed. In
# UTF-8 no character's bytes occur inside another's, so on a UTF-8 line each removal takes whole
# characters only; a line that is not UTF-8 is reported by the file's UTF-8 check whatever this
# decides.
is_blank_line() {
  local rest="$1" i=0
  case "$rest" in
    *[!"$WS"]*) ;;
    *) return 0 ;;
  esac
  case "$rest" in
    *["$WS_LEADS"]*) ;;
    *) return 1 ;;
  esac
  while [ "$i" -lt "${#UNICODE_WS[@]}" ]; do
    rest=${rest//"${UNICODE_WS[$i]}"/}
    i=$((i + 1))
  done
  case "$rest" in
    *[!"$WS"]*) return 1 ;;
  esac
  return 0
}

# utf8_status FILE: returns 0 when every line of FILE matches UTF8_LINE, 1 when one does not, and
# 2 when grep cannot read FILE. UTF-8 never uses a newline byte inside a character, so a file is
# UTF-8 exactly when each of its lines is.
utf8_status() {
  LC_ALL=C grep -a -q -v -x -E -e "$UTF8_LINE" "$1" 2>/dev/null
  case "$?" in
    0) return 1 ;;
    1) return 0 ;;
  esac
  return 2
}

# check_text_budget FILE LIMIT: the corpus reader's rules for one file it reads whole, FILE being
# a regular file reached through no symbolic link: it holds at most LIMIT bytes and, within
# them, is UTF-8. A file over budget is not searched, as the reader stops at its budget. A file
# within it is added to IN_BUDGET, whether or not it is UTF-8.
check_text_budget() {
  if ! file_size "$1"; then
    add_problem "$1: $SIZE_FAULT"
    return 0
  fi
  if [ "$SIZE" -gt "$2" ]; then
    add_problem "$1: larger than $2 bytes"
    return 0
  fi
  IN_BUDGET="$IN_BUDGET$1$NL"
  utf8_status "$1"
  case "$?" in
    1) add_problem "$1: not UTF-8" ;;
    2) add_problem "$1: cannot be read" ;;
  esac
}

# in_budget PATH: whether PATH is in IN_BUDGET, a file a check has measured within its own
# budget. A path that holds a newline never is, whatever lines its parts match.
in_budget() {
  case "$1" in
    *"$NL"*) return 1 ;;
  esac
  case "$IN_BUDGET" in
    *"$NL$1$NL"*) return 0 ;;
  esac
  return 1
}

# walk VISITOR: runs VISITOR KIND PATH for every entry under the corpus folder C, depth first,
# where KIND is file (a regular file), folder (a folder, walked next when it can be listed) or
# other (a symbolic link, never followed, or a FIFO, device or socket, never opened). For each
# folder, C included, whose entries cannot all be listed, it also runs VISITOR unlisted FOLDER
# CAUSE, where CAUSE names the folder and why. It lists with bash globs in this shell, so a
# visitor's add_problem counts, and checks each listing against ls -A, because a glob gives no
# status when a folder cannot be read. The walk is bounded: it lists at most ENTRY_BUDGET entries
# in all and no folder more than DEPTH_BUDGET levels below C, and a folder past either bound is
# reported as unlisted and not listed. It returns 1, visiting nothing, when C is not a folder it
# can list.
walk() {
  if [ -L "$C" ] || [ ! -d "$C" ] || [ ! -r "$C" ] || [ ! -x "$C" ]; then
    return 1
  fi
  ENTRIES_LEFT=$ENTRY_BUDGET
  shopt -s nullglob dotglob
  shopt -u failglob
  walk_folder "$1" "$C" 0
  shopt -u nullglob dotglob
  return 0
}

# walk_folder VISITOR DIR DEPTH: the walk of one folder that can be listed, DEPTH levels below C.
# ls -A first counts its entries: it prints each name followed by a newline, and wc -l counts the
# newlines, so no listing is held in this shell. A folder deeper than DEPTH_BUDGET, a failed ls,
# or more entries than are left of ENTRY_BUDGET, is reported as unlisted and not globbed.
# Otherwise the glob gives the entries, by their exact names, and is expanded once; pathname
# expansion is on only while the list of the for loop is expanded. A glob count other than the ls
# count means the glob may have missed entries, or the folder changed, and the folder is reported
# as unlisted; of its entries no more than ls counted are visited. A name that holds a newline
# counts more than once, so its folder is reported too.
walk_folder() {
  local visitor="$1" dir="$2" depth="$3" entry count st globbed=0
  if [ "$depth" -gt "$DEPTH_BUDGET" ]; then
    "$visitor" unlisted "$dir" "$dir: folder not listed (more than $DEPTH_BUDGET folder levels below $C, the depth budget)"
    return 0
  fi
  count="$(set -o pipefail; LC_ALL=C ls -A "$dir" 2>/dev/null | LC_ALL=C wc -l)"
  st=$?
  # The count alone, without the spaces macOS wc pads it with.
  count="${count//[!0123456789]/}"
  if [ "$st" -ne 0 ] || [ -z "$count" ]; then
    "$visitor" unlisted "$dir" "$dir: folder cannot be listed (ls exited $st)"
    return 0
  fi
  if [ "$count" -gt "$ENTRIES_LEFT" ]; then
    "$visitor" unlisted "$dir" "$dir: folder not listed (its $count entries exceed the $ENTRIES_LEFT left of the $ENTRY_BUDGET-entry budget)"
    return 0
  fi
  ENTRIES_LEFT=$((ENTRIES_LEFT - count))
  set +f
  for entry in "$dir"/*; do
    set -f
    case "$entry" in
      */. | */..) continue ;;
    esac
    globbed=$((globbed + 1))
    [ "$globbed" -le "$count" ] || break
    if [ -L "$entry" ]; then
      "$visitor" other "$entry"
    elif [ -d "$entry" ]; then
      "$visitor" folder "$entry"
      if [ -r "$entry" ] && [ -x "$entry" ]; then
        walk_folder "$visitor" "$entry" "$((depth + 1))"
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
  if [ "$globbed" -gt "$count" ]; then
    "$visitor" unlisted "$dir" "$dir: folder listing incomplete (more entries by glob than the $count by ls)"
  elif [ "$globbed" -ne "$count" ]; then
    "$visitor" unlisted "$dir" "$dir: folder listing incomplete ($globbed entries by glob, $count by ls)"
  fi
}

# check_root: step 1, run before anything under the corpus folder is read. Each of tests,
# tests/golden, tests/golden/ifcap and the corpus folder C, in that order, must be a folder that
# is not a symbolic link, and C must be readable and searchable. The first that is not is
# reported, and none beneath it is looked at: a symbolic link found here is never followed, so no
# later step can read outside the crate's corpus folder unless the tree changes after these
# checks, which are not atomic with later opens. ROOT_OK becomes 1 when all hold and 0 otherwise.
check_root() {
  local folder
  ROOT_OK=0
  for folder in tests tests/golden tests/golden/ifcap "$C"; do
    if [ -L "$folder" ]; then
      add_problem "$folder: a symbolic link, not a folder (never followed)"
      return 0
    elif [ ! -e "$folder" ]; then
      add_problem "$folder: missing"
      return 0
    elif [ ! -d "$folder" ]; then
      add_problem "$folder: not a folder (not read)"
      return 0
    fi
  done
  if [ ! -r "$C" ] || [ ! -x "$C" ]; then
    add_problem "$C: folder cannot be listed"
    return 0
  fi
  ROOT_OK=1
}

# check_transcripts_listed: step 9, from what the walk of step 4 recorded, with no second walk.
# Every entry named transcript.txt under the corpus folder, of any kind and in any folder, a
# manifest case's or not, must have its path relative to the corpus folder among the paths of
# the SHA256SUMS lines that passed the line rules of step 8 (LISTED). A folder whose entries
# could not all be listed may hide one, so each is reported here as well.
check_transcripts_listed() {
  local i=0 path rel
  while [ "$i" -lt "$TRANSCRIPT_COUNT" ]; do
    path="${TRANSCRIPT_PATHS[$i]}"
    i=$((i + 1))
    rel="${path#"$C/"}"
    # A path that holds a newline is never listed, whatever lines its parts match.
    case "$rel" in
      *"$NL"*) ;;
      *)
        case "$LISTED" in
          *"$NL$rel$NL"*) continue ;;
        esac
        ;;
    esac
    add_problem "$path: not listed in SHA256SUMS"
  done
  i=0
  while [ "$i" -lt "$UNLISTED_COUNT" ]; do
    add_problem "${UNLISTED_CAUSES[$i]}, so not every transcript in it could be checked"
    i=$((i + 1))
  done
}

# check_case_budgets: step 3's budget and UTF-8 rules of the corpus reader, for each manifest
# case's input.txt and transcript.txt that is a regular file reached through no symbolic link
# (its case folder and itself both not links); any other is reported by the presence checks of
# step 3 or by step 4, and never opened. Each must hold at most CASE_FILE_LIMIT bytes, and each
# within budget must be UTF-8. One wc -c over all of them gives the sizes, and one grep over those
# within budget names each that holds a line that is not UTF-8, so the step starts two processes
# whatever the number of cases. The paths are built from ids that passed is_case_id, so they hold
# no space, newline or pattern character, and wc prints <count> <path> for each, in order, then a
# total line. When wc or grep fails, or wc's lines are not those, each file is checked on its own,
# so that a failure is put on the file that caused it. CASE_FILES_CHECKED gives the number of
# files checked. Each file within budget is added to IN_BUDGET, whether or not it is UTF-8.
check_case_budgets() {
  local id name path line count out rest st i=0 j=0 n=0 m=0
  CASE_PATHS=()
  CASE_SIZES=()
  BUDGET_PATHS=()
  while [ "$i" -lt "$N" ]; do
    id="${VALID_IDS[$i]}"
    i=$((i + 1))
    [ ! -L "$C/$id" ] && [ -d "$C/$id" ] || continue
    for name in input.txt transcript.txt; do
      path="$C/$id/$name"
      [ ! -L "$path" ] && [ -f "$path" ] || continue
      CASE_PATHS[n]="$path"
      n=$((n + 1))
    done
  done
  CASE_FILES_CHECKED=$n
  # The arrays are expanded whole only when they hold a path, since bash 3.2 under set -u rejects
  # the expansion of an empty array.
  [ "$n" -gt 0 ] || return 0
  out="$(LC_ALL=C wc -c "${CASE_PATHS[@]}" 2>/dev/null)"
  st=$?
  if [ "$st" -eq 0 ]; then
    # One line at a time, cut from the front of what is left, with no word splitting.
    rest="$out"
    while [ -n "$rest" ]; do
      line="${rest%%"$NL"*}"
      case "$rest" in
        *"$NL"*) rest="${rest#*"$NL"}" ;;
        *) rest="" ;;
      esac
      # <count> <path>, the count padded with leading spaces.
      line="${line#"${line%%[! ]*}"}"
      count="${line%% *}"
      path="${line#* }"
      path="${path#"${path%%[! ]*}"}"
      if [ "$j" -lt "$n" ] && [ "$path" = "${CASE_PATHS[$j]}" ]; then
        case "$count" in
          "" | *[!0123456789]*) st=1; break ;;
        esac
        CASE_SIZES[j]="$count"
        j=$((j + 1))
      elif [ "$j" -ne "$n" ] || [ "$path" != total ]; then
        st=1
        break
      fi
    done
    [ "$j" -eq "$n" ] || st=1
  fi
  i=0
  if [ "$st" -ne 0 ]; then
    while [ "$i" -lt "$n" ]; do
      check_text_budget "${CASE_PATHS[$i]}" "$CASE_FILE_LIMIT"
      i=$((i + 1))
    done
    return 0
  fi
  while [ "$i" -lt "$n" ]; do
    if [ "${CASE_SIZES[$i]}" -gt "$CASE_FILE_LIMIT" ]; then
      add_problem "${CASE_PATHS[$i]}: larger than $CASE_FILE_LIMIT bytes"
    else
      BUDGET_PATHS[m]="${CASE_PATHS[$i]}"
      m=$((m + 1))
      IN_BUDGET="$IN_BUDGET${CASE_PATHS[$i]}$NL"
    fi
    i=$((i + 1))
  done
  [ "$m" -gt 0 ] || return 0
  # -v -x -l: the name of each file holding a line that UTF8_LINE does not match as a whole.
  out="$(LC_ALL=C grep -a -l -v -x -E -e "$UTF8_LINE" "${BUDGET_PATHS[@]}" 2>/dev/null)"
  st=$?
  if [ "$st" -le 1 ]; then
    rest="$out"
    while [ -n "$rest" ]; do
      line="${rest%%"$NL"*}"
      case "$rest" in
        *"$NL"*) rest="${rest#*"$NL"}" ;;
        *) rest="" ;;
      esac
      [ -z "$line" ] || add_problem "$line: not UTF-8"
    done
    return 0
  fi
  i=0
  while [ "$i" -lt "$m" ]; do
    utf8_status "${BUDGET_PATHS[$i]}"
    case "$?" in
      1) add_problem "${BUDGET_PATHS[$i]}: not UTF-8" ;;
      2) add_problem "${BUDGET_PATHS[$i]}: cannot be read" ;;
    esac
    i=$((i + 1))
  done
}

# check_meta FILE: step 6, the rules of golden.rs parse_corpus_meta for meta.txt, each breach
# collected rather than the first only. FILE is read within TEXT_LIMIT and must be UTF-8. A line
# is_blank_line finds blank is skipped. Any other must hold =, and its key, the text before the
# first =, must be one of the five corpus meta keys, appearing once. Each of the five must
# appear. Values are not checked, as golden.rs keeps them exactly as written.
check_meta() {
  local file="$1" n=0 line key seen=" "
  if ! read_lines "$file" "$TEXT_LIMIT"; then
    add_problem "$file: $READ_FAULT"
    return 0
  fi
  utf8_status "$file"
  case "$?" in
    1) add_problem "$file: not UTF-8" ;;
    2) add_problem "$file: cannot be read" ;;
  esac
  while [ "$n" -lt "$FILE_LINE_COUNT" ]; do
    line="${FILE_LINES[$n]}"
    n=$((n + 1))
    is_blank_line "$line" && continue
    case "$line" in
      *=*) key="${line%%=*}" ;;
      *) add_problem "$file: line $n: no '='"; continue ;;
    esac
    case "$key" in
      image_digest | snapshot_id | source_revision | context | start_command) ;;
      *) add_problem "$file: line $n: unknown key $key"; continue ;;
    esac
    case "$seen" in
      *" $key "*) add_problem "$file: line $n: repeated key $key"; continue ;;
    esac
    seen="$seen$key "
  done
  for key in image_digest snapshot_id source_revision context start_command; do
    case "$seen" in
      *" $key "*) ;;
      *) add_problem "$file: missing key $key" ;;
    esac
  done
}

# check_run_summary FILE: step 7, the rules of golden.rs parse_run_summary for run-summary.txt,
# with the manifest's valid ids (VALID) as the cases, each breach collected rather than the first
# only. FILE is read within TEXT_LIMIT and must be UTF-8. A line is_blank_line finds blank is
# skipped. Every other line is tested in golden.rs's order:
#   - exports= and runs=: each once, its value exactly IDENTICAL;
#   - export_sha256=: once, its value 64 lower-case hex digits;
#   - DIFF<TAB><subject>: a difference record_corpus.sh collected, which it writes only beside a
#     DIFFERENT identity line; its subject must be exactly export or a manifest case;
#   - any other line holding a tab: <id><TAB><status>, then the two exit statuses, which are
#     evidence only and never read; the id is a manifest case listed once, and the status is
#     exactly RECORDED or QUARANTINED;
#   - anything else is not recognised.
# Each of the three identity lines, and a case line for each manifest case, must appear. When the
# first exports= and the first runs= are both IDENTICAL, the first DIFF line with a valid subject
# contradicts them and is a problem naming its line; beside a DIFFERENT one, in either line
# order, the identity line's own problem stands alone.
check_run_summary() {
  local file="$1" n=0 i=0 line value id status exports=0 runs=0 export_sha=0 cases=" "
  local identical_exports=0 identical_runs=0 first_diff=0
  if ! read_lines "$file" "$TEXT_LIMIT"; then
    add_problem "$file: $READ_FAULT"
    return 0
  fi
  utf8_status "$file"
  case "$?" in
    1) add_problem "$file: not UTF-8" ;;
    2) add_problem "$file: cannot be read" ;;
  esac
  while [ "$n" -lt "$FILE_LINE_COUNT" ]; do
    line="${FILE_LINES[$n]}"
    n=$((n + 1))
    is_blank_line "$line" && continue
    case "$line" in
      exports=*)
        value="${line#exports=}"
        if [ "$exports" -eq 1 ]; then
          add_problem "$file: line $n: repeated exports="
        else
          exports=1
          if [ "$value" = IDENTICAL ]; then
            identical_exports=1
          else
            add_problem "$file: exports=$value"
          fi
        fi
        ;;
      runs=*)
        value="${line#runs=}"
        if [ "$runs" -eq 1 ]; then
          add_problem "$file: line $n: repeated runs="
        else
          runs=1
          if [ "$value" = IDENTICAL ]; then
            identical_runs=1
          else
            add_problem "$file: runs=$value"
          fi
        fi
        ;;
      export_sha256=*)
        value="${line#export_sha256=}"
        if [ "$export_sha" -eq 1 ]; then
          add_problem "$file: line $n: repeated export_sha256="
        else
          export_sha=1
          case "$value" in
            *[!0123456789abcdef]*) value="" ;;
          esac
          [ "${#value}" -eq 64 ] || add_problem "$file: line $n: export_sha256 is not 64 lower-case hex digits"
        fi
        ;;
      "DIFF$TAB"*)
        value="${line#"DIFF$TAB"}"
        if [ "$value" != export ] && ! in_manifest "$value"; then
          add_problem "$file: line $n: DIFF names neither the export nor a case: $value"
        elif [ "$first_diff" -eq 0 ]; then
          first_diff="$n"
        fi
        ;;
      *"$TAB"*)
        id="${line%%"$TAB"*}"
        # The text after the first tab, cut at the next tab: taken by the length of id and one
        # replacement, so its time grows in proportion to the length of the line.
        status="${line:${#id}+1}"
        status="${status/"$TAB"*/}"
        if ! in_manifest "$id"; then
          add_problem "$file: line $n: unexpected case $id"
          continue
        fi
        case "$cases" in
          *" $id "*) add_problem "$file: line $n: repeated case $id"; continue ;;
        esac
        cases="$cases$id "
        case "$status" in
          RECORDED | QUARANTINED) ;;
          *) add_problem "$file: line $n: status $status is neither RECORDED nor QUARANTINED" ;;
        esac
        ;;
      *) add_problem "$file: line $n: not recognised" ;;
    esac
  done
  [ "$exports" -eq 1 ] || add_problem "$file: missing exports= line"
  [ "$runs" -eq 1 ] || add_problem "$file: missing runs= line"
  if [ "$identical_exports" -eq 1 ] && [ "$identical_runs" -eq 1 ] && [ "$first_diff" -gt 0 ]; then
    add_problem "$file: line $first_diff: DIFF contradicts exports=IDENTICAL and runs=IDENTICAL"
  fi
  [ "$export_sha" -eq 1 ] || add_problem "$file: missing export_sha256= line"
  while [ "$i" -lt "$N" ]; do
    id="${VALID_IDS[$i]}"
    i=$((i + 1))
    case "$cases" in
      *" $id "*) ;;
      *) add_problem "$file: missing case $id" ;;
    esac
  done
}

cd "$(dirname "$0")/../../../../.." || { echo "FAIL: cannot change to the crate folder five levels above $(dirname "$0")"; exit 1; }
[ -f Cargo.toml ] && [ -f tests/golden/ifcap/README.md ] || { echo "FAIL: run from the crate's corpus scripts folder"; exit 1; }

C=tests/golden/ifcap/corpus
MANIFEST="$C/manifest.tsv"
# The line every later step prints instead of its checks when step 1 failed.
ROOT_SKIP="skipped: the corpus folder failed its checks (counted in step 1)"

step "1. Corpus folder"
STEP_K=$K
check_root
step_result

step "2. Manifest"
STEP_K=$K
# VALID holds the valid case ids, space separated, in manifest order; later steps walk it.
# VALID_IDS holds the same ids indexed from 0, N of them.
VALID=""
VALID_IDS=()
N=0
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
elif [ -L "$MANIFEST" ] || { [ -e "$MANIFEST" ] && [ ! -f "$MANIFEST" ]; }; then
  add_problem "$MANIFEST: not a regular file (not read)"
elif [ ! -e "$MANIFEST" ]; then
  add_problem "$MANIFEST: missing"
else
  read_lines "$MANIFEST" "$MANIFEST_LIMIT"
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
      # The text before the first tab, or the whole line, by one replacement, whose time grows
      # in proportion to the length of the line, whatever it holds.
      id="${line/"$TAB"*/}"
      if [ "$n" -eq 1 ]; then
        # The header's first two fields are case and class: the line is case<TAB>class, alone or
        # followed by a tab. Fixed-length comparisons test it, so a long first line costs no
        # search.
        if [ "$line" != "case${TAB}class" ] && [ "${line:0:11}" != "case${TAB}class${TAB}" ]; then
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
      VALID_IDS[N]="$id"
      N=$((N + 1))
    done
    # An empty file has no header line either.
    [ "$n" -gt 0 ] || add_problem "$MANIFEST: line 1 is not the header case<TAB>class<TAB>..."
    # A manifest with no case would otherwise pass with nothing checked.
    [ "$N" -gt 0 ] || add_problem "$MANIFEST: lists no case"
    # Every corpus id is listed: each one the manifest does not list is reported once, in the
    # report order of golden::corpus_case_ids(), so that a manifest of only some of the cases
    # cannot pass. The ids are made here, not read, and VALID holds at most those 231, each once.
    i=0
    while [ "$i" -lt 231 ]; do
      i=$((i + 1))
      if [ "$i" -le 200 ]; then
        printf -v id 'CP-%03d' "$i"
      elif [ "$i" -le 226 ]; then
        printf -v id 'CE-%02d' "$((i - 200))"
      else
        printf -v id 'CR-%02d' "$((i - 226))"
      fi
      case "$VALID " in
        *" $id "*) ;;
        *) add_problem "$MANIFEST: corpus case $id not listed" ;;
      esac
    done
  fi
fi
echo "cases: $N (corpus_gen writes 231)"
step_result

step "3. Case files"
STEP_K=$K
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
else
  i=0
  while [ "$i" -lt "$N" ]; do
    id="${VALID_IDS[$i]}"
    i=$((i + 1))
    [ -f "$C/$id/input.txt" ] || add_problem "$C/$id/input.txt: missing"
    test -s "$C/$id/transcript.txt" || add_problem "$C/$id/transcript.txt: missing or empty"
  done
  check_case_budgets
  echo "case files checked against the $CASE_FILE_LIMIT-byte budget: $CASE_FILES_CHECKED"
fi
step_result

step "4. Inventory"
STEP_K=$K
# The one walk of the corpus folder. Every entry under it is visited, whatever the manifest lists:
# each regular file is searched for a carriage return once it is known to be within its budget,
# because step 2 or 3 measured it within it (IN_BUDGET) or because wc -c measures it here within
# TEXT_LIMIT, for a run file or BASE.txt, or SCAN_LIMIT, for any other. A file over it, or one
# that cannot be measured, is reported as not searched, and FILES counts only the files searched.
# Anything but a folder or a regular file is reported, never followed or opened, as is a folder
# whose entries cannot all be listed. Step 9 reuses what the walk records, so the folder is walked
# once: TRANSCRIPT_PATHS holds the path of every entry named transcript.txt, of any kind and in
# any folder, and UNLISTED_CAUSES the cause of every folder not listed in full, TRANSCRIPT_COUNT
# and UNLISTED_COUNT entries each. Both are bounded by the entry budget of the walk.
FILES=0
TRANSCRIPT_PATHS=()
TRANSCRIPT_COUNT=0
UNLISTED_CAUSES=()
UNLISTED_COUNT=0
WALKED=0
inventory_visit() {
  local limit
  if [ "$1" = unlisted ]; then
    add_problem "$3"
    UNLISTED_CAUSES[UNLISTED_COUNT]="$3"
    UNLISTED_COUNT=$((UNLISTED_COUNT + 1))
    return 0
  fi
  case "$2" in
    */transcript.txt)
      TRANSCRIPT_PATHS[TRANSCRIPT_COUNT]="$2"
      TRANSCRIPT_COUNT=$((TRANSCRIPT_COUNT + 1))
      ;;
  esac
  case "$1" in
    file)
      # Measured first unless a step already measured it within its own budget, so that grep
      # never reads a file past its budget, however large or sparse it is: TEXT_LIMIT for a run
      # file or BASE.txt, which later steps read within it, and SCAN_LIMIT for any other. The
      # message of wc is discarded, as that of grep is below, since the name is shown only through
      # add_problem, which escapes it, and the wc of macOS prints a name it cannot open raw.
      if ! in_budget "$2"; then
        case "$2" in
          "$C/meta.txt" | "$C/run-summary.txt" | "$C/SHA256SUMS" | "$C/BASE.txt")
            limit="$TEXT_LIMIT"
            ;;
          *) limit="$SCAN_LIMIT" ;;
        esac
        if ! file_size "$2" 2>/dev/null; then
          add_problem "$2: $SIZE_FAULT, so not searched for a carriage return"
          return 0
        fi
        if [ "$SIZE" -gt "$limit" ]; then
          add_problem "$2: larger than $limit bytes (not searched for a carriage return)"
          return 0
        fi
      fi
      FILES=$((FILES + 1))
      LC_ALL=C grep -l $'\r' "$2" >/dev/null 2>&1
      case "$?" in
        0) add_problem "$2: carriage return (a pseudo-terminal was used?)" ;;
        1) ;;
        *) add_problem "$2: cannot be read" ;;
      esac
      ;;
    other) add_problem "$2: not a regular file or folder (never followed or read)" ;;
  esac
}
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
elif walk inventory_visit; then
  WALKED=1
  echo "files searched: $FILES"
  echo "entries counted: $((ENTRY_BUDGET - ENTRIES_LEFT)) of the $ENTRY_BUDGET-entry budget"
else
  # Step 1 found the folder listable, so it changed since.
  add_problem "$C: folder cannot be listed"
fi
step_result

step "5. Run files"
STEP_K=$K
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
else
  for name in meta.txt run-summary.txt SHA256SUMS; do
    if [ -L "$C/$name" ]; then
      add_problem "$C/$name: a symbolic link, not a regular file (never followed or read)"
    elif [ ! -e "$C/$name" ]; then
      add_problem "$C/$name: missing"
    elif [ ! -f "$C/$name" ]; then
      add_problem "$C/$name: not a regular file (not read)"
    fi
  done
fi
step_result

step "6. meta.txt"
STEP_K=$K
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
elif ! is_run_file "$C/meta.txt"; then
  echo "skipped: meta.txt is not a regular file (counted in step 5)"
else
  check_meta "$C/meta.txt"
fi
step_result

step "7. run-summary.txt"
STEP_K=$K
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
elif ! is_run_file "$C/run-summary.txt"; then
  echo "skipped: run-summary.txt is not a regular file (counted in step 5)"
else
  check_run_summary "$C/run-summary.txt"
fi
step_result

step "8. SHA256SUMS verification"
STEP_K=$K
SUMS="$C/SHA256SUMS"
# SUMS_READ becomes 1 once SHA256SUMS is read in full. LISTED then holds, for step 9, the path of
# every line that passed the line rules, each path between newlines.
SUMS_READ=0
LISTED="$NL"
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
elif ! is_run_file "$SUMS"; then
  echo "skipped: SHA256SUMS is not a regular file (counted in step 5)"
else
  read_lines "$SUMS" "$TEXT_LIMIT"
  st=$?
  if [ "$st" -ne 0 ]; then
    add_problem "$SUMS: $READ_FAULT"
  else
    SUMS_READ=1
    utf8_status "$SUMS"
    case "$?" in
      1) add_problem "$SUMS: not UTF-8" ;;
      2) add_problem "$SUMS: cannot be read" ;;
    esac
    # The line rules of golden.rs walk_sha256sums, each line checked here and none left to
    # shasum: a blank line is skipped, and any other is 64 lower-case hex digits, two spaces or
    # a space and *, and a path allowed_path accepts, listed once. Only a line that passes them
    # and names a regular file reached through no symbolic link is handed to shasum, on its
    # standard input, so no listing can make it open a path outside the corpus folder, a FIFO or
    # a device; and only once a check has measured that file within its own budget (IN_BUDGET),
    # so that shasum never hashes a file past its budget, however large or sparse it is. Both
    # hold only while the tree stays as checked, since shasum opens each path after its checks.
    n=0
    lines=0
    passed=0
    OK_LINES=""
    while [ "$n" -lt "$FILE_LINE_COUNT" ]; do
      line="${FILE_LINES[$n]}"
      n=$((n + 1))
      is_blank_line "$line" && continue
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
        # The corpus reader reads every listed file within its budget. Step 3 checks the case
        # files and steps 6 and 7 the run files, so BASE.txt alone is left, checked here before
        # it can be handed over.
        [ "$path" != BASE.txt ] || check_text_budget "$C/BASE.txt" "$TEXT_LIMIT"
        if in_budget "$C/$path"; then
          OK_LINES="$OK_LINES$line$NL"
          passed=$((passed + 1))
        else
          # Over its budget, not measurable or not read in full, as the check that measured it
          # reports, or not there when its check ran: reported here as well, and never hashed.
          case "$path" in
            */*) limit="$CASE_FILE_LIMIT" ;;
            *) limit="$TEXT_LIMIT" ;;
          esac
          add_problem "$C/$path: listed in SHA256SUMS but not hashed (not confirmed within its $limit-byte budget)"
        fi
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

step "9. Every transcript listed"
STEP_K=$K
if [ "$ROOT_OK" -ne 1 ]; then
  echo "$ROOT_SKIP"
elif [ "$SUMS_READ" -ne 1 ]; then
  if is_run_file "$SUMS"; then
    echo "skipped: SHA256SUMS was not read (counted in step 8)"
  else
    echo "skipped: SHA256SUMS is not a regular file (counted in step 5)"
  fi
elif [ "$WALKED" -ne 1 ]; then
  echo "skipped: the corpus folder cannot be listed (counted in step 4)"
else
  check_transcripts_listed
  echo "transcripts found: $TRANSCRIPT_COUNT"
fi
step_result

step "Result"
# The array is expanded only when it holds a line, since bash 3.2 under set -u rejects the
# expansion of an empty array.
[ "$K" -eq 0 ] || printf '%s\n' "${PROBLEM_LINES[@]}"
if [ "$K" -eq 0 ]; then
  echo "CORPUS CHECK PASS $N CASES"
  exit 0
fi
echo "CORPUS CHECK FAIL $K PROBLEMS"
exit 1
