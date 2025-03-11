#!/usr/bin/env nu

use std

use wasmedge.nu
use wasmtime.nu

def main [...runtimes: string] {
    mut paths = []

    for $runtime in $runtimes {
        let valid = $runtime | str contains ":"

        if not $valid {
            error make { msg: "specify runtime as $NAME:$REPO" }
        }

        let i = $runtime | str index-of ':'
        let name = $runtime | str substring 0..($i - 1)
        let repo = $runtime | str substring ($i + 1)..

        match $name {
            "wasmedge" => {
                let $path = wasmedge $repo $env.LLVM_16 $env.LLD_16

                $paths = ($paths | append $path)
            }
            "wasmtime" => {
                let $path = wasmtime $repo

                $paths = ($paths | append $path)
            }
        }
    }

    let root = $env.FILE_PWD | path join ".." ".."
    let root = $root | path expand
    let source_prefix = $root | path join "tools" "activate"

    cat (std null-device) out> $"($source_prefix).nu"
    cat (std null-device) out> $"($source_prefix).zsh"

    mut activate = '$env.path = (
    $env.path'
    mut activate_zsh = 'path+=('

    for p in $paths {
        $activate = $"($activate)\n    | prepend ($p)"
        $activate_zsh = $"($activate_zsh)\n  ($p)"
    }

    $activate = $"($activate)\n)\n"
    $activate_zsh = $"($activate_zsh)\n)\nexport PATH"

    $activate out> $"($source_prefix).nu"
    $activate_zsh out> $"($source_prefix).zsh"
}
