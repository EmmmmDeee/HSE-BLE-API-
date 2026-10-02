#!/usr/bin/env bash
# scan-for-keys.sh <path>... — fail (exit 1) if any artifact or log contains a key-like string.
# Used by .github/workflows/release.yml on the exact artifact it publishes. It reports only the location and rule; it never prints the matched value.
# Fails closed (exit 2, and never prints "0 finding(s)") when it cannot actually scan: no path given, a
# path that does not exist, a required tool (strings, python3, …) missing, a file it cannot read or
# decode, or zero files scanned in total. A scan that silently scanned nothing must not pass as clean.
set -uo pipefail
hits=0
scanned=0
die() { echo "::error::scan-for-keys: $1 — refusing to report a result (fail closed)" >&2; exit 2; }
report() { echo "::error file=$1::key-like content ($2) — value withheld"; hits=$((hits+1)); }
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
list=$(mktemp) || die "cannot create a temporary file"
trap 'rm -f "$list"' EXIT
find "$@" -type f -print0 >"$list" || die "find failed on the given paths"
while IFS= read -r -d '' f; do
  [ -r "$f" ] || die "cannot read '$f'"
  # binaries: scan the printable strings; text: scan as-is
  if grep -Iq . "$f" 2>/dev/null; then text=1; src=(cat -- "$f"); else text=0; src=(strings -n 8 -- "$f"); fi
  data=$("${src[@]}") || die "'${src[0]}' failed on '$f'"
  for p in "${PATTERNS[@]}"; do
    if grep -Eo -- "$p" <<<"$data" | grep -Evq -- "$ALLOW_RE"; then report "$f" "$p"; fi
  done
  if grep -Eo -- "$NAME_RE" <<<"$data" | grep -Evq -- "$ALLOW_RE"; then report "$f" "HUNTSMAN_* credential assignment"; fi
  # 3) Value digests of the credentials that earlier builds shipped (src/util/keys/constants.rs COMPROMISED_EMBEDDED_DIGESTS):
  #    a regression that re-embeds one shows up as its digest. (Done in the cargo test
  #    `no_credential_is_embedded_in_the_build`; here we add a high-entropy heuristic for text logs/artifacts only.)
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
done <"$list"
[ "$scanned" -gt 0 ] || die "zero files were scanned"
echo "files scanned: $scanned"
echo "key scan: $hits finding(s)"
[ "$hits" -eq 0 ]
