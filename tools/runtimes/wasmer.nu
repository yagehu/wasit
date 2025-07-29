#!/usr/bin/env nu

export def --env main [repo: path, --clean, --cov] {
    let repo = $repo | path expand

    if $clean {
        cargo +nightly -Z unstable-options -C $repo clean
    }

    do {
        if $cov {
            $env.RUSTFLAGS = "-C instrument-coverage -Z coverage-options=branch"
        }

        rustup override set --path $repo nightly
        cargo +nightly -Z unstable-options -C $repo build --manifest-path lib/cli/Cargo.toml --bin wasmer --release
    }

    let build_dir = $repo | path join "target" "release"

    $env.path = ($env.path | prepend $build_dir)

    return $build_dir
}
