#!/usr/bin/env nu

export def main [repo: path, --clean, --cov] {
    let repo = $repo | path expand
    let build_dir = $repo | path join "build"

    if $clean {
        cd $repo
        make clean
    }

    do {
        cd $repo

        $env.CC = "clang"
        $env.CXX = "clang++"

        if $cov {
            $env.CFLAGS = "-fprofile-instr-generate -fcoverage-mapping"
            $env.CXXFLAGS = "-fprofile-instr-generate -fcoverage-mapping"
            $env.LDFLAGS = "-fprofile-instr-generate -fcoverage-mapping -fuse-ld=lld"
        }

        ./configure --ninja

        make
    }

    $env.path = ($env.path | prepend $repo)

    return $repo
}
