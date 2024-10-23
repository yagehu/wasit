(use-modules
  (guix packages)
  (gnu packages bash)
  (gnu packages certs)
  (gnu packages curl)
  (gnu packages gcc)
  (gnu packages shells)
  (gnu packages version-control)
)

;; Return a manifest containing that one package plus Git.
(specifications->manifest
  (list
    "autoconf"
    "automake"
    "bash"
    "coreutils"
    "clang"
    "cmake"
    "curl"
    "diffutils"
    "gawk"
    "gcc-toolchain"
    "git"
    "grep"
    "libtool"
    "make"
    "ninja"
    "nss-certs"
    "pkg-config"
    "python"
    "sed"
    "which"
    "z3"
    "zlib"
    "zsh"
  )
)
