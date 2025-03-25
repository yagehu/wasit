#!/usr/bin/env nu

export def --env main [repo: path] -> path {
    let repo = $repo | path expand
    let build_dir = $repo | path join "build"

    do {
        cd $repo

        $env.CC = "clang"
        $env.CXX = "clang++"
        $env.CFLAGS = [
            "-fno-lto"
            "-fsanitize=address"
            "-fsanitize-coverage=bb,no-prune,trace-pc-guard,inline-8bit-counters,pc-table"
        ] | str join " "
        $env.CXXFLAGS = $env.CFLAGS
        $env.LDFLAGS = "-fsanitize=address -fuse-ld=lld -Wl,--no-gc-sections" # Make sure we link dead code.

        ./configure --ninja

        make
    }

    $env.path = ($env.path | prepend $repo)

    return $repo
}
