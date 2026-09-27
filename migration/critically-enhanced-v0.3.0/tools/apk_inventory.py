#!/usr/bin/env python3
import hashlib, pathlib, re, subprocess, sys, zipfile
apk = pathlib.Path(sys.argv[1])
print(f"apk={apk}")
print(f"sha256={hashlib.sha256(apk.read_bytes()).hexdigest()}")
with zipfile.ZipFile(apk) as z:
    names = sorted(z.namelist())
    print(f"entries={len(names)}")
    for n in names:
        if n.endswith('.dex') or n.startswith('lib/') or n == 'AndroidManifest.xml':
            print(n)
