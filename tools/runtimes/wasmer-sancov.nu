#!/usr/bin/env nu

export def --env main [repo: path, --clean] -> path {
    let repo = $repo | path expand
    let target = (
        rustc +nightly -Z unstable-options --print target-spec-json
            | jq --raw-output '."llvm-target"'
    )

    if $clean {
        cargo -C $repo clean
    }

    do {
        $env.RUSTFLAGS = [
            "-Z sanitizer=address"
            "-C link-dead-code"
            "-C lto=no"
            "-C passes=sancov-module"
            "-C llvm-args=-sanitizer-coverage-inline-8bit-counters"
            "-C llvm-args=-sanitizer-coverage-level=1"
            "-C llvm-args=-sanitizer-coverage-pc-table"
            "-C llvm-args=-sanitizer-coverage-trace-pc-guard"
            "-C llvm-args=-sanitizer-coverage-prune-blocks=0"
        ] | str join " "

        (
            cargo -C $repo build
                -Z unstable-options
                --target $target
                --release
                --features cranelift,singlepass,wasmer-artifact-create,static-artifact-create,wasmer-artifact-load,static-artifact-load
                --manifest-path lib/cli/Cargo.toml
                --bin wasmer
                --locked
        )
    }

    let build_dir = $repo | path join  "target" $target "release"

    $env.path = ($env.path | prepend $build_dir)

    return $build_dir
}
