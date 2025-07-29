# WASIT

## Overview

This artifact contains the source code and a container image
for SOSP'25 paper #248
"WASIT: Deep and Continuous Differential Testing of
WebAssembly System Interface Implementations".

WASIT was tested on
Linux (Fedora 40),
Darwin (Sequoia 15.5), and
Windows 11.
It requires no special hardware.
We tested on an x86_64 MacBook Pro, an arm64 Macbook Pro, and
a x86_64 server.

## Running on Linux/Darwin with Docker

We've provided arm64 and amd64 images:

- `container-img-amd64.tar.gz`
- `container-img-arm64.tar.gz`

First, load the provided container image and drop into its shell.
For example, for arm64 CPUs:

```
docker image load < container-img-arm64.tar.gz
docker run -it wasit:arm64 /bin/bash
```

The container has:

- a prebuilt `wazzi` binary
- prebuilt Wasm runtimes tested in the paper under `runtimes/`
- an annotated WASI specification `preview1.witx`

Within the container, run the fuzzer with a time limit (e.g. `10s`, `5m`, `2h`):

```
./wazzi configs/all.yaml workspace/ --time-limit 10s
```
The output of the fuzzer is stored in `workspace/`.
For example, `workspace/runs/0/progress` is a log of the first run.

This will run the fuzzer with no parallelism and stop after 10 seconds.
You can also run more parallel fuzzers with the `-c $COUNT` flag,
for example, `-c 8` will run 8 in parallel.

The `--strategy stateless` option will toggle on Syzkaller-like input
generation which is used to produce `WASIT-syzkaller` baseline results
in the paper.

## Building a container image


To build a new container image, you should obtain the sources for the
tested runtimes:

```
cd artifact/
mkdir runtimes
pushd runtimes
git clone https://github.com/yagehu/wasmtime
git clone https://github.com/yagehu/wasmer
git clone https://github.com/yagehu/WasmEdge
git clone https://github.com/yagehu/wasm-micro-runtime
git clone https://github.com/yagehu/wazero
popd
docker build -t wasit .
```

## Building from Source

Although we provide a prebuilt Linux container image,
you can build WASIT from source using the official Rust toolchain
by invoking Cargo.
The build requires:

- wasi-sdk (the `WASI_SDK` environment variable is mandatory)
- protobuf
- z3

```
WASI_SDK=$WASI_SDK_DIR cargo build --release
```

On Fedora 40, you may install protobuf and z3 with:

```
dnf install protobuf-devel z3-devel
```

On Darwin with Homebrew, you may install protobuf and z3 with:

```
brew install protobuf z3
```

## Running on Windows

It is possible to build WASIT and run on Windows.
Although getting all the runtimes built is non-trivial.

Steps to build WASIT:

1. Obtain prebuilt `wasi-sdk` from its GitHub release page
2. Install z3.
   Easiest way is to use the scoop package manager `scoop install z3`.
3. Install or build protobuf from source.

Here is a long one liner example that
sets up the environment variables for a user named `yagej`.

powershell -Command { $env:WASI_SDK="C:\Users\yagej\src\wasit\wasi-sdk-25.0-x86_64-windows"; $env:Protobuf_DIR="C:\Users\yagej\.local\protobuf\3.19"; $env:Z3_SYS_Z3_HEADER="C:\Users\yagej\scoop\apps\z3\current\include\z3.h"; $env:LIB="C:\Users\yagej\.local\z3\4.14.1\lib"; $env:Path+="C:\Users\yagej\.local\z3\4.14.1\bin"; $env:Path+=";C:\Users\yagej\src\wasit\runtimes\wasm-micro-runtime\product-mini\platforms\windows\build\Release"; $env:Path+=";C:\Users\yagej\src\wasit\runtimes\wasmtime\target\release"; cargo run --release -- -c 1 --strategy stateful --time-limit 5s .\configs\wamr-wasmtime.yaml .\workspace }

