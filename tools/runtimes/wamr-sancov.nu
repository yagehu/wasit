#!/usr/bin/env nu

export def --env main [repo: path, --clean] -> path {
    let repo = $repo | path expand
    let os = (uname | get kernel-name | str downcase)
    let src_dir = $repo | path join "product-mini" "platforms" $os
    let build_dir = $repo | path join $src_dir "build"

    if $clean {
        rm -rf $build_dir
    }

    mkdir $build_dir

    do {
        $env.CC = "clang"
        $env.CXX = "clang++"
        $env.CFLAGS = [
            "-fno-lto"
            "-fsanitize=address"
            "-fsanitize-coverage=bb,no-prune,trace-pc-guard,inline-8bit-counters,pc-table"
        ] | str join " "
        $env.CXXFLAGS = $env.CFLAGS
        $env.LDFLAGS = "-fuse-ld=lld -Wl,--no-gc-sections" # Make sure we link dead code.

        cmake -DCMAKE_BUILD_TYPE=Release -B $build_dir -S $src_dir
        cmake --build $build_dir
    }

    $env.path = ($env.path | prepend $build_dir)

    return $build_dir
}
