#!/usr/bin/env bash
# test-scan-for-keys.sh — behavioural test for scripts/scan-for-keys.sh.
# Planted tokens are assembled at run time, so this file itself never contains a key-shaped string.
# Exit 0 only if every case behaves: findings exit 1, a clean scan exits 0, and every way the scan
# could silently scan nothing (missing tool, missing path, nothing to scan, a grep or strings call that
# errors instead of answering, a broken or looping symlink) exits 2 without "0 finding(s)", and so does
# a symlink that resolves outside every scanned root (it must never be followed or read).
# No case may print a planted value; a finding names its path, plus " (resolved: <target>)" when it was
# reached through a symlink. Roots named -L, -H and -P are scanned as paths, never parsed as find options.
# Symlink fixtures live only inside this script's mktemp dir, and every case runs under timeout: a scan
# that wanders off (a link to / once walked the whole filesystem) fails the case instead of hanging.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scanner="$here/scan-for-keys.sh"
bash_bin=$(command -v bash)
timeout_bin=$(command -v timeout) || { echo "test-scan-for-keys: 'timeout' not found on PATH"; exit 1; }
limit=60   # seconds per case; the symlink cases below run under 10
work=$(mktemp -d) || exit 1; trap 'rm -rf "$work"' EXIT
work=$(cd "$work" && pwd -P) || exit 1   # physical, so the expected "(resolved: …)" paths below are exact
fail=0
token="gh""p_$(printf 'A1b2C3d4E5%.0s' 1 2 3 4)"   # GitHub-token shape, 40 chars after the prefix
# 34 distinct alphanumerics (H ~ 5.1 bits/char): caught only by the text-only entropy check, no token rule.
highent=$(printf '%s' {A..L} {a..l} {0..9})

