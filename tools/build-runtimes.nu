#!/usr/bin/env nu

use std

let os = (uname | get kernel-name | str downcase)

let root = ($"($env.FILE_PWD)/.." | path expand)
let node_path = $"($root)/runtimes/node"
let wamr_path = $"($root)/runtimes/wasm-micro-runtime"
let wasmedge_path = $"($root)/runtimes/WasmEdge"
let wasmer_path = $"($root)/runtimes/wasmer"
let wasmtime_path = $"($root)/runtimes/wasmtime"
let wazero_path = $"($root)/runtimes/wazero"

def build_node [path: string] string {
    cd $path
    ./configure --ninja
    make

    $path
}

def build_wamr [path: string] string {
    let build_path = $"($path)/product-mini/platforms/($os)/build"

    mkdir $build_path
    cd $build_path
    cmake ..
    cmake --build .

    $build_path
}

def build_wasmedge [path: string] string {
    let build_path = $"($path)/build"
    let link_llvm_static = match $os {
        "darwin" => "ON",
        _ => "OFF",
    }

    mkdir $build_path
    cd $build_path
    cmake -DCMAKE_BUILD_TYPE=Release $"-DWASMEDGE_LINK_LLVM_STATIC=($link_llvm_static)" ..
    cmake --build .

    $"($build_path)/tools/wasmedge"
}

def build_wasmer [path: string] string {
    cd $path
    make build-wasmer

    $"($path)/target/release"
}

def build_wasmtime [path: string] string {
    cd $path
    cargo build --release

    $"($path)/target/release"
}

def build_wazero [path: string] string {
    cd $path
    CGO_ENABLED=0 go build ./cmd/wazero

    $path
}

def main [] {
    let paths = [
        (build_node $node_path),
        (build_wamr $wamr_path),
        (build_wasmedge $wasmedge_path),
        (build_wasmer $wasmer_path),
        (build_wasmtime $wasmtime_path),
        (build_wazero $wazero_path),
    ]
    let source_path = $"($root)/tools/activate.nu"

    cat (std null-device) out> $source_path

    mut activate = '$env.path = (
    $env.path'

    for p in $paths {
        $activate = $"($activate)\n    | prepend ($p)"
    }

    $activate = $"($activate)\n)\n"
    $activate out> $source_path
}
