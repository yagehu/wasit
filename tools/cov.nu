#!/usr/bin/env nu

def main [path: glob, runtime: string, --merge] {
    let cov_command = match $runtime {
        "node" => node_cov,
        "node-wasi" => node_wasi_cov,
        "wamr" => wamr_cov,
        "wasmedge" => wasmedge_cov,
        "wasmedge-wasi" => wasmedge_wasi_cov,
        "wasmtime" => wasmtime_cov,
        "wasmtime-wasi" => wasmtime_wasi_cov,
        "wasmtime-wasi-30" => wasmtime_wasi_30_cov,
        _ => ( error make { msg: $"unknown runtime ($runtime)" } )
    }
    let rt = $cov_command.runtime
    let llvm_prefix = $env.LLVM? | default "/usr/local"
    let llvm_profdata = [$llvm_prefix, "bin", "llvm-profdata"] | path join
    let llvm_cov = [$llvm_prefix, "bin", "llvm-cov"] | path join
    let prof_raws = ls $path
        | get "name"
        | save --force $"($rt)-profraws.txt";
    let profdata = $"($rt).profdata"

    if $merge {
        ^$"($llvm_profdata)" merge -sparse -o $profdata $"--input-files=($rt)-profraws.txt"
    }

    (
        ^$"($llvm_cov)" report
            --color
            --show-branch-summary
            ...$cov_command.options
            $"-instr-profile=($profdata)"
            $cov_command.target
    )

    (
        ^$"($llvm_cov)" show
            --show-branch-summary
            ...$cov_command.options
            $"-instr-profile=($profdata)"
            $cov_command.target
            out> $"($rt).cov"
    )
}

def node_cov [] {
    let bin = which "node" | first | get "path"

    {
        runtime: "node",
        target: $bin,
        options: []
    }
}

def node_wasi_cov [] {
    let bin = which "node" | first | get "path"

    {
        runtime: "node-wasi",
        target: $bin,
        options: [
            "-ignore-filename-regex=/out/",
            "-ignore-filename-regex=/deps/ada/",
            "-ignore-filename-regex=/deps/base64/",
            "-ignore-filename-regex=/deps/brotli/",
            "-ignore-filename-regex=/deps/cares/",
            "-ignore-filename-regex=/deps/histogram/",
            "-ignore-filename-regex=/deps/icu-small/",
            "-ignore-filename-regex=/deps/llhttp/",
            "-ignore-filename-regex=/deps/nghttp2/",
            "-ignore-filename-regex=/deps/ngtcp2/",
            "-ignore-filename-regex=/deps/openssl/",
            "-ignore-filename-regex=/deps/postject/",
            "-ignore-filename-regex=/deps/simdutf/",
            "-ignore-filename-regex=/deps/v8/",
            "-ignore-filename-regex=/deps/zlib/",
            "-ignore-filename-regex=/node/src/"
        ]
    }
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
        runtime: "wasmedge-wasi",
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
    let bin = which "wasmtime" | first | get "path"

    {
        runtime: "wasmtime",
        target: $bin,
        options: [
            "-Xdemangler=rustfilt"
        ]
    }
}

def wasmtime_wasi_cov [] {
    let bin = which "wasmtime" | first | get "path"

    {
        runtime: "wasmtime-wasi",
        target: $bin,
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

def wasmtime_wasi_30_cov [] {
    let bin = which "wasmtime" | first | get "path"

    {
        runtime: "wasmtime-wasi-30",
        target: $bin,
        options: [
            "-Xdemangler=rustfilt"
            "-ignore-filename-regex=/rustlib/"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/a.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/b.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/capstone-.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/clap.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/colorchoice-.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/cpp_demangle-.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/cpufeatures-.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/crc32fast-.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/crossbeam-.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/d.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/e.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/f.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/g.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/h.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/i.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/j.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/k.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/l.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/m.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/n.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/o.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/p.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/q.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/r.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/s.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/t.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/u.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/v.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/w.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/x.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/y.*"
            "-ignore-filename-regex=/\\.cargo/registry/src/index.crates.io-[0123456789abcdef]*/z.*"
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
            "-ignore-filename-regex=/wasmtime/crates/math"
            "-ignore-filename-regex=/wasmtime/crates/runtime"
            "-ignore-filename-regex=/wasmtime/crates/slab"
            "-ignore-filename-regex=/wasmtime/crates/types"
            "-ignore-filename-regex=/wasmtime/crates/wasmtime"
            "-ignore-filename-regex=/wasmtime/crates/wasi-common"
            "-ignore-filename-regex=/wasmtime/crates/wasi-config"
            "-ignore-filename-regex=/wasmtime/crates/wasi-nn"
            "-ignore-filename-regex=/wasmtime/crates/wasi-threads"
            "-ignore-filename-regex=/wasmtime/crates/wasi-keyvalue"
            "-ignore-filename-regex=/wasmtime/crates/wasi-http"
            "-ignore-filename-regex=/wasmtime/crates/winch"
            "-ignore-filename-regex=/wasmtime/crates/wiggle"
            "-ignore-filename-regex=/wasmtime/winch/codegen"
            "-ignore-filename-regex=/wasmtime/pulley/"
            "-ignore-filename-regex=/wasmtime/src"
            "-ignore-filename-regex=/wasmtime/crates/wast"
        ]
    }
}