# run <name> <expected-exit> <env PATH or ""> <args...>; sets $out. A case that outlives $limit seconds fails.
run() {
  local name=$1 want=$2 path=$3; shift 3
  if [ -n "$path" ]; then out=$(PATH="$path" "$timeout_bin" -k 5 "$limit" "$bash_bin" "$scanner" "$@" 2>&1)
  else out=$("$timeout_bin" -k 5 "$limit" "$bash_bin" "$scanner" "$@" 2>&1); fi
  local got=$?
  if [ "$got" -eq 124 ] || [ "$got" -eq 137 ]; then echo "FAIL $name: timed out after ${limit}s (exit $got), want $want"; fail=1; return 1; fi
  if [ "$got" -ne "$want" ]; then echo "FAIL $name: exit $got, want $want"; printf '    %s\n' "$out"; fail=1; return 1; fi
  if [ "$want" -eq 2 ] && grep -q "finding(s)" <<<"$out"; then echo "FAIL $name: a scanner error still printed a finding count"; fail=1; return 1; fi
  if grep -qF -e "$token" -e "$highent" <<<"$out"; then echo "FAIL $name: the output contains a planted value"; fail=1; return 1; fi
  echo "ok   $name (exit $got)"
}
# has <name> <text>... / lacks <name> <text>...: the last run's output must contain every <text> / none.
has() {
  local name=$1 t; shift
  for t in "$@"; do grep -qF -- "$t" <<<"$out" || { echo "FAIL $name: output lacks '$t'"; printf '    %s\n' "$out"; fail=1; }; done
}
lacks() {
  local name=$1 t; shift
  for t in "$@"; do ! grep -qF -- "$t" <<<"$out" || { echo "FAIL $name: output has '$t'"; printf '    %s\n' "$out"; fail=1; }; done
}
# A PATH holding only the scanner's tools, minus the one named.
path_without() {
  local d="$work/bin-without-$1"; mkdir -p "$d"
  for t in find grep cat strings python3 mktemp realpath rm; do
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
# Containment: a link may only resolve inside one of the scanned roots. Each escape below points at
# something the scan was not given (a planted token, /proc, /), so following it is the bug.
mkdir -p "$work/esc-proc" "$work/esc-file" "$work/pfx/root" "$work/pfx/root-evil" "$work/esc-slash"
mkdir -p "$work/esc-dir" "$work/rel/root/sub" "$work/rel/loot" "$work/inlink/root/real" "$work/argroot/real/inner"
mkdir -p "$work/dedup/root/data"
for d in esc-proc esc-file pfx/root esc-slash esc-dir rel/root inlink/root dedup/root; do echo "nothing secret" >"$work/$d/clean.txt"; done
ln -s /proc/self/environ "$work/esc-proc/env"
ln -s ../outside/secret.log "$work/esc-file/link.log"
echo "token=$token" >"$work/pfx/root-evil/secret.log"; ln -s ../root-evil "$work/pfx/root/evil"   # /…/root-evil is not under /…/root
ln -s / "$work/esc-slash/everything"
ln -s "../outside/linked dir" "$work/esc-dir/sub"
echo "token=$token" >"$work/rel/loot/secret.log"; ln -s ../../loot/secret.log "$work/rel/root/sub/up.log"
echo "token=$token" >"$work/inlink/root/real/secret.log"; ln -s real "$work/inlink/root/via"
# A root given as a symlink is resolved first: its inner link resolves under real/, not under the link's name.
echo "token=$token" >"$work/argroot/real/inner/secret.log"; ln -s ../real/inner "$work/argroot/real/peer"
ln -s argroot/real "$work/rootlink"
echo "token=$token" >"$work/dedup/root/data/secret.log"; ln -s data "$work/dedup/root/l1"; ln -s data "$work/dedup/root/l2"
# A link in one root to a token file in another: links/ is walked first, so the finding is the link's.
mkdir -p "$work/show/links" "$work/show/data"
echo "token=$token" >"$work/show/data/secret.log"; ln -s ../data/secret.log "$work/show/links/link.log"
# Roots named like find's options, scanned from dash/cwd: walking "." instead would reach out.log, a
# link outside it (exit 2), and sib/, next to decoy tokens in the parent and a sibling directory.
mkdir -p "$work/dash/cwd/-L" "$work/dash/cwd/-H" "$work/dash/cwd/-P" "$work/dash/sibling"
echo "token=$token" >"$work/dash/cwd/-L/secret.log"
echo "nothing secret" >"$work/dash/cwd/-H/clean.txt"; echo "nothing secret" >"$work/dash/cwd/-P/clean.txt"
echo "token=$token" >"$work/dash/decoy.log"; echo "token=$token" >"$work/dash/sibling/decoy.log"
ln -s ../decoy.log "$work/dash/cwd/out.log"; ln -s ../sibling "$work/dash/cwd/sib"

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
limit=10
# The links in links/ and dirlink/ point into outside/, so outside/ is scanned as a root too: a link
# between two scanned roots is inside. Scanned alone, the same kind of link is an escape (below).
run "symlink to a token file inside a dir is found"  1 "" "$work/links" "$work/outside"
run "symlink to a token file as an argument is found" 1 "" "$work/arglink.log" "$work/clean/readme.txt"
run "symlink to a dir holding a token is found"      1 "" "$work/dirlink" "$work/outside"
run "broken symlink inside a dir fails closed"       2 "" "$work/broken"
run "looping symlink inside a dir fails closed"      2 "" "$work/loop"
run "no realpath on PATH fails closed"               2 "$(path_without realpath)" "$work/clean"
run "symlink to /proc/self/environ fails closed"     2 "" "$work/esc-proc"
run "symlink to a file outside the roots fails closed" 2 "" "$work/esc-file"
run "symlink to a prefix sibling (root-evil) fails closed" 2 "" "$work/pfx/root"
# Quickly, and without walking anything: exiting 2 only after find -L has walked the whole filesystem
# (or hanging until the timeout) still fails this case.
if run "symlink to / fails closed quickly"           2 "" "$work/esc-slash"; then
  if grep -qF -- "$work/esc-slash/everything/" <<<"$out"; then echo "FAIL symlink to /: the scan walked beneath the link"; fail=1; fi
fi
run "symlink to a dir outside the roots fails closed" 2 "" "$work/esc-dir"
run "../.. relative symlink escape fails closed"     2 "" "$work/rel/root"
if run "symlink to a dir inside the root is scanned" 1 "" "$work/inlink/root"; then
  grep -qx "key scan: 1 finding(s)" <<<"$out" || { echo "FAIL inside-link report (want 1 finding):"; printf '    %s\n' "$out"; fail=1; }
fi
if run "symlink given as the root is resolved and scanned" 1 "" "$work/rootlink"; then
  grep -qx "key scan: 1 finding(s)" <<<"$out" || { echo "FAIL symlink-root report (want 1 finding):"; printf '    %s\n' "$out"; fail=1; }
fi
# One token, reached as data/ and through two links to it: one file, one finding (not three).
if run "a file reached through 2 links is scanned once" 1 "" "$work/dedup/root"; then
  if ! { grep -qx "key scan: 1 finding(s)" <<<"$out" && grep -qx "files scanned: 2" <<<"$out"; }; then
    echo "FAIL dedup report (want 2 files, 1 finding):"; printf '    %s\n' "$out"; fail=1
  fi
fi
# A finding names the path it was walked by, and the resolved path when a symlink was crossed.
if run "a symlink's finding names the link and its resolved target" 1 "" "$work/show/links" "$work/show/data"; then
  has "link finding" "$work/show/links/link.log (resolved: $work/show/data/secret.log): key-like content" "key scan: 1 finding(s)"
fi
if run "a finding reached through no symlink names the path alone" 1 "" "$work/text"; then
  has "plain finding" "::error file=$work/text/log.txt::$work/text/log.txt: key-like content"
  lacks "plain finding" "(resolved:"
fi
# rootlink -> argroot/real holds inner/secret.log and peer -> ../real/inner: either may be walked first.
if run "a symlink root's finding names the link path and its target" 1 "" "$work/rootlink"; then
  grep -qF -e "$work/rootlink/inner/secret.log (resolved: $work/argroot/real/inner/secret.log): key-like" \
    -e "$work/rootlink/peer/secret.log (resolved: $work/argroot/real/inner/secret.log): key-like" <<<"$out" ||
    { echo "FAIL symlink-root finding (want rootlink/…/secret.log (resolved: …/argroot/real/inner/secret.log)):"; printf '    %s\n' "$out"; fail=1; }
fi
# Relative roots are walked as ./<root> and reported as given.
prev=$PWD; cd "$work/dash/cwd" || exit 1
if run "a root named -L is a path: its token is found, . is not walked" 1 "" -L; then
  has "-L root" "::error file=-L/secret.log::-L/secret.log: key-like content" "files scanned: 1" "key scan: 1 finding(s)"
  lacks "-L root" "decoy" "out.log" "(resolved:"
fi
if run "roots named -H and -P are paths, not find options" 0 "" -H -P; then
  has "-H -P roots" "files scanned: 2" "key scan: 0 finding(s)"
fi
if run "roots named -L, -H and -P together scan only themselves" 1 "" -L -H -P; then
  has "-L -H -P roots" "files scanned: 3" "key scan: 1 finding(s)"
  lacks "-L -H -P roots" "decoy" "out.log"
fi
if run "a root given as ./-L is reported as given" 1 "" ./-L; then
  has "./-L root" "::error file=./-L/secret.log::./-L/secret.log: key-like content"
fi
run "an empty path argument fails closed (never walks .)" 2 "" ""
cd "$prev" || exit 1

[ "$fail" -eq 0 ] && echo "scan-for-keys: all cases passed" || echo "scan-for-keys: FAILED"
exit "$fail"
