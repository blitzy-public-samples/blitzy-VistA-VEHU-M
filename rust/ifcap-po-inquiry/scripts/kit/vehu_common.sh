# vehu_common.sh: shared helpers, sourced by the other scripts. Not run on its own.

vc_find_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    for d in /Applications/Docker.app/Contents/Resources/bin /usr/local/bin "$HOME/.docker/bin"; do
      [ -x "$d/docker" ] && { export PATH="$d:$PATH"; break; }
    done
  fi
  command -v docker >/dev/null 2>&1
}

# Platform flag: the VEHU image is Intel-only, so Apple silicon needs emulation.
vc_platform_args() {
  case "$(uname -m)" in arm64|aarch64) echo "--platform linux/amd64";; *) echo "";; esac
}

# The export scope of tests/golden/ifcap/README.md Part B step 1. One root per word,
# written without the closing parenthesis.
VC_SCOPE_ROOTS='^PRC(411 ^PRC(411.2 ^PRCD(420.5 ^PRCD(420.8 ^PRC(440 ^PRCD(442.3 ^PRC(442.4 ^PRCD(442.5 ^PRC(442.8 ^DIC(5 ^DIC(49 ^PRC(442 ^VA(200,.5 ^DIC(4,500'

# vc_mcmd "<roots>": the M command that dumps each root and everything under it.
# (ZWRITE of ^X(1) alone prints only that one node; ^X(1,*) prints the whole subtree.)
vc_mcmd() {
  local out="" r
  for r in $1; do
    case "$r" in
      *\(*) r="${r%)}"; r="$r,*)";;
    esac
    out="$out zwrite $r"
  done
  echo "${out# }"
}

# vc_wait_ready <container> <seconds>
vc_wait_ready() {
  local waited=0 o
  while [ "$waited" -lt "$2" ]; do
    o="$(docker exec "$1" su - vehu -c 'mumps -run %XCMD "write 1,!"' 2>/dev/null | tr -d '[:space:]')"
    [ "$o" = "1" ] && return 0
    sleep 5; waited=$((waited + 5))
  done
  return 1
}

# vc_detect_inquiry <container>: sets INQUIRY_CMD and INQUIRY_NOTE.
# The crate's start command (README Part B step 1) sources /home/vehu/etc/env and calls yottadb.
# If either is missing in this image, fall back to the login shell's mumps and say so.
vc_detect_inquiry() {
  local pre="" bin="yottadb"
  INQUIRY_NOTE=""
  if docker exec "$1" su - vehu -c 'test -f /home/vehu/etc/env' >/dev/null 2>&1; then pre='source /home/vehu/etc/env && '
  else INQUIRY_NOTE="no /home/vehu/etc/env in this image, so the env source was skipped. "; fi
  if ! docker exec "$1" su - vehu -c "${pre}command -v yottadb" >/dev/null 2>&1; then bin="mumps"; INQUIRY_NOTE="${INQUIRY_NOTE}yottadb not found, used mumps. "; fi
  INQUIRY_CMD="${pre}${bin} -run %XCMD \"S U=\\\$C(94),DTIME=300,DT=\\\$\\\$DT^XLFDT D DUZ^XUP(.5) D EN3^PRCHRPT\""
}
