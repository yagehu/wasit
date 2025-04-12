#!/usr/bin/env nu

use std

use node.nu
use node-sancov.nu
use wamr.nu
use wamr-sancov.nu
use wasmer.nu
use wasmer-sancov.nu
use wasmedge.nu
use wasmedge-sancov.nu
use wasmtime.nu
use wasmtime-sancov.nu

def main [--clean, --cov, ...runtimes: string] {
    mut paths = []

    for $runtime in $runtimes {
        let valid = $runtime | str contains ":"

        if not $valid {
            error make { msg: "specify runtime as $NAME:$REPO" }
        }

        let i = $runtime | str index-of ':'
        let name = $runtime | str substring 0..($i - 1)
        let repo = $runtime | str substring ($i + 1)..
        let path = match $name {
            "node" => { node $repo --cov=$cov },
            "node-sancov" => { node-sancov $repo },
            "wamr" => { wamr $repo --clean=$clean --cov=$cov },
            "wamr-sancov" => { wamr-sancov $repo },
            "wasmer" => { wasmer $repo --clean=$clean --cov=$cov },
            "wasmer-sancov" => { wasmer-sancov $repo },
            "wasmedge" => { wasmedge $repo $env.LLVM_16 $env.LLD_16 --clean=$clean --cov=$cov },
            "wasmedge-sancov" => { wasmedge-sancov $repo $env.LLVM_16 $env.LLD_16 },
            "wasmtime" => { wasmtime $repo --clean=$clean --cov=$cov },
            "wasmtime-sancov" => { wasmtime-sancov $repo },
            _ => { error make { msg: "unknown build configuration" } }
        }

        $paths = ($paths | append $path)
    }

    let root = $env.FILE_PWD | path join ".." ".."
    let root = $root | path expand
    let source_prefix = $root | path join "tools" "activate"

    cat (std null-device) out> $"($source_prefix).nu"
    cat (std null-device) out> $"($source_prefix).zsh"

    mut activate = '$env.path = (
    $env.path'
    mut activate_zsh = 'path=('

    for p in $paths {
        $activate = $"($activate)\n    | prepend ($p)"
        $activate_zsh = $"($activate_zsh)\n  ($p)"
    }

    $activate = $"($activate)\n)\n"
    $activate_zsh = $"($activate_zsh)
  $path
)

export PATH\n"

    $activate out> $"($source_prefix).nu"
    $activate_zsh out> $"($source_prefix).zsh"
}
