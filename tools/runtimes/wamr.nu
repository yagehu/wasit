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
        $env.CFLAGS = "-fprofile-instr-generate -fcoverage-mapping"
        $env.CXXFLAGS = "-fprofile-instr-generate -fcoverage-mapping"
        $env.LDFLAGS = "-fprofile-instr-generate -fcoverage-mapping -fuse-ld=lld"

        cmake -DCMAKE_BUILD_TYPE=Release -B $build_dir -S $src_dir
        cmake --build $build_dir
    }

    $env.path = ($env.path | prepend $build_dir)

    return $build_dir
}
