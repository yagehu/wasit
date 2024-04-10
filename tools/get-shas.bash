#!/usr/bin/env bash

(
  cd runtimes/node
  echo "node:"
  git rev-parse HEAD
)

(
  cd runtimes/wasm-micro-runtime
  echo "wamr:"
  git rev-parse HEAD
)

(
  cd runtimes/WasmEdge
  echo "wasmedge:"
  git rev-parse HEAD
)

(
  cd runtimes/wasmer
  echo "wasmer:"
  git rev-parse HEAD
)

(
  cd runtimes/wasmtime
  echo "wasmtime:"
  git rev-parse HEAD
)