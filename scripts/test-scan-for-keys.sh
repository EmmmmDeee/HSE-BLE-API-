#!/usr/bin/env bash
# test-scan-for-keys.sh — behavioural test for scripts/scan-for-keys.sh.
# Planted tokens are assembled at run time, so this file itself never contains a key-shaped string.
# Exit 0 only if every case behaves: findings exit 1, a clean scan exits 0, and every way the scan
# could silently scan nothing (missing tool, missing path, nothing to scan) exits 2 without "0 finding(s)".
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scanner="$here/scan-for-keys.sh"
bash_bin=$(command -v bash)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
fail=0
token="gh""p_$(printf 'A1b2C3d4E5%.0s' 1 2 3 4)"   # GitHub-token shape, 40 chars after the prefix

# run <name> <expected-exit> <env PATH or ""> <args...>; sets $out
run() {
  local name=$1 want=$2 path=$3; shift 3
  if [ -n "$path" ]; then out=$(PATH="$path" "$bash_bin" "$scanner" "$@" 2>&1); else out=$("$bash_bin" "$scanner" "$@" 2>&1); fi
  local got=$?
  if [ "$got" -ne "$want" ]; then echo "FAIL $name: exit $got, want $want"; printf '    %s\n' "$out"; fail=1; return 1; fi
  if [ "$want" -eq 2 ] && grep -q "finding(s)" <<<"$out"; then echo "FAIL $name: a scanner error still printed a finding count"; fail=1; return 1; fi
  echo "ok   $name (exit $got)"
}
# A PATH holding only the scanner's tools, minus the one named.
path_without() {
  local d="$work/bin-without-$1"; mkdir -p "$d"
  for t in find grep cat strings python3 mktemp rm; do
    [ "$t" = "$1" ] && continue
    ln -sf "$(command -v "$t")" "$d/$t"
  done
  echo "$d"
}

mkdir -p "$work/clean" "$work/text" "$work/bin" "$work/spaced dir" "$work/empty"
echo "hello world, nothing secret here" >"$work/clean/readme.txt"
printf '\x00\x01\x02plain binary payload\x00\xff' >"$work/clean/blob.bin"
echo "token=$token" >"$work/text/log.txt"
printf '\x00\x01%s\x00\xff\xfe' "$token" >"$work/bin/classes.dex"
echo "token=$token" >"$work/spaced dir/a log.txt"

if run "clean tree passes" 0 "" "$work/clean"; then
  if ! { grep -qx "key scan: 0 finding(s)" <<<"$out" && grep -qx "files scanned: 2" <<<"$out"; }; then
    echo "FAIL clean tree report: $out"; fail=1
  fi
fi
run "token in a text file is found"     1 "" "$work/text"
run "token in a binary is found"        1 "" "$work/bin"
run "token under a path with spaces"    1 "" "$work/spaced dir"
run "no strings on PATH fails closed"   2 "$(path_without strings)" "$work/clean"
run "no python3 on PATH fails closed"   2 "$(path_without python3)" "$work/clean"
run "no strings: planted binary token not reported clean" 2 "$(path_without strings)" "$work/bin"
run "zero files scanned fails closed"   2 "" "$work/empty"
run "missing path fails closed"         2 "" "$work/does-not-exist"
run "no arguments fails closed"         2 ""

[ "$fail" -eq 0 ] && echo "scan-for-keys: all cases passed" || echo "scan-for-keys: FAILED"
exit "$fail"
