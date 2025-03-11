#!/usr/bin/env nu

export def --env main [repo: path, --clean] -> path {
    let repo = $repo | path expand

    do {
        $env.RUSTFLAGS = "-C instrument-coverage -Z coverage-options=branch"

        cargo +nightly -Z unstable-options -C $repo build --release
    }

    let build_dir = $repo | path join "target" "release"

    $env.path = ($env.path | prepend $build_dir)

    return $build_dir
}
