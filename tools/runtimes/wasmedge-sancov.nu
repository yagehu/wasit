#!/usr/bin/env nu

export def --env main [repo: path, llvm_16: path, lld_16: path, --clean] -> path {
    let repo = $repo | path expand
    let build_dir = $repo | path join "build"

    if $clean {
        rm -rf $build_dir
    }

    mkdir $build_dir

    do {
        $env.CC = "clang"
        $env.CXX = "clang++"
        $env.LLVM_DIR = $llvm_16
        $env.LLD_DIR = $lld_16
        $env.CXXFLAGS = [
            "-fno-lto"
            "-fsanitize=address"
            "-fsanitize-coverage=bb,no-prune,trace-pc-guard,inline-8bit-counters,pc-table"
        ] | str join " "
        $env.LDFLAGS = "-fuse-ld=lld -Wl,--no-gc-sections" # Make sure we link dead code.

        cmake -DCMAKE_BUILD_TYPE=Debug -B $build_dir -S $repo

        # Patch in LLD headers
        let lld_include_dir = $lld_16 | path join "include" "lld"
        let cmake_include_dir = $build_dir | path join "include"
        cp -r $lld_include_dir $cmake_include_dir

        cmake --build $build_dir
    }

    let bin_dir = $"($build_dir)" | path join "tools" "wasmedge"

    $env.path = ($env.path | prepend $bin_dir)

    return $bin_dir
}
