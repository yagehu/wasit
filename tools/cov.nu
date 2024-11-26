#!/usr/bin/env nu

def main [path: string, runtime: string] {
    let cov_command = match $runtime {
        "wamr" => wamr_cov,
        "wasmtime" => wasmtime_cov,
        _ => ( error make { msg: $"unknown runtime ($runtime)" } )
    }
    let llvm_prefix = $env.LLVM | default "/usr/local"
    let llvm_profdata = [$llvm_prefix, "bin", "llvm-profdata"] | path join
    let llvm_cov = [$llvm_prefix, "bin", "llvm-cov"] | path join
    let prof_raws = glob $"($path)/*"
        | sort
        | each { |run| [$run, "runtimes", $runtime, "**", "*.profraw"] | path join }
        | each { |p| glob $p }
        | flatten
        | save --force $"($runtime)-profraws.txt";
    let profdata = $"($runtime).profdata"
    ^$"($llvm_profdata)" merge -sparse -o $profdata $"--input-files=($runtime)-profraws.txt"

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
        target: $"runtimes/wasm-micro-runtime/product-mini/platforms/($os)/build/iwasm",
        options: [
        ]
    }
}

def wasmtime_cov [] {
    {
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
