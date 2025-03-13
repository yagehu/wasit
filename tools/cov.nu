#!/usr/bin/env nu

def main [path: string, runtime: string] {
    let cov_command = match $runtime {
        "wamr" => wamr_cov,
        "wasmedge" => wasmedge_cov,
        "wasmedge-wasi" => wasmedge_wasi_cov,
        "wasmtime" => wasmtime_cov,
        "wasmtime-wasi" => wasmtime_wasi_cov,
        _ => ( error make { msg: $"unknown runtime ($runtime)" } )
    }
    let rt = $cov_command.runtime
    let llvm_prefix = $env.LLVM? | default "/usr/local"
    let llvm_profdata = [$llvm_prefix, "bin", "llvm-profdata"] | path join
    let llvm_cov = [$llvm_prefix, "bin", "llvm-cov"] | path join
    let prof_raws = glob $"($path)/*"
        | sort
        | each { |run| [$run, "**", $"($rt)*.profraw"] | path join }
        | each { |p| glob $p }
        | flatten
        | save --force $"($rt)-profraws.txt";
    let profdata = $"($rt).profdata"
    ^$"($llvm_profdata)" merge -sparse -o $profdata $"--input-files=($rt)-profraws.txt"

    (
        ^$"($llvm_cov)" report
            --color
            --show-branch-summary
            ...$cov_command.options
            $"-instr-profile=($profdata)"
            $cov_command.target
    )
}

def wamr_cov [] {
    let os = (uname | get kernel-name | str downcase)

    {
        runtime: "wamr",
        target: $"runtimes/wasm-micro-runtime/product-mini/platforms/($os)/build/iwasm",
        options: [
        ]
    }
}

def wasmedge_cov [] {
    let bin = which "wasmedge" | first | get "path"
    let lib = ldd $bin | find "wasmedge" | ansi strip | split row " " | get 2 | path expand

    {
        runtime: "wasmedge",
        target: $lib,
        options: []
    }
}

def wasmedge_wasi_cov [] {
    let bin = which "wasmedge" | first | get "path"
    let lib = ldd $bin | find "wasmedge" | ansi strip | split row " " | get 2 | path expand

    {
        runtime: "wasmedge",
        target: $lib,
        options: [
            "-ignore-filename-regex=/spdlog/",
            "-ignore-filename-regex=/lld/",
            "-ignore-filename-regex=/aot/",
            "-ignore-filename-regex=/ast/",
            "-ignore-filename-regex=/common/",
            "-ignore-filename-regex=/driver/",
            "-ignore-filename-regex=/executor/",
            "-ignore-filename-regex=/experimental/",
            "-ignore-filename-regex=/host/mock/",
            "-ignore-filename-regex=/host/loader/",
            "-ignore-filename-regex=/host/po/",
            "-ignore-filename-regex=/loader/",
            "-ignore-filename-regex=/plugin/",
            "-ignore-filename-regex=/po/",
            "-ignore-filename-regex=/system/",
            "-ignore-filename-regex=/validator/",
            "-ignore-filename-regex=/vm/",
            "-ignore-filename-regex=/runtime/",
            "-ignore-filename-regex=/lib/api/",
        ]
    }
}

def wasmtime_cov [] {
    {
        runtime: "wasmtime",
        target: "runtimes/wasmtime/target/release/wasmtime",
        options: [
            "-Xdemangler=rustfilt"
        ]
    }
}

def wasmtime_wasi_cov [] {
    {
        runtime: "wasmtime",
        target: "runtimes/wasmtime/target/release/wasmtime",
        options: [
            "-Xdemangler=rustfilt"
            "-ignore-filename-regex=/rustlib/"
            "-ignore-filename-regex=/\\.cargo/registry"
            "-ignore-filename-regex=/src/commands"
            "-ignore-filename-regex=/wasmtime/target/release/build"
            "-ignore-filename-regex=/wasmtime/cranelift"
            "-ignore-filename-regex=/wasmtime/crates/cache"
            "-ignore-filename-regex=/wasmtime/crates/cli-flags"
            "-ignore-filename-regex=/wasmtime/crates/component-util"
            "-ignore-filename-regex=/wasmtime/crates/cranelift"
            "-ignore-filename-regex=/wasmtime/crates/explorer"
            "-ignore-filename-regex=/wasmtime/crates/fiber"
            "-ignore-filename-regex=/wasmtime/crates/jit/.*"
            "-ignore-filename-regex=/wasmtime/crates/jit-.*"
            "-ignore-filename-regex=/wasmtime/crates/environ"
            "-ignore-filename-regex=/wasmtime/crates/runtime"
            "-ignore-filename-regex=/wasmtime/crates/slab"
            "-ignore-filename-regex=/wasmtime/crates/types"
            "-ignore-filename-regex=/wasmtime/crates/wasmtime"
            "-ignore-filename-regex=/wasmtime/crates/wasi-config"
            "-ignore-filename-regex=/wasmtime/crates/wasi-nn"
            "-ignore-filename-regex=/wasmtime/crates/wasi-threads"
            "-ignore-filename-regex=/wasmtime/crates/wasi-keyvalue"
            "-ignore-filename-regex=/wasmtime/crates/wasi-http"
            "-ignore-filename-regex=/wasmtime/crates/winch"
            "-ignore-filename-regex=/wasmtime/crates/wiggle"
            "-ignore-filename-regex=/wasmtime/winch/codegen"
            "-ignore-filename-regex=/wasmtime/src"
            "-ignore-filename-regex=/wasmtime/crates/wast"
        ]
    }
}
