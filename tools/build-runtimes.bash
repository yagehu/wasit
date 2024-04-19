#/usr/bin/env bash

set -euxo pipefail

declare -a extra_paths=()

os=""

function join_by {
  local d="${1-}" f="${2-}"

  if shift 2; then
    printf %s "$f" "${@/#/$d}"
  fi
}

function build_node {
  local repo_path="$1"

  (
    cd "$repo_path"
    ./configure
    make -j 4
  )

  extra_paths+=("$(realpath $repo_path)")
}

function build_wamr {
  local repo_path="$1"
  local build_path="$repo_path/product-mini/platforms/$os/build"

  mkdir -p "$build_path"

  (
    cd "$build_path"
    cmake ..
    make
  )

  extra_paths+=("$(realpath $build_path)")
}

function build_wasmedge {
  local repo_path="$1"
  local build_path="$repo_path/build"

  mkdir -p "$build_path"

  (
    cd "$build_path"
    cmake -DCMAKE_BUILD_TYPE=Release ..
    make -j
  )

  extra_paths+=("$(realpath $build_path/tools/wasmedge)")
}

function build_wasmer {
  local path="$1"

  (
    cd "$path"
    make build-wasmer
  )

  extra_paths+=("$(realpath $path/target/release)")
}

function build_wasmtime {
  local path="$1"

  (
    cd "$path"
    cargo build --release
  )

  extra_paths+=("$(realpath $path/target/release)")
}

case "$(uname -s)" in
  Linux*)  os=linux;;
  Darwin*) os=darwin;;
  *)
    echo "Unknown OS"
    exit 1
    ;;
esac

node_repo_dir="${NODE_REPO_DIR:-runtimes/node}"
wamr_repo_dir="${WAMR_REPO_DIR:-runtimes/wasm-micro-runtime}"
wasmedge_repo_dir="${WASMEDGE_REPO_DIR:-runtimes/WasmEdge}"
wasmer_repo_dir="${WASMER_REPO_DIR:-runtimes/wasmer}"
wasmtime_repo_dir="${WASMTIME_REPO_DIR:-runtimes/wasmtime}"

build_node "$node_repo_dir"
build_wamr "$wamr_repo_dir"
build_wasmedge "$wasmedge_repo_dir"
build_wasmer "$wasmer_repo_dir"
build_wasmtime "$wasmtime_repo_dir"

extra_paths="$(join_by : ${extra_paths[*]})"
PATH="$extra_paths:$PATH" "$SHELL"

