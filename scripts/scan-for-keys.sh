#!/usr/bin/env bash
# scan-for-keys.sh <path>... — fail (exit 1) if any artifact or log contains a key-like string.
# Used by .github/workflows/release.yml on the exact artifact it publishes. It reports only the location and rule; it never prints the matched value.
# Fails closed (exit 2, and never prints "0 finding(s)") when it cannot actually scan: no path given, a
# path that does not exist, a required tool (strings, python3, …) missing, a file it cannot read or
# decode, a grep/strings/python3 call that errors (grep: status >= 2) rather than answering, a broken
# or looping symlink, or zero files scanned in total. A scan that silently scanned nothing must not pass as clean.
# Symlinks are followed (find -L), so a linked file or directory is scanned as its target.
# Binaries are scanned as 7-bit strings plus UTF-16LE and UTF-16BE strings: compiled Android XML
# (AndroidManifest.xml, <meta-data android:value=…>) keeps its string pool in UTF-16LE.
set -uo pipefail
# One byte = one character for grep's classes and ranges, whatever the runner's locale.
export LC_ALL=C
hits=0
scanned=0
die() { echo "::error::scan-for-keys: $1 — refusing to report a result (fail closed)" >&2; exit 2; }
report() { echo "::error file=$1::key-like content ($2) — value withheld"; hits=$((hits+1)); }
# matches <file> <rule> <regex>: report <rule> if <regex> matches $data outside the allow-list.
# grep's status 1 means "no match"; anything >= 2 is an error and must never be read as "no finding",
# so each grep runs on its own (no pipeline, whose status would hide the first grep's error).
matches() {
  local m rc
  m=$(grep -Eo -- "$3" <<<"$data"); rc=$?
  case $rc in 0) ;; 1) return 0 ;; *) die "grep failed (status $rc) matching rule '$2' on '$1'" ;; esac
  grep -Evq -- "$ALLOW_RE" <<<"$m"; rc=$?
  case $rc in 0) report "$1" "$2" ;; 1) ;; *) die "grep failed (status $rc) applying the allow-list on '$1'" ;; esac
}
# 0) Preconditions: every tool the scan depends on, and every path it was asked to scan.
for tool in find grep cat strings python3 mktemp; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool '$tool' not found on PATH"
done
python3 -c 'import math,re,sys,collections' >/dev/null 2>&1 || die "python3 cannot run the entropy check"
[ "$#" -gt 0 ] || die "no path to scan was given"
for p in "$@"; do [ -e "$p" ] || die "path '$p' does not exist"; done
# 1) Known provider token shapes.
PATTERNS=(
  '(^|[^A-Za-z0-9_-])sk-[A-Za-z0-9_-]{20,}'  # OpenAI-style (word boundary: not 'xtask-sdk-…')
  'sk-ant-[A-Za-z0-9_-]{20,}'        # Anthropic
  'AIza[0-9A-Za-z_-]{35}'            # Google API key
  'gh[pousr]_[A-Za-z0-9]{36,}'       # GitHub tokens
  'github_pat_[A-Za-z0-9_]{60,}'
  'xox[abprs]-[A-Za-z0-9-]{10,}'     # Slack
  'AKIA[0-9A-Z]{16}'                 # AWS access key id
  '-----BEGIN [A-Z ]*PRIVATE KEY-----'
)
# 2) Known HSE/Huntsman credential variable names, but only when they carry a non-placeholder value.
NAME_RE='HUNTSMAN_[A-Z0-9_]*(KEY|TOKEN|SECRET|USER|ID|GUID)[\"'"'"' ]*[:=][\"'"'"' ]*[A-Za-z0-9_./+-]{12,}'
ALLOW_RE='insert_[a-z0-9_]+_here|AKIAIOSFODNN7EXAMPLE|<value>|\$\{?[A-Z_]+'
# NUL-delimited, so a path with spaces or newlines is scanned whole instead of being word-split and skipped.
# -L follows symlinks, both as arguments and inside directories; find itself fails on a symlink or
# directory loop. Under -L only a dangling symlink is still -type l: it is listed so the loop exits 2 on it.
list=$(mktemp) || die "cannot create a temporary file"
trap 'rm -f "$list"' EXIT
find -L "$@" \( -type f -o -type l \) -print0 >"$list" || die "find failed on the given paths"
# The list is read once, in find's order (no sort, no second pass), on fd 3 so nothing in the loop body
# can consume entries from it; every entry either counts as scanned or exits 2.
while IFS= read -r -d '' -u 3 f; do
  if [ -L "$f" ] && [ ! -e "$f" ]; then die "broken symlink '$f'"; fi
  [ -f "$f" ] || die "'$f' is not a regular file"
  [ -r "$f" ] || die "cannot read '$f'"
  # binaries: scan the printable strings; text: scan as-is. grep -I: 0 = text, 1 = binary or empty,
  # >= 2 = error, which fails closed instead of silently skipping the text-only entropy check.
  grep -Iq . "$f"; rc=$?
  case $rc in
    0) text=1
       data=$(cat -- "$f") || die "'cat' failed on '$f'" ;;
    1) text=0
       # 7-bit, then 16-bit little-endian and big-endian strings; each must succeed.
       data=$(strings -n 8 -- "$f") || die "'strings' failed on '$f'"
       d16=$(strings -e l -n 8 -- "$f") || die "'strings -e l' (UTF-16LE) failed on '$f'"
       data+=$'\n'$d16
       d16=$(strings -e b -n 8 -- "$f") || die "'strings -e b' (UTF-16BE) failed on '$f'"
       data+=$'\n'$d16 ;;
    *) die "grep failed (status $rc) classifying '$f' as text or binary" ;;
  esac
  for p in "${PATTERNS[@]}"; do matches "$f" "$p" "$p"; done
  matches "$f" "HUNTSMAN_* credential assignment" "$NAME_RE"
  # 3) A high-entropy heuristic, for text logs/artifacts only. (This repository keeps no digest list of
  #    previously shipped credentials, so there is no digest check here.)
  if [ "$text" -eq 1 ]; then
    # Split on path/identifier separators and score each segment of 24+ chars that mixes upper,
    # lower and digits at >= 4.2 bits/char. That catches base64/alnum secrets but not paths, snake_case or hex digests.
    ents=$(python3 - "$f" <<'PY2'
import math,re,sys,collections
txt=open(sys.argv[1],errors="ignore").read()
for seg in set(re.findall(r"[A-Za-z0-9+]{24,}", txt)):
    if not (re.search(r"[A-Z]",seg) and re.search(r"[a-z]",seg) and re.search(r"[0-9]",seg)): continue
    c=collections.Counter(seg); n=len(seg); h=-sum(v/n*math.log2(v/n) for v in c.values())
    if h>=4.2: print(f"{h:.2f}")
PY2
) || die "the python3 entropy check failed on '$f'"
    while read -r ent; do [ -n "$ent" ] && report "$f" "high-entropy string (H=$ent)"; done <<<"$ents"
  fi
  scanned=$((scanned+1))
done 3<"$list"
[ "$scanned" -gt 0 ] || die "zero files were scanned"
echo "files scanned: $scanned"
echo "key scan: $hits finding(s)"
[ "$hits" -eq 0 ]
