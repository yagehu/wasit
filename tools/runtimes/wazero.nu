#!/usr/bin/env nu

export def --env main [repo: path] {
    let repo = $repo | path expand

    do {
        go build -C $repo
    }

    let build_dir = $repo

    $env.path = ($env.path | prepend $build_dir)

    return $build_dir
}

