#/usr/bin/env bash

set -euxo pipefail

declare -a extra_paths=()

function join_by {
  local d="${1-}" f="${2-}"

  if shift 2; then
    printf %s "$f" "${@/#/$d}"
  fi
}

function build_wasmtime {
  local path="$1"

  (
    cd "$path"
    cargo build --release
  )

  extra_paths+=("$(realpath $path/target/release)")
}

function build_wamr {
  local repo_path="$1"
  local build_path="$repo_path/product-mini/platforms/linux/build"

  mkdir -p "$build_path"

  (
    cd "$build_path"
    cmake ..
    make
  )

  extra_paths+=("$(realpath $build_path)")
}

wasmtime_repo_dir="${WASMTIME_REPO_DIR:-runtimes/wasmtime}"
wamr_repo_dir="${WAMR_REPO_DIR:-runtimes/wasm-micro-runtime}"

build_wasmtime "$wasmtime_repo_dir"
build_wamr "$wamr_repo_dir"

extra_paths="$(join_by : ${extra_paths[*]})"
PATH="$extra_paths:$PATH" "$SHELL"

