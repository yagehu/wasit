#!/usr/bin/env nu

export def --env main [repo: path] -> path {
    let repo = $repo | path expand
    let build_dir = $repo | path join "build"

    do {
        cd $repo

        $env.CC = "clang"
        $env.CXX = "clang++"
        $env.CFLAGS = "-fprofile-instr-generate -fcoverage-mapping"
        $env.CXXFLAGS = "-fprofile-instr-generate -fcoverage-mapping"
        $env.LDFLAGS = "-fprofile-instr-generate -fcoverage-mapping -fuse-ld=lld"

        ./configure --ninja

        make
    }

    $env.path = ($env.path | prepend $repo)

    return $repo
}
