#!/usr/bin/env bash
# test-scan-for-keys.sh — behavioural test for scripts/scan-for-keys.sh.
# Planted tokens are assembled at run time, so this file itself never contains a key-shaped string.
# Exit 0 only if every case behaves: findings exit 1, a clean scan exits 0, and every way the scan
# could silently scan nothing (missing tool, missing path, nothing to scan, a grep or strings call that
# errors instead of answering, a broken or looping symlink) exits 2 without "0 finding(s)".
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scanner="$here/scan-for-keys.sh"
bash_bin=$(command -v bash)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
fail=0
token="gh""p_$(printf 'A1b2C3d4E5%.0s' 1 2 3 4)"   # GitHub-token shape, 40 chars after the prefix
# 34 distinct alphanumerics (H ~ 5.1 bits/char): caught only by the text-only entropy check, no token rule.
highent=$(printf '%s' {A..L} {a..l} {0..9})

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
# path_fails_on <tool> <args>: a PATH whose <tool> exits 2 (an error, not "no match") when its
# arguments start with <args>, and is the real tool otherwise.
path_fails_on() {
  local d="$work/bin-$1-fails-on${2// /}" real; real=$(command -v "$1"); mkdir -p "$d"
  cat >"$d/$1" <<WRAPPER
#!$bash_bin
case "\$*" in "$2"*) echo "$1: injected error" >&2; exit 2 ;; esac
exec "$real" "\$@"
WRAPPER
  chmod +x "$d/$1"
  echo "$d:$PATH"
}
# grep's calls: -Iq = the text/binary classification, -Eo = a rule match, -Evq = the allow-list.
path_grep_fails_on() { path_fails_on grep "$1"; }
# utf16 <le|be> <text>: <text> as UTF-16 (ASCII input), built byte by byte.
utf16() {
  local i
  for ((i = 0; i < ${#2}; i++)); do
    if [ "$1" = le ]; then printf '%s\0' "${2:i:1}"; else printf '\0%s' "${2:i:1}"; fi
  done
}

mkdir -p "$work/clean" "$work/text" "$work/bin" "$work/spaced dir" "$work/empty" "$work/entropy" "$work/many"
mkdir -p "$work/utf16le" "$work/utf16be" "$work/outside/linked dir" "$work/links" "$work/dirlink" "$work/broken" "$work/loop"
echo "hello world, nothing secret here" >"$work/clean/readme.txt"
printf '\x00\x01\x02plain binary payload\x00\xff' >"$work/clean/blob.bin"
echo "token=$token" >"$work/text/log.txt"
printf '\x00\x01%s\x00\xff\xfe' "$token" >"$work/bin/classes.dex"
echo "token=$token" >"$work/spaced dir/a log.txt"
echo "session=$highent" >"$work/entropy/session.log"
# > 64 KiB of rule matches: a "grep -Eo | grep -Evq" pipeline under pipefail lost this finding to SIGPIPE.
for _ in $(seq 3000); do echo "token=$token"; done >"$work/many/big.log"
# UTF-16 strings in a binary (the string-pool encoding of compiled Android XML): 7-bit strings misses them.
{ printf '\x00\x01\x02\xff'; utf16 le "token=$token"; printf '\x00\x00\xfe'; } >"$work/utf16le/AndroidManifest.xml"
{ printf '\x00\x01\x02\xff'; utf16 be "token=$token"; printf '\x00\x00\xfe'; } >"$work/utf16be/resources.arsc"
# Symlinks: to a token file from inside a directory, as an argument, to a directory, broken, looping.
echo "token=$token" >"$work/outside/secret.log"
echo "token=$token" >"$work/outside/linked dir/secret.log"
echo "nothing secret" >"$work/links/clean.txt"; ln -s ../outside/secret.log "$work/links/link.log"
ln -s outside/secret.log "$work/arglink.log"
echo "nothing secret" >"$work/dirlink/clean.txt"; ln -s "../outside/linked dir" "$work/dirlink/sub"
echo "nothing secret" >"$work/broken/clean.txt"; ln -s does-not-exist "$work/broken/dangling.log"
echo "nothing secret" >"$work/loop/clean.txt"; ln -s self.log "$work/loop/self.log"

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
run "high-entropy-only text secret is found" 1 "" "$work/entropy"
run "3000 copies of a token in one log are found" 1 "" "$work/many"
# grep status >= 2 is an error, never "binary" or "no match": each case must exit 2 with no finding count.
run "grep error classifying: high-entropy text secret fails closed" 2 "$(path_grep_fails_on -Iq)" "$work/entropy"
run "grep error classifying: planted text token fails closed"       2 "$(path_grep_fails_on -Iq)" "$work/text"
run "grep error matching: planted text token fails closed"          2 "$(path_grep_fails_on -Eo)" "$work/text"
run "grep error matching: planted binary token fails closed"        2 "$(path_grep_fails_on -Eo)" "$work/bin"
run "grep error matching: high-entropy text secret fails closed"    2 "$(path_grep_fails_on -Eo)" "$work/entropy"
run "grep error in allow-list: planted binary token fails closed"   2 "$(path_grep_fails_on -Evq)" "$work/bin"
run "UTF-16LE token in a binary is found"            1 "" "$work/utf16le"
run "UTF-16BE token in a binary is found"            1 "" "$work/utf16be"
run "strings -e l error fails closed"                2 "$(path_fails_on strings "-e l")" "$work/clean"
run "strings -e b error fails closed"                2 "$(path_fails_on strings "-e b")" "$work/clean"
run "symlink to a token file inside a dir is found"  1 "" "$work/links"
run "symlink to a token file as an argument is found" 1 "" "$work/arglink.log" "$work/clean/readme.txt"
run "symlink to a dir holding a token is found"      1 "" "$work/dirlink"
run "broken symlink inside a dir fails closed"       2 "" "$work/broken"
run "looping symlink inside a dir fails closed"      2 "" "$work/loop"

[ "$fail" -eq 0 ] && echo "scan-for-keys: all cases passed" || echo "scan-for-keys: FAILED"
exit "$fail"
