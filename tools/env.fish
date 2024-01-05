#!/usr/bin/env fish

set extra_paths

# function setup_wasmer
#   set --function path $argv[1]
#   set --append extra_paths (realpath $path/target/release)
# end

function setup_wasmtime
  set --function path $argv[1]
  set --append extra_paths (realpath $path/target/release)
end

# function setup_wamr
#   set --function repo_path  $argv[1]
#   set --function build_path $repo_path/product-mini/platforms/linux/build

#   set --append extra_paths (realpath $build_path)
# end

# function setup_wasmedge
#   set --function repo_path  $argv[1]
#   set --function build_path $repo_path/build

#   set --append extra_paths (realpath $build_path/tools/wasmedge)
# end

# function setup_node
#   set --function repo_path $argv[1]

#   set --append extra_paths (realpath $repo_path)
# end

# function setup_wazero
#   set --function repo_path $argv[1]
#   set --append   extra_paths (realpath $repo_path)
# end

# function setup_wasmi
#   set --function repo_path $argv[1]
#   set --append    extra_paths (realpath $repo_path/target/release)
# end

# set wasmer_repo_dir   $_Z_DATA
set wasmtime_repo_dir $_Z_DATA
# set wamr_repo_dir     $_Z_DATA
# set wasmedge_repo_dir $_Z_DATA
# set node_repo_dir     $_Z_DATA
# set wazero_repo_dir   $_Z_DATA
# set wasmi_repo_dir    $_Z_DATA

# test -z $wasmer_repo_dir  ; and set wasmer_repo_dir   "runtimes/wasmer"
test -z $wasmtime_repo_dir; and set wasmtime_repo_dir "runtimes/wasmtime"
# test -z $wamr_repo_dir    ; and set wamr_repo_dir     "runtimes/wasm-micro-runtime"
# test -z $wasmedge_repo_dir; and set wasmedge_repo_dir "runtimes/WasmEdge"
# test -z $node_repo_dir    ; and set node_repo_dir     "runtimes/node"
# test -z $wazero_repo_dir  ; and set wazero_repo_dir   "runtimes/wazero"
# test -z $wasmi_repo_dir   ; and set wasmi_repo_dir    "runtimes/wasmi"

# setup_wasmer   "$wasmer_repo_dir"
setup_wasmtime "$wasmtime_repo_dir"
# setup_wamr     "$wamr_repo_dir"
# setup_wasmedge "$wasmedge_repo_dir"
# setup_node     "$node_repo_dir"
# setup_wazero   "$wazero_repo_dir"
# setup_wasmi    "$wasmi_repo_dir"

set extra_paths (string join : $extra_paths)
export PATH="$extra_paths:$PATH"
