#!/usr/bin/env bash
# check_captures.sh: pre-flight check for golden capture directories.
# Usage: check_captures.sh <golden_dir> [exit_input]
#   <golden_dir>  e.g. rust/ifcap-po-inquiry/tests/golden/ifcap
#                 or   rust/prosthetics-record-inquiry/tests/golden/prosthetics
#   [exit_input]  the workflow exit line, default ^
# Checks each GC-0N directory against the same READY definition used in the
# Refactor prompts. Prints READY or INCOMPLETE per case, with each failed check.
# Exit code 0 only when every case is READY. Reads files only; changes nothing.

set -u
dir="${1:-}"
exit_input="${2:-^}"
if [ -z "$dir" ] || [ ! -d "$dir" ]; then
  echo "usage: $0 <golden_dir> [exit_input]" >&2
  exit 2
fi

files="input.txt transcript.txt returned.txt db-before.zwr db-after.zwr meta.txt normalisation.txt"
keys="image_digest source_revision snapshot_id record_ids entry_point context docker_exec_args"
all_ready=1

for n in 01 02 03 04 05; do
  c="$dir/GC-$n"
  problems=()

  if [ ! -d "$c" ]; then
    echo "GC-$n: INCOMPLETE (directory missing)"
    all_ready=0
    continue
  fi

  for f in $files; do
    [ -f "$c/$f" ] || problems+=("missing file $f")
  done

  # carriage returns mean the capture ran with a pseudo-terminal
  for f in $files; do
    if [ -f "$c/$f" ] && LC_ALL=C grep -q $'\r' "$c/$f"; then
      problems+=("carriage return in $f (capture used -t?)")
    fi
  done

  if [ -f "$c/transcript.txt" ] && [ ! -s "$c/transcript.txt" ]; then
    problems+=("transcript.txt is empty")
  fi

  if [ -f "$c/meta.txt" ]; then
    for k in $keys; do
      grep -q "^$k=." "$c/meta.txt" || problems+=("meta.txt missing key $k")
    done
  fi

  if [ -s "$c/input.txt" ]; then
    last=$(tail -n 1 "$c/input.txt")
    [ "$last" = "$exit_input" ] || problems+=("input.txt last line is '$last', not exit input '$exit_input'")
  elif [ -f "$c/input.txt" ]; then
    problems+=("input.txt is empty")
  fi

  if [ -f "$c/normalisation.txt" ]; then
    ln=0
    while IFS= read -r line || [ -n "$line" ]; do
      ln=$((ln + 1))
      [ -z "$line" ] && continue
      tabs=$(printf '%s' "$line" | tr -cd '\t' | wc -c)
      open="${line%%$'\t'*}"
      close="${line#*$'\t'}"
      if [ "$tabs" -ne 1 ] || [ -z "$open" ] || [ -z "$close" ]; then
        problems+=("normalisation.txt line $ln needs one tab between two non-empty delimiters")
      fi
    done < "$c/normalisation.txt"
  fi

  if [ -f "$c/db-before.zwr" ] && [ -f "$c/db-after.zwr" ]; then
    cmp -s "$c/db-before.zwr" "$c/db-after.zwr" || problems+=("db-before.zwr and db-after.zwr differ")
  fi

  if [ ${#problems[@]} -eq 0 ]; then
    echo "GC-$n: READY"
  else
    all_ready=0
    echo "GC-$n: INCOMPLETE"
    for p in "${problems[@]}"; do echo "  - $p"; done
  fi
done

[ "$all_ready" -eq 1 ] && exit 0 || exit 1
