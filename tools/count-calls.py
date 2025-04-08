#!/usr/bin/env python3

import subprocess
import sys
from glob import glob

wasm_files = glob(sys.argv[1])
calls = 0

for wasm_file in wasm_files:
    wat = subprocess.run(["wasm2wat", "--enable-all", wasm_file], capture_output=True).stdout.decode("utf-8")

    for line in wat.splitlines():
        if not line.strip().startswith("call $__imported_wasi_snapshot_preview1_"):
            continue

        calls += 1

print(calls)
